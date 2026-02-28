use serde::{Deserialize, Serialize};

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
        let mut cats: Vec<String> = self
            .apps
            .iter()
            .map(|a| a.category.clone())
            .collect();
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

        serde_json::from_str::<Registry>(&response.body)
            .map_err(|e| format!("Failed to parse registry JSON: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(registry.filter_by_category("tools")[0].id, "dev.cartridge.test");
        assert_eq!(registry.filter_by_category("missing").len(), 0);
    }
}
