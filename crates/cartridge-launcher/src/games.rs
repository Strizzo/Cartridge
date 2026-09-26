//! Read the stock game library off the UI thread; launch only after SDL closes.
use serde::{Deserialize, de::DeserializeOwned};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Debug, Default)]
pub struct GameRequest {
    pub system: String,
    pub path: PathBuf,
    pub error: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct GameSystem {
    pub id: String,
    pub name: String,
    pub theme: String,
}

#[derive(Clone, Deserialize)]
pub struct Game {
    pub path: PathBuf,
    pub name: String,
    pub image: Option<PathBuf>,
    pub favorite: bool,
    pub description: String,
    pub players: String,
}

pub enum Loaded {
    Systems(Vec<GameSystem>),
    Games(Vec<Game>),
}

fn helper() -> PathBuf {
    let root = cartridge_core::paths::assets_dir()
        .parent()
        .unwrap()
        .to_path_buf();
    let installed = root.join("game-library.py");
    if installed.is_file() {
        installed
    } else {
        root.join("deploy/game-library.py")
    }
}

fn read<T: DeserializeOwned>(args: &[&str]) -> Result<T, String> {
    let output = Command::new("python3")
        .arg(helper())
        .args(args)
        .output()
        .map_err(|e| format!("Cannot read game library: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid game library: {e}"))
}

pub fn load(system: Option<String>) -> Receiver<Result<Loaded, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match system {
            Some(id) => read(&["games", "--system", &id]).map(Loaded::Games),
            None => read(&["systems"]).map(Loaded::Systems),
        };
        let _ = tx.send(result);
    });
    rx
}

pub fn launch(game: &GameRequest) -> Result<(), String> {
    // No SDL context survives run_launcher_with_config's return. The stock
    // emulator owns display/audio/input until this synchronous child exits.
    let output = Command::new("python3")
        .arg(helper())
        .args(["launch", "--system", &game.system, "--rom"])
        .arg(&game.path)
        .output()
        .map_err(|e| format!("Cannot start emulator: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
