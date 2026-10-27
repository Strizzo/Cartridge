//! Opt-in live-site checks for the local engine; not part of offline CI.
use base64::Engine as _;
use cartridge_browser::engine::{discover, Action, Session, Shared};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn wait(s: &mut Session, expression: &str) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_secs(35);
    while Instant::now() < deadline {
        s.pump(Duration::from_millis(50))?;
        if let Ok(v) = s.evaluate(expression) {
            if v != Value::Null && v != Value::Bool(false) && v != json!(0) {
                return Ok(v);
            }
        }
        std::thread::sleep(Duration::from_millis(80));
    }
    Err(format!("Site condition timed out: {expression}"))
}
fn capture(s: &mut Session, out: &Path, name: &str) -> Result<(), String> {
    let result = s.call(
        "Page.captureScreenshot",
        json!({"format":"png", "captureBeyondViewport":false}),
    )?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(result["data"].as_str().ok_or("No screenshot")?)
        .map_err(|e| e.to_string())?;
    std::fs::write(out.join(format!("{name}.png")), bytes).map_err(|e| e.to_string())
}
fn click_selector(s: &mut Session, selector: &str) -> Result<(), String> {
    let selector = serde_json::to_string(selector).unwrap();
    let point=s.evaluate(&format!("(()=>{{const e=[...document.querySelectorAll({selector})].find(e=>{{const r=e.getBoundingClientRect();return r.width>1&&r.height>1}}); if(!e)return null; e.scrollIntoView({{block:'center',behavior:'instant'}});const r=[...e.getClientRects()].find(r=>r.width>1&&r.height>1);return r?[r.x+r.width/2,r.y+r.height/2]:null}})()"))?;
    let scale = 720.0
        / s.evaluate("innerWidth")?
            .as_f64()
            .ok_or("Missing viewport width")?;
    s.action(Action::Click(
        point[0].as_f64().ok_or("Missing clickable element")? * scale,
        point[1].as_f64().ok_or("Missing clickable element")? * scale,
    ))
}
/// Find rendered text through the browser DOM API, including consent shadow
/// trees. Only send a real click to a visible control; never remove overlays.
fn click_text(s: &mut Session, label: &str) -> Result<bool, String> {
    s.call("DOM.getDocument", json!({"depth":0,"pierce":true}))?;
    let found = s.call(
        "DOM.performSearch",
        json!({"query":label,"includeUserAgentShadowDOM":true}),
    )?;
    let id = found["searchId"].as_str().ok_or("Missing DOM search")?;
    let count = found["resultCount"].as_u64().unwrap_or(0).min(64);
    if count == 0 {
        s.call("DOM.discardSearchResults", json!({"searchId":id}))?;
        return Ok(false);
    }
    let nodes = s.call(
        "DOM.getSearchResults",
        json!({"searchId":id,"fromIndex":0,"toIndex":count}),
    )?;
    s.call("DOM.discardSearchResults", json!({"searchId":id}))?;
    let width = s.evaluate("innerWidth")?.as_f64().unwrap_or(720.0);
    let height = s.evaluate("innerHeight")?.as_f64().unwrap_or(576.0);
    for node in nodes["nodeIds"].as_array().ok_or("Missing DOM nodes")? {
        let Ok(model) = s.call("DOM.getBoxModel", json!({"nodeId":node})) else {
            continue;
        };
        let Some(quad) = model["model"]["content"].as_array() else {
            continue;
        };
        if quad.len() != 8 {
            continue;
        }
        let x = (0..4)
            .map(|i| quad[i * 2].as_f64().unwrap_or(-10000.0))
            .sum::<f64>()
            / 4.0;
        let y = (0..4)
            .map(|i| quad[i * 2 + 1].as_f64().unwrap_or(-10000.0))
            .sum::<f64>()
            / 4.0;
        if x > 0.0 && x < width && y > 0.0 && y < height {
            s.action(Action::Click(x * 720.0 / width, y * 720.0 / width))?;
            let until = Instant::now() + Duration::from_millis(500);
            while Instant::now() < until {
                s.pump(Duration::from_millis(30))?;
            }
            return Ok(true);
        }
    }
    Ok(false)
}
fn check_sandbox(engine: &Path, out: &Path) -> Result<(), String> {
    let out = out.join("sandbox");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let mut session = Session::start(engine, &out.join("profile"), Shared::default())?;
    // Fixed internal diagnostic only; normal address entry still allows web URLs.
    session.call("Page.navigate", json!({"url":"chrome://sandbox/"}))?;
    let body = wait(
        &mut session,
        "document.body && document.body.innerText.includes('sandboxed') && document.body.innerText",
    )?;
    let text = body.as_str().ok_or("Missing Chromium sandbox report")?;
    std::fs::write(out.join("sandbox.txt"), text).map_err(|e| e.to_string())?;
    capture(&mut session, &out, "sandbox")?;
    let normalized = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    for expected in [
        "layer 1 sandbox namespace",
        "pid namespaces yes",
        "network namespaces yes",
        "seccomp-bpf sandbox yes",
        "you are adequately sandboxed",
    ] {
        if !normalized.contains(expected) {
            return Err(format!(
                "Chromium sandbox report is missing {expected:?}; see sandbox.txt"
            ));
        }
    }
    println!(
        "{}",
        json!({"check":"chromium sandbox", "result":"pass", "report":text})
    );
    Ok(())
}

fn check(site: &str, engine: &Path, out: &Path) -> Result<(), String> {
    if site == "sandbox" {
        return check_sandbox(engine, out);
    }
    let out = out.join(site);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let profile = out.join("profile");
    let shared = Shared::default();
    let started = Instant::now();
    let mut s = Session::start(engine, &profile, shared.clone())?;
    let url = match site {
        "slashdot" => "https://slashdot.org/",
        "cnn" => "https://www.cnn.com/",
        "osm" => "https://www.openstreetmap.org/",
        _ => return Err("Choose slashdot, cnn or osm".into()),
    };
    s.action(Action::Navigate(url.into()))?;
    s.action(Action::Zoom(0.75))?;
    wait(
        &mut s,
        "location.protocol.startsWith('http') && document.readyState==='complete'",
    )?;
    let inspect=s.evaluate("({title:document.title,url:location.href,status:performance.getEntriesByType('navigation')[0]?.responseStatus,body:document.body.innerText.slice(0,500),buttons:[...document.querySelectorAll('button,[role=button]')].map(e=>e.innerText).filter(Boolean).slice(0,60),frames:[...document.querySelectorAll('iframe')].map(e=>({title:e.title,src:e.src})).slice(0,15),links:[...document.querySelectorAll('a[href]')].filter(e=>e.innerText.trim().length>30).slice(0,20).map(e=>({text:e.innerText.slice(0,180),url:e.href}))})")?;
    std::fs::write(
        out.join("initial.json"),
        serde_json::to_vec_pretty(&inspect).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    capture(&mut s, &out, "initial")?;
    if site == "cnn"
        && inspect["body"]
            .as_str()
            .is_some_and(|s| s.trim() == "Unknown Error")
    {
        return Err(format!(
            "CNN returned HTTP {} with a literal Unknown Error body; see initial.json/png",
            inspect["status"]
        ));
    }
    if inspect["status"].as_u64().is_some_and(|s| s >= 400) {
        return Err(format!(
            "The site returned HTTP {}: {}",
            inspect["status"],
            inspect["body"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(100)
                .collect::<String>()
        ));
    }
    let controls=s.evaluate("[...document.querySelectorAll('a[title],button[title],button[aria-label]')].map(e=>({text:e.innerText,title:e.title,aria:e.getAttribute('aria-label'),class:e.className})).slice(0,100)")?;
    std::fs::write(
        out.join("controls.json"),
        serde_json::to_vec_pretty(&controls).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    let mut checks = vec!["real page loaded"];
    if site == "osm" {
        wait(
            &mut s,
            "document.querySelectorAll('.leaflet-tile-loaded').length>0",
        )?;
        let before = s.evaluate("location.hash")?;
        s.action(Action::Button(350.0, 320.0, true))?;
        for i in 1..=8 {
            s.action(Action::Pointer(
                350.0 + i as f64 * 18.0,
                320.0 + i as f64 * 4.0,
            ))?;
            std::thread::sleep(Duration::from_millis(30));
        }
        s.action(Action::Button(494.0, 352.0, false))?;
        wait(&mut s, &format!("location.hash!=={}", before))?;
        checks.push("map panned with pointer drag");
        let before = s.evaluate("location.hash.match(/map=(\\d+)/)?.[1]")?;
        s.action(Action::Scroll(350.0, 320.0, 0.0, -320.0))?;
        wait(
            &mut s,
            &format!("location.hash.match(/map=(\\d+)/)?.[1]!=={}", before),
        )?;
        checks.push("map wheel zoom changed map position");
        capture(&mut s, &out, "map-pan-zoom")?;
        click_selector(&mut s, "#query")?;
        s.action(Action::Text("Luxembourg".into()))?;
        s.action(Action::Key("Enter".into(), false))?;
        wait(&mut s, "!!document.querySelector('.search_results_entry')")?;
        checks.push("map search returned results");
        capture(&mut s, &out, "map-search")?;
    } else {
        // Use the site's visible consent buttons, including shadow DOM.
        for _ in 0..3 {
            let mut rejected = false;
            for label in ["Essential Cookies Only", "I DO NOT ACCEPT", "Reject All"] {
                if click_text(&mut s, label)? {
                    rejected = true;
                    break;
                }
            }
            if rejected {
                checks.push("rejected optional consent through site control");
                break;
            }
            if !click_text(&mut s, "Show Purposes")? {
                break;
            }
        }
        capture(&mut s, &out, "after-consent")?;
        let selector = if site == "slashdot" {
            "a[href*='/story/']"
        } else {
            "a[href*='/202']:not([href*='/videos/']):not([href*='/live-news/'])"
        };
        let before = s.evaluate("location.href")?;
        click_selector(&mut s, selector)?;
        wait(
            &mut s,
            &format!(
                "location.href!=={} && document.readyState==='complete'",
                before
            ),
        )?;
        wait(&mut s, "document.querySelector('h1')?.innerText?.length>10")?;
        checks.push("article opened through real link input");
        capture(&mut s, &out, "article")?;
        s.action(Action::Back)?;
        wait(&mut s, &format!("location.href==={}", before))?;
        checks.push("Back returned to listing");
    }
    let final_url = s.evaluate("location.href")?;
    let engine_version = shared.lock().unwrap().info.engine_version.clone();
    let report = json!({"site":site,"engine":engine_version,"elapsed_seconds":started.elapsed().as_secs_f64(),"checks":checks,"final_url":final_url});
    std::fs::write(
        out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string(&report).unwrap());
    Ok(())
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut site = "all".to_string();
    let mut out = None;
    let mut engine = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--site" => site = args.next().ok_or("Missing site")?,
            "--out" => {
                out = Some(PathBuf::from(
                    args.next().ok_or("Missing output directory")?,
                ))
            }
            "--engine" => engine = Some(PathBuf::from(args.next().ok_or("Missing engine path")?)),
            "--help" => {
                println!(
                    "browser-check --site all|slashdot|cnn|osm|sandbox --out NEW-DIRECTORY [--engine PATH]\nOpt-in real-site checks. Creates isolated profiles and public-page captures, never personal browser sessions."
                );
                return Ok(());
            }
            _ => return Err(format!("Unknown argument {arg}")),
        }
    }
    let out = out.ok_or("--out NEW-DIRECTORY required")?;
    if out.exists() {
        return Err("Output directory already exists; choose a new one.".into());
    }
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let engine = discover(engine)?;
    let sites = if site == "all" {
        vec!["slashdot", "cnn", "osm"]
    } else {
        vec![site.as_str()]
    };
    let mut errors = vec![];
    for site in sites {
        if let Err(e) = check(site, &engine, &out) {
            eprintln!("{site}: {e}");
            errors.push(format!("{site}: {e}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
fn main() {
    env_logger::init();
    if let Err(e) = run() {
        eprintln!("Browser check: {e}");
        std::process::exit(1)
    }
}
