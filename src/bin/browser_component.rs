//! Standalone entry point for the shared, verified browser component CLI.
fn main() {
    if let Err(error) = cartridge_net::browser_component_cli::run(std::env::args().skip(1)) {
        eprintln!("browser-component: {error}");
        std::process::exit(1);
    }
}
