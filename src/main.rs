use std::path::PathBuf;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("run") => {
            let app_dir = parse_run_args(&args);
            let assets_dir = find_assets_dir();
            log::info!("Running Lua app from: {}", app_dir.display());

            if let Err(e) = cartridge_lua::run_lua_app(&app_dir, &assets_dir) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some("demo") => {
            let assets_dir = find_assets_dir();
            if let Err(e) = cartridge_runner::run_demo(&assets_dir) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some("help") | Some("--help") | Some("-h") => {
            print_usage();
        }
        Some(unknown) => {
            eprintln!("Unknown command: {unknown}");
            print_usage();
            std::process::exit(1);
        }
        None => {
            // Default: run the launcher in a loop so we can launch apps and return
            let assets_dir = find_assets_dir();
            loop {
                match cartridge_launcher::run_launcher(&assets_dir) {
                    Ok(cartridge_launcher::LauncherResult::Quit) => break,
                    Ok(cartridge_launcher::LauncherResult::LaunchApp(app_dir)) => {
                        log::info!("Launching app from: {}", app_dir.display());
                        if let Err(e) = cartridge_lua::run_lua_app(&app_dir, &assets_dir) {
                            log::error!("App error: {e}");
                            eprintln!("App error: {e}");
                            // Write crash log next to the binary for debugging
                            if let Ok(exe) = std::env::current_exe() {
                                if let Some(dir) = exe.parent() {
                                    let log_path = dir.join("crash.log");
                                    let msg = format!(
                                        "App: {}\nError: {e}\n",
                                        app_dir.display()
                                    );
                                    let _ = std::fs::OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open(&log_path)
                                        .and_then(|mut f| {
                                            use std::io::Write;
                                            f.write_all(msg.as_bytes())
                                        });
                                }
                            }
                        }
                        // Loop back to launcher
                    }
                    Err(e) => {
                        eprintln!("Launcher error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}

fn parse_run_args(args: &[String]) -> PathBuf {
    if let Some(pos) = args.iter().position(|a| a == "--path") {
        if pos + 1 < args.len() {
            return PathBuf::from(&args[pos + 1]);
        }
        eprintln!("Error: --path requires a directory argument");
        std::process::exit(1);
    }

    if args.len() > 2 {
        return PathBuf::from(&args[2]);
    }

    eprintln!("Error: 'run' command requires a path to the cartridge app directory");
    eprintln!("Usage: cartridge run --path <dir>");
    std::process::exit(1);
}

fn print_usage() {
    eprintln!("CartridgeOS - A cyberdeck OS for Linux handheld devices");
    eprintln!();
    eprintln!("Usage: cartridge <command>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  (default)          Launch the CartridgeOS home screen");
    eprintln!("  run --path <dir>   Run a Lua cartridge app");
    eprintln!("  demo               Run the built-in demo screen");
    eprintln!("  help               Show this help message");
}

fn find_assets_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    let cwd_assets = cwd.join("assets");
    if cwd_assets.join("fonts").exists() {
        return cwd_assets;
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        let exe_assets = exe_dir.join("assets");
        if exe_assets.join("fonts").exists() {
            return exe_assets;
        }
        if let Some(parent) = exe_dir.parent() {
            let parent_assets = parent.join("assets");
            if parent_assets.join("fonts").exists() {
                return parent_assets;
            }
        }
    }

    cwd_assets
}
