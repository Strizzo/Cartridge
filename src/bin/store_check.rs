//! Explicit live check of the public Store. Every write is in a new temporary home.
use cartridge_net::{AppInstaller, HttpClient, RegistryClient};
use std::path::PathBuf;
struct Scratch {
    path: PathBuf,
    keep: bool,
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !args.iter().any(|a| a == "--live") || args.iter().any(|a| a != "--live" && a != "--keep") {
        return Err("Usage: store-check --live [--keep] (downloads the official catalogue and packages into isolated temporary storage)".into());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = Scratch {
        path: std::env::temp_dir().join(format!(
            "cartridge-store-check-{}-{nonce}",
            std::process::id()
        )),
        keep: args.iter().any(|a| a == "--keep"),
    };
    std::fs::create_dir_all(&scratch.path).map_err(|e| e.to_string())?;
    // Set before starting any threads. Never touch the developer's real app state.
    unsafe {
        std::env::set_var("CARTRIDGE_HOME", &scratch.path);
    }
    let http = || HttpClient::new(scratch.path.join("http"));
    let registry =
        RegistryClient::new(http(), cartridge_net::registry::OFFICIAL_CATALOG_URL.into())
            .fetch()?;
    let installer = AppInstaller::new(http());
    if registry.apps.is_empty() {
        return Err("Official catalogue was empty".into());
    }
    for app in &registry.apps {
        let data = scratch.path.join(".cartridges").join(&app.id).join("data");
        std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
        let settings = data.join("store-check.json");
        std::fs::write(&settings, b"{\"preserved\":true}").map_err(|e| e.to_string())?;
        installer.install(app)?;
        let installed = installer.app_path(&app.id);
        let manifest = cartridge_lua::manifest::CartridgeManifest::load(&installed)?;
        if manifest.id != app.id || manifest.version != app.version {
            return Err("Installed identity mismatch".into());
        }
        if cartridge_core::paths::resolve_cartridge_dir(&app.id) != Some(installed.clone()) {
            return Err("Installed app did not override bundled app".into());
        }
        let mut corrupt = app.clone();
        corrupt.package.as_mut().ok_or("Missing package")?.sha256 = "00".repeat(32);
        if installer.install(&corrupt).is_ok() {
            return Err("Corrupt checksum accepted".into());
        }
        if !installed.join("main.lua").is_file() {
            return Err("Failed update damaged existing install".into());
        }
        // Keep the real downloaded package for optional simulator runs; uninstall
        // and data preservation are covered by the installer's deterministic tests.
        if std::fs::read(&settings).map_err(|e| e.to_string())? != b"{\"preserved\":true}" {
            return Err("App settings changed".into());
        }
        println!(
            "Verified {} {}: signed metadata, HTTPS package, checksum, install, override and rejected tampered update",
            app.name, app.version
        );
    }
    println!(
        "Isolated Store home: {}{}",
        scratch.path.display(),
        if scratch.keep {
            " (retained)"
        } else {
            " (removed after check)"
        }
    );
    Ok(())
}
fn main() {
    env_logger::init();
    if let Err(error) = run() {
        eprintln!("Store check failed: {error}");
        std::process::exit(1);
    }
}
