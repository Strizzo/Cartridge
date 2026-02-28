use std::path::PathBuf;

fn main() {
    env_logger::init();

    // Find assets directory relative to the executable or current dir
    let assets_dir = find_assets_dir();
    log::info!("Assets directory: {}", assets_dir.display());

    if let Err(e) = cartridge_runner::run_demo(&assets_dir) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn find_assets_dir() -> PathBuf {
    // Try relative to current directory first
    let cwd = std::env::current_dir().unwrap_or_default();
    let cwd_assets = cwd.join("assets");
    if cwd_assets.join("fonts").exists() {
        return cwd_assets;
    }

    // Try relative to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let exe_assets = exe_dir.join("assets");
            if exe_assets.join("fonts").exists() {
                return exe_assets;
            }
            // Try one level up (common for cargo run)
            if let Some(parent) = exe_dir.parent() {
                let parent_assets = parent.join("assets");
                if parent_assets.join("fonts").exists() {
                    return parent_assets;
                }
            }
        }
    }

    // Fallback to cwd/assets
    cwd_assets
}
