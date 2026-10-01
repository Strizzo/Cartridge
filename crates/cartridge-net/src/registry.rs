use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const OFFICIAL_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/Strizzo/cartridge-apps/main/catalog.json";
pub const CATALOG_KEY_ID: &str = "cartridge-official-v1";
pub const CATALOG_PUBLIC_KEY: &str =
    "f5a7873122ed9a2bc262a4d3e7fbc32ab214d339735c989630ab2c4d56cdad1c";

/// Release metadata approved by the catalogue publisher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPackage {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    pub min_runtime: String,
}

use crate::client::HttpClient;

/// A single application entry in the Cartridge registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryApp {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub category: String,
    pub tags: Vec<String>,
    pub repo_url: String,
    pub permissions: Vec<String>,
    #[serde(default)]
    pub package: Option<AppPackage>,
}

/// The full registry payload (matches `registry.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub apps: Vec<RegistryApp>,
}

impl Registry {
    /// Return a sorted, deduplicated list of all categories present in the
    /// registry.
    pub fn get_categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = self.apps.iter().map(|a| a.category.clone()).collect();
        cats.sort();
        cats.dedup();
        cats
    }

    /// Return all apps that belong to `category`.
    pub fn filter_by_category(&self, category: &str) -> Vec<&RegistryApp> {
        self.apps
            .iter()
            .filter(|a| a.category == category)
            .collect()
    }
}

/// Client for fetching the Cartridge app registry over HTTP.
#[derive(Clone)]
pub struct RegistryClient {
    http: HttpClient,
    url: String,
}

impl RegistryClient {
    pub fn new(http: HttpClient, url: String) -> Self {
        Self { http, url }
    }

    /// Fetch and parse the registry. Uses a 5-minute cache by default.
    pub fn fetch(&self) -> Result<Registry, String> {
        let response = self.http.get_cached(&self.url, 300)?;
        if !response.ok {
            return Err(format!(
                "Registry request failed with status {}",
                response.status
            ));
        }

        verify_catalog(&response.body)
    }
}

/// The exact UTF-8 payload string is signed, avoiding JSON canonicalization differences.
/// Every network catalogue must use the pinned official key; plain local registry.json
/// remains available for bundled apps when offline.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedCatalog {
    key_id: String,
    payload: String,
    signature: String,
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.is_ascii() {
        return Err("Invalid catalogue signature encoding".into());
    }
    let mut result = [0; N];
    for (i, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .map_err(|_| "Invalid catalogue signature encoding")?;
    }
    Ok(result)
}

pub fn verify_catalog(body: &str) -> Result<Registry, String> {
    verify_catalog_with_key(body, &decode_hex::<32>(CATALOG_PUBLIC_KEY)?)
}

fn verify_catalog_with_key(body: &str, public_key: &[u8; 32]) -> Result<Registry, String> {
    let envelope: SignedCatalog = serde_json::from_str(body).map_err(|_| {
        "Store catalogue is unsigned or malformed; keeping the current catalogue".to_string()
    })?;
    if envelope.key_id != CATALOG_KEY_ID {
        return Err("Store catalogue uses an unknown signing key".into());
    }
    let key = VerifyingKey::from_bytes(public_key).map_err(|e| e.to_string())?;
    let signature = Signature::from_bytes(&decode_hex::<64>(&envelope.signature)?);
    key.verify_strict(envelope.payload.as_bytes(), &signature)
        .map_err(|_| "Store catalogue signature did not verify".to_string())?;
    let registry: Registry = serde_json::from_str(&envelope.payload)
        .map_err(|e| format!("Invalid signed catalogue: {e}"))?;
    if registry.version != 2 || registry.apps.len() > 1000 {
        return Err("Unsupported Store catalogue version or size".into());
    }
    let mut ids = HashSet::new();
    for app in &registry.apps {
        if app.id.is_empty()
            || app.id.len() > 128
            || !app
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
            || app.id.starts_with('.')
            || !ids.insert(&app.id)
        {
            return Err("Invalid or duplicate app identifier in catalogue".into());
        }
        let package = app
            .package
            .as_ref()
            .ok_or("Catalogue app is missing a verified package")?;
        semver::Version::parse(&app.version).map_err(|_| "Invalid app version")?;
        semver::Version::parse(&package.min_runtime)
            .map_err(|_| "Invalid minimum runtime version")?;
        decode_hex::<32>(&package.sha256).map_err(|_| "Invalid package checksum")?;
        if !package.url.starts_with("https://")
            || package.size == 0
            || package.size > 32 * 1024 * 1024
        {
            return Err("Invalid package URL or size".into());
        }
    }
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    use ed25519_dalek::{Signer, SigningKey};

    fn signed_fixture() -> (serde_json::Value, [u8; 32]) {
        let key = SigningKey::from_bytes(&[42; 32]);
        let payload = serde_json::json!({"version":2,"apps":[{
            "id":"dev.cartridge.test", "name":"Test", "description":"test", "version":"1.0.0",
            "author":"Test", "category":"tools", "tags":[], "repo_url":"https://github.com/example/test",
            "permissions":["storage"], "package":{"url":"https://example.org/test.tar.gz",
                "sha256":"00".repeat(32),"size":1024,"min_runtime":"0.6.0"}
        }]}).to_string();
        let signature = key
            .sign(payload.as_bytes())
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        (
            serde_json::json!({"key_id":CATALOG_KEY_ID,"payload":payload,"signature":signature}),
            key.verifying_key().to_bytes(),
        )
    }

    #[test]
    fn verifies_payload_and_rejects_tampering_wrong_key_and_unsigned_json() {
        let (envelope, key) = signed_fixture();
        assert_eq!(
            verify_catalog_with_key(&envelope.to_string(), &key)
                .unwrap()
                .apps
                .len(),
            1
        );
        let mut changed = envelope.clone();
        changed["payload"] = changed["payload"]
            .as_str()
            .unwrap()
            .replace("1.0.0", "9.0.0")
            .into();
        assert!(verify_catalog_with_key(&changed.to_string(), &key).is_err());
        assert!(verify_catalog(&envelope.to_string()).is_err());
        changed = envelope.clone();
        changed["key_id"] = "unknown".into();
        assert!(verify_catalog_with_key(&changed.to_string(), &key).is_err());
        assert!(verify_catalog(envelope["payload"].as_str().unwrap()).is_err());
        changed = envelope;
        changed["signature"] = "é".repeat(64).into();
        assert!(verify_catalog_with_key(&changed.to_string(), &key).is_err());
    }

    #[test]
    fn official_openssl_signed_catalogue_matches_pinned_runtime_key() {
        let registry =
            verify_catalog(include_str!("../tests/fixtures/official-catalog.json")).unwrap();
        assert_eq!(registry.apps.len(), 3);
        assert!(
            registry
                .apps
                .iter()
                .all(|app| app.package.as_ref().unwrap().min_runtime == "0.6.0")
        );
    }

    #[test]
    fn correctly_signed_invalid_schema_is_rejected() {
        let (mut envelope, key) = signed_fixture();
        let mut payload: serde_json::Value =
            serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
        let duplicate = payload["apps"][0].clone();
        payload["apps"].as_array_mut().unwrap().push(duplicate);
        let bytes = payload.to_string();
        let signature = SigningKey::from_bytes(&[42; 32])
            .sign(bytes.as_bytes())
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        envelope["payload"] = bytes.into();
        envelope["signature"] = signature.into();
        assert!(
            verify_catalog_with_key(&envelope.to_string(), &key)
                .unwrap_err()
                .contains("duplicate")
        );
    }

    #[test]
    fn parse_registry_json() {
        let json = r#"{
            "version": 1,
            "apps": [
                {
                    "id": "dev.cartridge.test",
                    "name": "Test App",
                    "description": "A test app",
                    "version": "0.1.0",
                    "author": "Test",
                    "category": "tools",
                    "tags": ["test"],
                    "repo_url": "https://github.com/test/test",
                    "permissions": ["network"]
                },
                {
                    "id": "dev.cartridge.news",
                    "name": "News",
                    "description": "A news app",
                    "version": "0.2.0",
                    "author": "Test",
                    "category": "news",
                    "tags": ["news", "social"],
                    "repo_url": "https://github.com/test/news",
                    "permissions": ["network", "storage"]
                }
            ]
        }"#;

        let registry: Registry = serde_json::from_str(json).unwrap();
        assert_eq!(registry.version, 1);
        assert_eq!(registry.apps.len(), 2);
        assert_eq!(registry.get_categories(), vec!["news", "tools"]);
        assert_eq!(registry.filter_by_category("tools").len(), 1);
        assert_eq!(
            registry.filter_by_category("tools")[0].id,
            "dev.cartridge.test"
        );
        assert_eq!(registry.filter_by_category("missing").len(), 0);
    }
}
