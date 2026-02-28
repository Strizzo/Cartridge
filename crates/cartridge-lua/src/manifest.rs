use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct CartridgeManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub category: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub entry: String,
}

impl CartridgeManifest {
    pub fn load(app_dir: &Path) -> Result<Self, String> {
        let manifest_path = app_dir.join("cartridge.json");
        let content = fs::read_to_string(&manifest_path).map_err(|e| {
            format!(
                "Failed to read cartridge.json at {}: {e}",
                manifest_path.display()
            )
        })?;
        let manifest: CartridgeManifest = serde_json::from_str(&content).map_err(|e| {
            format!("Failed to parse cartridge.json: {e}")
        })?;
        Ok(manifest)
    }
}
