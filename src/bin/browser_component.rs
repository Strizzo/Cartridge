//! Offline/HTTPS browser component delivery. Never runs the installed engine.
use cartridge_net::browser_component::{self, BrowserComponents, VerifiedComponent};
use std::collections::BTreeMap;
use std::path::PathBuf;

const USAGE: &str = "browser-component commands (stop the browser before changing its component):
  inspect --manifest FILE | --url HTTPS_URL
  verify --manifest FILE [--archive FILE] [--source PREPARED_TREE]
  verify [--home USER_HOME]                  # verify the active installed tree
  adopt --manifest FILE --source TREE [--home USER_HOME]  # verified MOVE, same filesystem
  install --manifest FILE --archive FILE [--home USER_HOME]
  install --url HTTPS_MANIFEST_URL [--home USER_HOME]     # streaming verified update
  rollback [--home USER_HOME]
  status [--home USER_HOME]
Default home: CARTRIDGE_HOME, else HOME. Fixed storage: <home>/.cartridges/components/browser-engine.
Only the pinned cartridge-os-v1 signing key is accepted. Persistent changes require Linux.
Inspect/verify never execute the engine. There is no unsigned or sandbox-bypass option.";

fn describe(component: &VerifiedComponent) {
    let r = component.release();
    println!(
        "Verified browser-engine {}\nID: {}\nTarget: {}\nChromium: {}\nEntries: {}\nArchive: {} bytes\nStored: {} bytes; logical: {} bytes\nSource: {}",
        r.version,
        component.id(),
        r.target,
        r.chromium_version,
        r.entries.len(),
        r.archive.size,
        r.archive.stored_bytes,
        r.archive.logical_bytes,
        r.source_distribution
    );
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let mut flags = BTreeMap::new();
    while let Some(flag) = args.next() {
        if !["--home", "--manifest", "--archive", "--source", "--url"].contains(&flag.as_str()) {
            return Err(format!("Unknown argument {flag}\n{USAGE}"));
        }
        let value = args
            .next()
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| format!("Missing value for {flag}"))?;
        if flags.insert(flag, value).is_some() {
            return Err("Duplicate argument".into());
        }
    }
    let allowed: &[&str] = match command.as_str() {
        "inspect" => &["--manifest", "--url"],
        "verify" => &["--home", "--manifest", "--archive", "--source"],
        "adopt" => &["--home", "--manifest", "--source"],
        "install" => &["--home", "--manifest", "--archive", "--url"],
        "rollback" | "status" => &["--home"],
        _ => return Err(format!("Unknown command\n{USAGE}")),
    };
    if flags.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(format!("Unexpected argument for {command}"));
    }
    let path = |name: &str| {
        flags
            .get(name)
            .map(PathBuf::from)
            .ok_or_else(|| format!("{command} requires {name}"))
    };
    let store = || match flags.get("--home") {
        Some(home) => BrowserComponents::at_home(&PathBuf::from(home)),
        None => BrowserComponents::from_environment(),
    };
    match command.as_str() {
        "inspect" => {
            let component = match (flags.get("--manifest"), flags.get("--url")) {
                (Some(file), None) => browser_component::inspect_local(&PathBuf::from(file))?,
                (None, Some(url)) => browser_component::inspect_https(url)?,
                _ => return Err("inspect requires exactly one of --manifest or --url".into()),
            };
            describe(&component);
        }
        "verify" if flags.contains_key("--manifest") => {
            if flags.contains_key("--home")
                || (!flags.contains_key("--archive") && !flags.contains_key("--source"))
            {
                return Err(
                    "verify --manifest requires --archive and/or --source, without --home".into(),
                );
            }
            let component = browser_component::inspect_local(&path("--manifest")?)?;
            if let Ok(archive) = path("--archive") {
                browser_component::verify_archive(&component, &archive)?;
            }
            if let Ok(source) = path("--source") {
                component.verify_directory(&source)?;
            }
            describe(&component);
        }
        "verify" => {
            if flags.contains_key("--source") || flags.contains_key("--archive") {
                return Err("--archive/--source require --manifest".into());
            }
            describe(&store()?.verify_current()?);
        }
        "adopt" => {
            describe(&store()?.adopt(&path("--manifest")?, &path("--source")?)?);
        }
        "install" => {
            let progress = |text| eprintln!("{text}");
            let component = if let Some(url) = flags.get("--url") {
                if flags.contains_key("--manifest") || flags.contains_key("--archive") {
                    return Err("Choose --url or local --manifest/--archive".into());
                }
                store()?.install_https(url, progress)?
            } else {
                store()?.install_local(&path("--manifest")?, &path("--archive")?, progress)?
            };
            describe(&component);
        }
        "rollback" => describe(&store()?.rollback()?),
        "status" => {
            let store = store()?;
            let status = store.status()?;
            println!(
                "Store: {}\nCurrent: {}\nPrevious: {}",
                store.root().display(),
                status.current.as_deref().unwrap_or("none"),
                status.previous.as_deref().unwrap_or("none")
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("browser-component: {error}");
        std::process::exit(1);
    }
}
