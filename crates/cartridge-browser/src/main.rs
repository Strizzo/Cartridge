use cartridge_browser::ui::{self, Options};
fn run() -> Result<(), String> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--url" => options.initial = Some(args.next().ok_or("--url needs an address")?),
            "--engine" => options.engine = Some(args.next().ok_or("--engine needs a path")?.into()),
            "--profile" => {
                options.profile = Some(args.next().ok_or("--profile needs a directory")?.into())
            }
            "--capture" => {
                options.capture = Some(args.next().ok_or("--capture needs a PNG path")?.into())
            }
            "--seconds" => {
                let s: f64 = args
                    .next()
                    .ok_or("--seconds needs a number")?
                    .parse()
                    .map_err(|_| "Invalid duration")?;
                if !s.is_finite() || s <= 0.0 {
                    return Err("Duration must be positive".into());
                }
                options.seconds = Some(s);
            }
            "--capture-after" => {
                let seconds: f64 = args
                    .next()
                    .ok_or("--capture-after needs seconds")?
                    .parse()
                    .map_err(|_| "Invalid capture delay")?;
                if !seconds.is_finite() || seconds < 0.0 {
                    return Err("Capture delay must be nonnegative".into());
                }
                options.capture_after = Some(seconds);
            }
            "--hidden" => options.hidden = true,
            "--help" => {
                println!(
                    "cartridge-browser [--url URL] [--engine PATH] [--profile DIRECTORY] [--capture PNG] [--capture-after SECONDS] [--seconds N] [--hidden]\nArrows scroll / focus links; Z open/click, X back, C address, V saved, Enter tools, Space exit.\nIJKL left stick: pointer. TFGH right stick: scroll. Hold A while moving to drag. F2 address; F12 screenshot."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }
    ui::run(&cartridge_core::paths::assets_dir(), options)
}
fn main() {
    env_logger::init();
    if let Err(e) = run() {
        eprintln!("Browser: {e}");
        std::process::exit(1);
    }
}
