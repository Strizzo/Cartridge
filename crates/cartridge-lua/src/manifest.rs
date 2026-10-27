use serde::Deserialize;
use std::fs;
use std::path::Path;

/// A named, built-in runtime, never an arbitrary executable or shell command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppRuntime {
    #[default]
    Lua,
    Browser,
}

#[derive(Debug, Deserialize)]
pub struct CartridgeManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub entry: String,
    #[serde(default)]
    pub runtime: AppRuntime,
    #[serde(default)]
    pub min_runtime: Option<String>,
}

impl CartridgeManifest {
    /// A manifest may select a fixed built-in runtime, never an executable.
    pub fn validate_runtime(&self) -> Result<(), String> {
        if self.runtime == AppRuntime::Browser
            && (self.id != "dev.cartridge.browser"
                || !["network", "storage", "audio"]
                    .iter()
                    .all(|permission| self.permissions.iter().any(|p| p == permission)))
        {
            return Err("The built-in browser requires its official app identity and declared network, storage and audio permissions.".into());
        }
        Ok(())
    }

    pub fn load(app_dir: &Path) -> Result<Self, String> {
        let manifest_path = app_dir.join("cartridge.json");
        let content = fs::read_to_string(&manifest_path).map_err(|e| {
            format!(
                "Failed to read cartridge.json at {}: {e}",
                manifest_path.display()
            )
        })?;
        let manifest: CartridgeManifest = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse cartridge.json: {e}"))?;
        if let Some(minimum) = &manifest.min_runtime {
            let minimum = semver::Version::parse(minimum)
                .map_err(|_| "Invalid minimum runtime version in cartridge.json".to_string())?;
            let runtime = semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
            if minimum.cmp_precedence(&runtime).is_gt() {
                return Err(format!(
                    "{} requires CartridgeOS {} or later (this is {})",
                    manifest.name, minimum, runtime
                ));
            }
        }
        manifest.validate_runtime()?;
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn browser() -> serde_json::Value {
        serde_json::json!({"id":"dev.cartridge.browser", "name":"Browser", "version":"0.1.0",
            "author":"CartridgeOS", "entry":"main.lua", "runtime":"browser",
            "permissions":["network", "storage", "audio"]})
    }
    #[test]
    fn existing_apps_default_to_lua_and_native_paths_are_not_runtimes() {
        let mut json = browser();
        json.as_object_mut().unwrap().remove("runtime");
        let manifest: CartridgeManifest = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(manifest.runtime, AppRuntime::Lua);
        json["runtime"] = serde_json::json!("/bin/sh");
        assert!(serde_json::from_value::<CartridgeManifest>(json).is_err());
    }
    #[test]
    fn browser_dispatch_requires_identity_and_declared_permissions() {
        let mut json = browser();
        assert!(
            serde_json::from_value::<CartridgeManifest>(json.clone())
                .unwrap()
                .validate_runtime()
                .is_ok()
        );
        json["id"] = serde_json::json!("other.app");
        assert!(
            serde_json::from_value::<CartridgeManifest>(json)
                .unwrap()
                .validate_runtime()
                .is_err()
        );
        for missing in ["network", "storage", "audio"] {
            let mut json = browser();
            json["permissions"] = serde_json::json!(
                ["network", "storage", "audio"]
                    .into_iter()
                    .filter(|p| *p != missing)
                    .collect::<Vec<_>>()
            );
            assert!(
                serde_json::from_value::<CartridgeManifest>(json)
                    .unwrap()
                    .validate_runtime()
                    .is_err()
            );
        }
    }
}
