//! Opt-in live-site checks for the local engine; not part of offline CI.
use base64::Engine as _;
use cartridge_browser::engine::{Action, Session, Shared, discover};
use serde_json::{Value, json};
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
fn wait_for_visible_link(s: &mut Session, selector: &str) -> Result<(), String> {
    // Client-rendered news lists can appear after DOMContentLoaded or a
    // consent decision. Require a rendered link before real pointer input.
    let selector = serde_json::to_string(selector).unwrap();
    wait(s, &format!("[...document.querySelectorAll({selector})].some(e=>{{const r=e.getBoundingClientRect();return r.width>1&&r.height>1}})"))?;
    Ok(())
}
fn click_selector(s: &mut Session, selector: &str) -> Result<String, String> {
    let selector = serde_json::to_string(selector).unwrap();
    let point = wait(s, &format!(r#"(async()=>{{
        const selector={selector};
        const visible=e=>{{const r=e.getBoundingClientRect();return r.width>1&&r.height>1}};
        const first=[...document.querySelectorAll(selector)].find(visible);
        if(!first)return false;
        const key=first.href||first.id||'';
        first.scrollIntoView({{block:'center',behavior:'instant'}});
        await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
        const e=[...document.querySelectorAll(selector)].find(e=>(e.href||e.id||'')===key&&visible(e));
        if(!e)return false;
        const r=[...e.getClientRects()].find(r=>r.width>1&&r.height>1);
        if(!r)return false;
        const x=(Math.max(0,r.left)+Math.min(innerWidth,r.right))/2;
        const y=(Math.max(0,r.top)+Math.min(innerHeight,r.bottom))/2;
        const hit=document.elementFromPoint(x,y);
        if(!hit||(hit!==e&&!e.contains(hit)))return false;
        return [x,y,innerWidth,key];
    }})()"#))?;
    let scale = 720.0 / point[2].as_f64().ok_or("Missing viewport width")?;
    log::debug!("Trusted click target: {}", point[3]);
    s.action(Action::Click(
        point[0].as_f64().ok_or("Missing clickable element")? * scale,
        point[1].as_f64().ok_or("Missing clickable element")? * scale,
    ))?;
    Ok(point[3].as_str().unwrap_or("").to_string())
}
// Called on a candidate element (or CDP search text node). Read layout and
// hit-test each shadow boundary; never scroll, reveal or activate a control in JS.
const CONTROL_POINT: &str = r#"function(labels) {
    const selector = 'button, a[href], input[type=button], input[type=submit], input[type=reset], [role=button]';
    const node = this.nodeType === 1 ? this : this.parentElement;
    const control = node?.closest(selector);
    if (!control || control.matches(':disabled') || control.getAttribute('aria-disabled') === 'true') return null;
    for (let e = control; e; e = e.parentElement || e.getRootNode().host) {
        const style = getComputedStyle(e);
        if (e.inert || style.display === 'none' || style.contentVisibility === 'hidden' || Number(style.opacity) === 0) return null;
    }
    if (getComputedStyle(control).visibility !== 'visible') return null;
    const normalize = text => (text || '').replace(/\s+/g, ' ').trim().toLowerCase();
    const texts = [control.innerText, control.getAttribute('aria-label'), control.getAttribute('title'), control.value].map(normalize);
    const index = labels.findIndex(label => texts.some(text => text.includes(normalize(label))));
    if (index < 0) return null;
    for (const rect of control.getClientRects()) {
        const left = Math.max(0, rect.left), right = Math.min(innerWidth, rect.right);
        const top = Math.max(0, rect.top), bottom = Math.min(innerHeight, rect.bottom);
        if (right - left <= 1 || bottom - top <= 1) continue;
        const x = (left + right) / 2, y = (top + bottom) / 2;
        let visible = true;
        for (let e = control; e;) {
            const root = e.getRootNode(), hit = root.elementFromPoint(x, y);
            if (!hit || (hit !== e && !e.contains(hit))) { visible = false; break; }
            e = root.host;
        }
        if (visible) return {x, y, width: innerWidth, index};
    }
    return null;
}"#;

fn visible_text_point(s: &mut Session, labels: &[&str]) -> Result<Value, String> {
    s.evaluate(&format!(
        r#"(()=>{{
            const labels = {labels}, point = {point};
            let remaining = 20000, best = null;
            const exhausted = {{}};
            const filter = {{acceptNode(e) {{
                if (--remaining < 0) throw exhausted;
                const style = getComputedStyle(e);
                return e.inert || style.display === 'none' || style.contentVisibility === 'hidden' || Number(style.opacity) === 0
                    ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
            }}}};
            function scan(root, depth) {{
                if (depth > 32) return;
                const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT, filter);
                for (let e; (e = walker.nextNode());) {{
                    if (e.matches('button, a[href], input[type=button], input[type=submit], input[type=reset], [role=button]')) {{
                        const found = point.call(e, labels);
                        if (found && (!best || found.index < best.index)) best = found;
                        if (best?.index === 0) return;
                    }}
                    if (e.shadowRoot) scan(e.shadowRoot, depth + 1);
                    if (best?.index === 0) return;
                }}
            }}
            try {{ scan(document, 0); }} catch (error) {{ if (error !== exhausted) throw error; }}
            return best;
        }})()"#,
        labels = json!(labels),
        point = CONTROL_POINT,
    ))
}

fn shadow_text_point(s: &mut Session, labels: &[&str]) -> Result<Value, String> {
    let mut search_id = None;
    let result: Result<Value, String> = (|| {
        s.call("DOM.getDocument", json!({"depth":0,"pierce":true}))?;
        for label in labels {
            let found = s.call(
                "DOM.performSearch",
                json!({"query":label,"includeUserAgentShadowDOM":true}),
            )?;
            let id = found["searchId"]
                .as_str()
                .ok_or("Missing DOM search")?
                .to_owned();
            search_id = Some(id.clone());
            let count = found["resultCount"].as_u64().unwrap_or(0).min(64);
            let nodes = if count == 0 {
                json!({"nodeIds":[]})
            } else {
                s.call(
                    "DOM.getSearchResults",
                    json!({"searchId":id,"fromIndex":0,"toIndex":count}),
                )?
            };
            s.call("DOM.discardSearchResults", json!({"searchId":id}))?;
            search_id = None;
            for node in nodes["nodeIds"].as_array().ok_or("Missing DOM nodes")? {
                let resolved = s.call(
                    "DOM.resolveNode",
                    json!({"nodeId":node,"objectGroup":"browser-check-consent"}),
                )?;
                let object = resolved["object"]["objectId"]
                    .as_str()
                    .ok_or("Missing DOM object")?;
                let point = s.call("Runtime.callFunctionOn", json!({"objectId":object,"functionDeclaration":CONTROL_POINT,"arguments":[{"value":[label]}],"returnByValue":true}))?;
                if point.get("exceptionDetails").is_some() {
                    return Err("Cannot inspect consent control".into());
                }
                if point["result"]["value"].is_object() {
                    return Ok(point["result"]["value"].clone());
                }
            }
        }
        Ok(Value::Null)
    })();
    // getDocument enables the DOM agent. Release its bindings as well as the
    // search cache and temporary Runtime handles, including on an error path.
    let discarded = match search_id {
        Some(id) => s.call("DOM.discardSearchResults", json!({"searchId":id})),
        None => Ok(json!({})),
    };
    let released = s.call(
        "Runtime.releaseObjectGroup",
        json!({"objectGroup":"browser-check-consent"}),
    );
    let disabled = s.call("DOM.disable", json!({}));
    let point = result?;
    discarded?;
    released?;
    disabled?;
    Ok(point)
}

/// Prefer rendered controls without materializing the DevTools DOM. The CDP
/// fallback checks up to 64 results per label, including closed shadow roots.
/// Both paths use real input.
fn click_text(s: &mut Session, labels: &[&str]) -> Result<bool, String> {
    let mut point = visible_text_point(s, labels)?;
    if !point.is_object() {
        log::debug!("Consent controls {labels:?}: using CDP shadow fallback");
        point = shadow_text_point(s, labels)?;
    } else {
        log::debug!("Consent controls {labels:?}: found rendered control without DOM search");
    }
    if !point.is_object() {
        return Ok(false);
    }
    let scale = 720.0 / point["width"].as_f64().ok_or("Missing viewport width")?;
    s.action(Action::Click(
        point["x"].as_f64().ok_or("Missing control x")? * scale,
        point["y"].as_f64().ok_or("Missing control y")? * scale,
    ))?;
    let until = Instant::now() + Duration::from_millis(500);
    while Instant::now() < until {
        s.pump(Duration::from_millis(30))?;
    }
    Ok(true)
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
        // A parsed, nonempty document is interactive while slow images/ads
        // may still load. Subsequent real controls/links verify usability.
        "location.protocol.startsWith('http') && document.readyState!=='loading' && document.body?.innerText.trim().length>0",
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
            if click_text(
                &mut s,
                &["Essential Cookies Only", "I DO NOT ACCEPT", "Reject All"],
            )? {
                checks.push("rejected optional consent through site control");
                break;
            }
            if !click_text(&mut s, &["Show Purposes"])? {
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
        wait_for_visible_link(&mut s, selector)?;
        let expected_article = click_selector(&mut s, selector)?;
        let expected = serde_json::to_string(&expected_article).unwrap();
        wait(
            &mut s,
            &format!("(()=>{{const wanted=new URL({expected});return location.pathname===wanted.pathname&&location.hostname.split('.').slice(-2).join('.')===wanted.hostname.split('.').slice(-2).join('.')&&location.href!=={before}&&document.readyState!=='loading'}})()"),
        )?;
        wait(&mut s, "document.querySelector('h1')?.innerText?.length>10")?;
        checks.push("article opened through real link input");
        capture(&mut s, &out, "article")?;
        log::debug!("Article URL before Back: {}", s.evaluate("location.href")?);
        s.action(Action::Back)?;
        log::debug!("URL just after Back: {}", s.evaluate("location.href")?);
        wait(&mut s, &format!("location.href==={}", before))?;
        wait_for_visible_link(&mut s, selector)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires local Chromium; real trusted input and shadow DOM"]
    fn visible_consent_controls_and_closed_shadow_cleanup() {
        let _ = env_logger::try_init();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let profile =
            std::env::temp_dir().join(format!("browser-consent-{}-{nonce}", std::process::id()));
        let mut s = Session::start(&discover(None).unwrap(), &profile, Shared::default()).unwrap();
        let assert_dom_disabled = |s: &mut Session| {
            assert!(
                s.call(
                    "DOM.getNodeForLocation",
                    json!({"x":20,"y":20,"includeUserAgentShadowDOM":true})
                )
                .unwrap()
                .get("nodeId")
                .is_none()
            );
        };
        s.action(Action::Zoom(0.75)).unwrap();
        s.evaluate(r#"(()=>{
            document.body.innerHTML = `<style>button{width:180px;height:44px} #vendors{display:none}</style>
                <div id="vendors"></div><button disabled>Essential Cookies Only</button>
                <button id="reject">I DO NOT ACCEPT</button><button id="accept">Accept All</button>`;
            const hidden = document.getElementById('vendors');
            for (let i=0;i<25000;i++) {
                const e=document.createElement('button');e.textContent='Essential Cookies Only';hidden.append(e);
            }
            window.clicked=[];
            document.addEventListener('click',e=>window.clicked.push({id:e.target.id,trusted:e.isTrusted}));
            return true;
        })()"#).unwrap();
        // A missing first label must not index the hidden vendor tree before
        // trying the other permitted rejection choices. Scaling uses CSS pixels.
        let labels = ["Essential Cookies Only", "I DO NOT ACCEPT", "Reject All"];
        assert!(visible_text_point(&mut s, &labels).unwrap().is_object());
        assert!(click_text(&mut s, &labels).unwrap());
        assert_eq!(
            s.evaluate("window.clicked").unwrap(),
            json!([{"id":"reject","trusted":true}])
        );
        assert_dom_disabled(&mut s);
        println!("PASS: visible rejection beside 25,000 hidden vendor controls, at 75% zoom");

        s.evaluate(r#"(()=>{
            document.body.innerHTML='<div id="host"></div>';
            const outer=document.getElementById('host').attachShadow({mode:'open'});
            outer.innerHTML='<div id="inner"></div>';
            const root=outer.getElementById('inner').attachShadow({mode:'open'});
            root.innerHTML='<button style="width:180px;height:44px" aria-label="Reject All">Choice</button>';
            window.choice=null;
            root.querySelector('button').onclick=e=>window.choice=e.isTrusted;
            return true;
        })()"#).unwrap();
        assert!(
            visible_text_point(&mut s, &["Reject All"])
                .unwrap()
                .is_object()
        );
        assert!(click_text(&mut s, &["Reject All"]).unwrap());
        assert_eq!(s.evaluate("window.choice").unwrap(), true);
        assert_dom_disabled(&mut s);
        println!("PASS: nested open shadow roots and accessible label, trusted click");

        s.evaluate(r#"(()=>{
            document.body.innerHTML=`<button style="position:absolute;left:10px;top:10px;width:180px;height:44px">Reject All</button>
                <div style="position:absolute;left:0;top:0;width:400px;height:200px;background:white"></div>
                <button style="position:absolute;top:2000px">Reject All</button>
                <button style="opacity:0">Reject All</button>`;
            return true;
        })()"#).unwrap();
        assert_eq!(
            visible_text_point(&mut s, &["Reject All"]).unwrap(),
            Value::Null
        );
        println!("PASS: occluded, offscreen and transparent controls excluded");

        s.evaluate(
            r#"(()=>{
            document.body.innerHTML='<div id="host"></div>';
            const root=document.getElementById('host').attachShadow({mode:'closed'});
            root.innerHTML='<button style="width:180px;height:44px">Closed reject</button>';
            window.closedButton=root.querySelector('button');
            window.choice=null;
            window.closedButton.onclick=e=>window.choice=e.isTrusted;
            return true;
        })()"#,
        )
        .unwrap();
        assert_eq!(
            visible_text_point(&mut s, &["Closed reject"]).unwrap(),
            Value::Null
        );
        assert!(click_text(&mut s, &["Closed reject"]).unwrap());
        assert_eq!(s.evaluate("window.choice").unwrap(), true);
        assert_dom_disabled(&mut s);
        println!("PASS: closed shadow fallback, trusted click, DOM agent disabled after success");

        s.evaluate("window.closedButton.getClientRects=()=>{throw new Error('fixture layout failure')};true").unwrap();
        assert!(shadow_text_point(&mut s, &["Closed reject"]).is_err());
        assert_dom_disabled(&mut s);
        assert!(!click_text(&mut s, &["No such choice"]).unwrap());
        assert_dom_disabled(&mut s);
        println!("PASS: DOM agent disabled after inspection failure and missing label");
        drop(s);
        std::fs::remove_dir_all(profile).unwrap();
    }
}
