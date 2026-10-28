//! Opt-in live-site checks for the local engine; not part of offline CI.
use base64::Engine as _;
use cartridge_browser::engine::{Action, Session as EngineSession, Shared, discover};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// Match ui.rs: real input keeps transfer at 24 fps for two seconds; an idle
// page transfers at 8 fps. Inspection is not user input. The direct engine API
// otherwise stays at its initial rate even during a long network/consent wait.
struct Session {
    engine: EngineSession,
    last_input: Instant,
    frame_rate: u8,
}
impl Session {
    fn start(engine: &Path, profile: &Path, shared: Shared) -> Result<Self, String> {
        Ok(Self {
            engine: EngineSession::start(engine, profile, shared)?,
            last_input: Instant::now(),
            frame_rate: 24,
        })
    }
    fn pace(&mut self) -> Result<(), String> {
        let fps = if self.last_input.elapsed() < Duration::from_secs(2) {
            24
        } else {
            8
        };
        if self.frame_rate != fps {
            self.engine.action(Action::FrameRate(fps))?;
            self.frame_rate = fps;
            log::debug!("Native page-transfer budget: {fps} fps");
        }
        Ok(())
    }
    fn action(&mut self, action: Action) -> Result<(), String> {
        if matches!(
            &action,
            Action::Navigate(_)
                | Action::Back
                | Action::Forward
                | Action::Reload
                | Action::Stop
                | Action::Pointer(..)
                | Action::Button(..)
                | Action::Click(..)
                | Action::Scroll(..)
                | Action::Key(..)
                | Action::Text(_)
                | Action::Zoom(_)
        ) {
            self.last_input = Instant::now();
        }
        self.pace()?;
        if let Action::Click(x, y) = action {
            // The frontend points, presses and releases through these actions.
            // Exercise that path (including buttons=1/0), rather than the raw
            // convenience click whose pressed event omits the buttons state.
            self.engine.action(Action::Pointer(x, y))?;
            self.engine.action(Action::Button(x, y, true))?;
            self.engine.action(Action::Button(x, y, false))
        } else {
            self.engine.action(action)
        }
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.pace()?;
        self.engine.call(method, params)
    }
    fn evaluate(&mut self, expression: &str) -> Result<Value, String> {
        self.pace()?;
        self.engine.evaluate(expression)
    }
    fn pump(&mut self, timeout: Duration) -> Result<(), String> {
        self.pace()?;
        self.engine.pump(timeout)
    }
}

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
    let state = s.evaluate("({url:location.href,ready:document.readyState,title:document.title,body_chars:document.body?.innerText.length,h1:document.querySelector('h1')?.innerText})");
    Err(format!(
        "Site condition timed out: {expression}; actual page: {state:?}"
    ))
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
    wait(
        s,
        &format!(
            "[...document.querySelectorAll({selector})].some(e=>{{const r=e.getBoundingClientRect();return r.width>1&&r.height>1}})"
        ),
    )?;
    Ok(())
}
fn selector_point_expression(selector: &str) -> String {
    let selector = serde_json::to_string(selector).unwrap();
    format!(
        r#"(async()=>{{
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
        const left=Math.max(0,r.left),right=Math.min(innerWidth,r.right);
        const top=Math.max(0,r.top),bottom=Math.min(innerHeight,r.bottom);
        if(right-left<=1||bottom-top<=1)return false;
        const x=(left+right)/2,y=(top+bottom)/2;
        const hit=document.elementFromPoint(x,y);
        if(!hit)return false;
        const blocked=hit!==e&&!e.contains(hit),style=getComputedStyle(hit);
        const overlay=blocked&&visible(hit)&&style.visibility==='visible'&&Number(style.opacity)>0;
        return {{x,y,width:innerWidth,key,blocked,overlay,text:e.innerText}};
    }})()"#
    )
}
fn click_link_point(s: &mut Session, point: &Value) -> Result<String, String> {
    let scale = 720.0 / point["width"].as_f64().ok_or("Missing viewport width")?;
    log::debug!("Trusted click target: {}", point["key"]);
    s.action(Action::Click(
        point["x"].as_f64().ok_or("Missing clickable element")? * scale,
        point["y"].as_f64().ok_or("Missing clickable element")? * scale,
    ))?;
    Ok(point["key"].as_str().unwrap_or("").to_string())
}
fn click_selector(s: &mut Session, selector: &str) -> Result<String, String> {
    let expression = selector_point_expression(selector);
    let point = wait(
        s,
        &format!("(async()=>{{const p=await {expression};return p&&!p.blocked?p:false}})()"),
    )?;
    click_link_point(s, &point)
}
// Called on a candidate element (or CDP search text node). Read layout and
// hit-test each shadow boundary; never scroll, reveal or activate a control in JS.
const CONTROL_POINT: &str = r#"function(labels) {
    const selector = 'button, a[href], input[type=button], input[type=submit], input[type=reset], [role=button]';
    const node = this.nodeType === 1 ? this : this.parentElement;
    const control = node?.closest(selector);
    if (!control || control.matches(':disabled') || control.getAttribute('aria-disabled') === 'true') return null;
    const normalize = text => (text || '').replace(/\s+/g, ' ').trim().toLowerCase();
    // Most controls on a news page are unrelated links. Reject those without
    // forcing style/layout for every ancestor. This is only a cheap prefilter;
    // matching candidates still require visible innerText and full hit testing.
    const possible = [control.textContent, control.getAttribute('aria-label'), control.getAttribute('title'), control.value].map(normalize);
    if (!labels.some(label => possible.some(text => text.includes(normalize(label))))) return null;
    for (let e = control; e; e = e.parentElement || e.getRootNode().host) {
        const style = getComputedStyle(e);
        if (e.inert || style.display === 'none' || style.contentVisibility === 'hidden' || Number(style.opacity) === 0) return null;
    }
    if (getComputedStyle(control).visibility !== 'visible') return null;
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
    let started = Instant::now();
    let result = s.evaluate(&format!(
        r#"(()=>{{
            const started = performance.now();
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
            return {{point:best, script_ms:performance.now()-started, visited:20000-remaining}};
        }})()"#,
        labels = json!(labels),
        point = CONTROL_POINT,
    ))?;
    log::debug!(
        "Visible consent probe: {} ms script, {:.1} ms round trip, {} visited",
        result["script_ms"],
        started.elapsed().as_secs_f64() * 1000.0,
        result["visited"]
    );
    Ok(result["point"].clone())
}

fn shadow_text_point(s: &mut Session, labels: &[&str]) -> Result<Value, String> {
    let mut search_id = None;
    let result: Result<Value, String> = (|| {
        s.call("DOM.getDocument", json!({"depth":0,"pierce":true}))?;
        for (index, label) in labels.iter().enumerate() {
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
                    let mut point = point["result"]["value"].clone();
                    point["index"] = json!(index);
                    return Ok(point);
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

// JS only locates a rendered control; activation stays trusted CDP input.
fn click_control_point(s: &mut Session, point: &Value) -> Result<(), String> {
    let scale = 720.0 / point["width"].as_f64().ok_or("Missing viewport width")?;
    s.action(Action::Click(
        point["x"].as_f64().ok_or("Missing control x")? * scale,
        point["y"].as_f64().ok_or("Missing control y")? * scale,
    ))?;
    let until = Instant::now() + Duration::from_millis(500);
    while Instant::now() < until {
        s.pump(Duration::from_millis(30))?;
    }
    Ok(())
}
#[cfg(test)]
fn click_text(s: &mut Session, labels: &[&str]) -> Result<bool, String> {
    let point = visible_text_point(s, labels)?;
    if !point.is_object() {
        return Ok(false);
    }
    click_control_point(s, &point)?;
    Ok(true)
}

fn news_interaction(
    s: &mut Session,
    selector: &str,
    checks: &mut Vec<&str>,
    article_link: bool,
    require_consent: bool,
) -> Result<String, String> {
    // Rejection has priority over opening purposes. Missing controls alone are
    // never a reason to index the hidden DOM when a normal article is clickable.
    let labels = [
        "Essential Cookies Only",
        "I DO NOT ACCEPT",
        "Reject All",
        "Show Purposes",
    ];
    let deadline = Instant::now() + Duration::from_secs(35);
    let expression = selector_point_expression(selector);
    let mut shadow_searches = 0;
    let mut next_shadow_search = Instant::now();
    let mut choices = 0;
    while Instant::now() < deadline {
        let mut control = visible_text_point(s, &labels)?;
        if !control.is_object() {
            if require_consent
                && !checks.contains(&"rejected optional consent through site control")
            {
                // Exercise a reader who waits for the site's own privacy UI.
                // Do not navigate away early merely because a link is ready.
                s.pump(Duration::from_millis(50))?;
                std::thread::sleep(Duration::from_millis(80));
                continue;
            }
            let point = s.evaluate(&expression)?;
            if point.is_object() && point["blocked"] == false {
                if !article_link {
                    // This replaces the heading wait with the same 35-second
                    // bound, while honoring consent that arrives on the article.
                    if point["text"]
                        .as_str()
                        .is_some_and(|text| text.trim().len() > 10)
                    {
                        return Ok(String::new());
                    }
                    s.pump(Duration::from_millis(50))?;
                    continue;
                }
                // The expression already revalidated this target after two
                // animation frames and a hit test. Dispatch input now: forcing a
                // diagnostic PNG here stalls the renderer and lets unrelated
                // layout/consent changes invalidate an otherwise usable link.
                return click_link_point(s, &point);
            }
            if article_link && point["overlay"] == true {
                // An asynchronous consent UI may have appeared during the link
                // probe. Try ALL visible choices again before closed-shadow search.
                control = visible_text_point(s, &labels)?;
                if !control.is_object()
                    && shadow_searches < 3
                    && Instant::now() >= next_shadow_search
                {
                    log::debug!(
                        "Stable intended link {} is visibly occluded; closed-shadow consent search",
                        point["key"]
                    );
                    shadow_searches += 1;
                    control = shadow_text_point(s, &labels)?;
                    next_shadow_search = Instant::now() + Duration::from_millis(500);
                }
            }
        }
        if control.is_object() {
            if choices == 6 {
                return Err("Consent controls did not expose an actionable article".into());
            }
            choices += 1;
            log::debug!(
                "Trusted visible consent choice: {}",
                labels[control["index"].as_u64().unwrap_or(0) as usize]
            );
            click_control_point(s, &control)?;
            // Poll the newly exposed UI before paying for another shadow search.
            next_shadow_search = Instant::now() + Duration::from_millis(500);
            if control["index"].as_u64().unwrap_or(0) < 3
                && !checks.contains(&"rejected optional consent through site control")
            {
                checks.push("rejected optional consent through site control");
            }
        } else {
            s.pump(Duration::from_millis(50))?;
            std::thread::sleep(Duration::from_millis(80));
        }
    }
    if require_consent && !checks.contains(&"rejected optional consent through site control") {
        return Err("The required visible consent flow did not finish within 35 seconds".into());
    }
    Err(format!(
        "News interaction timed out: no stable, unobstructed target {selector}"
    ))
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
        "cnn" | "cnn-consent" => "https://www.cnn.com/",
        "osm" => "https://www.openstreetmap.org/",
        _ => return Err("Choose slashdot, cnn, cnn-consent or osm".into()),
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
    if matches!(site, "cnn" | "cnn-consent")
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
        let selector = if site == "slashdot" {
            "a[href*='/story/']"
        } else {
            "a[href*='/202']:not([href*='/videos/']):not([href*='/live-news/'])"
        };
        let before = s.evaluate("location.href")?;
        let expected_article =
            news_interaction(&mut s, selector, &mut checks, true, site == "cnn-consent")?;
        let expected = serde_json::to_string(&expected_article).unwrap();
        wait(
            &mut s,
            &format!(
                "(()=>{{const wanted=new URL({expected});return location.pathname===wanted.pathname&&location.hostname.split('.').slice(-2).join('.')===wanted.hostname.split('.').slice(-2).join('.')&&location.href!=={before}&&document.readyState!=='loading'}})()"
            ),
        )?;
        news_interaction(&mut s, "h1", &mut checks, false, false)?;
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
                    "browser-check --site all|slashdot|cnn|cnn-consent|osm|sandbox --out NEW-DIRECTORY [--engine PATH]\nOpt-in real-site checks. Creates isolated profiles and public-page captures, never personal browser sessions."
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
        let point = shadow_text_point(&mut s, &["Closed reject"]).unwrap();
        assert!(point.is_object());
        click_control_point(&mut s, &point).unwrap();
        assert_eq!(s.evaluate("window.choice").unwrap(), true);
        assert_dom_disabled(&mut s);
        println!("PASS: closed shadow fallback, trusted click, DOM agent disabled after success");

        s.evaluate("window.closedButton.getClientRects=()=>{throw new Error('fixture layout failure')};true").unwrap();
        assert!(shadow_text_point(&mut s, &["Closed reject"]).is_err());
        assert_dom_disabled(&mut s);
        assert!(!click_text(&mut s, &["No such choice"]).unwrap());
        assert_dom_disabled(&mut s);
        println!("PASS: DOM agent disabled after inspection failure and missing label");
        println!("FIXTURE: normal listing with hidden consent tree");
        s.evaluate(r##"(()=>{
            document.body.innerHTML='<div id="vendors" hidden></div><a id="story" href="#article">Read intended story</a>';
            for(let i=0;i<25000;i++) {
                const button=document.createElement('button');button.textContent='Reject All';
                document.getElementById('vendors').append(button);
            }
            window.articleClick=null;window.buttonStates=[];
            for(const kind of ['pointerdown','pointerup']) document.getElementById('story').addEventListener(kind,e=>window.buttonStates.push([e.buttons,e.isTrusted]));
            document.getElementById('story').onclick=e=>window.articleClick=e.isTrusted;
            return true;
        })()"##).unwrap();
        let mut checks = vec![];
        assert!(
            news_interaction(&mut s, "#story", &mut checks, true, false)
                .unwrap()
                .ends_with("#article")
        );
        assert_eq!(s.evaluate("window.articleClick").unwrap(), true);
        assert!(checks.is_empty());
        assert_eq!(
            s.evaluate("window.buttonStates").unwrap(),
            json!([[1, true], [0, true]])
        );
        assert_dom_disabled(&mut s);
        println!("PASS: normal article click never indexes 25,000 hidden consent controls");

        println!("FIXTURE: visible Show Purposes before asynchronous rejection");
        s.evaluate(r#"(()=>{
            document.body.insertAdjacentHTML('beforeend',`<div id="consent" style="position:fixed;inset:0;background:white;z-index:10">
                <button id="purposes">Show Purposes</button><button id="later-reject" hidden>Reject All</button></div>`);
            window.consentClicks=[];
            document.getElementById('purposes').onclick=e=>{
                window.consentClicks.push(['purposes',e.isTrusted]);e.target.hidden=true;
                setTimeout(()=>document.getElementById('later-reject').hidden=false,750);
            };
            document.getElementById('later-reject').onclick=e=>{
                window.consentClicks.push(['reject',e.isTrusted]);document.getElementById('consent').remove();
            };
            window.articleClick=null;
            return true;
        })()"#).unwrap();
        let mut checks = vec![];
        news_interaction(&mut s, "#story", &mut checks, true, true).unwrap();
        assert_eq!(
            s.evaluate("window.consentClicks").unwrap(),
            json!([["purposes", true], ["reject", true]])
        );
        assert_eq!(s.evaluate("window.articleClick").unwrap(), true);
        assert!(checks.contains(&"rejected optional consent through site control"));
        assert_dom_disabled(&mut s);
        println!(
            "PASS: visible Show Purposes then delayed rejection and intended article, no DOM search"
        );

        println!("FIXTURE: consent arriving on the article");
        s.evaluate(r#"(()=>{
            document.body.insertAdjacentHTML('beforeend',`<h1>Intended rendered article heading</h1>
                <div id="late-consent" style="position:fixed;inset:0;background:white;z-index:10"><button>Reject All</button></div>`);
            window.lateChoice=null;window.articleClick=null;
            document.querySelector('#late-consent button').onclick=e=>{window.lateChoice=e.isTrusted;document.getElementById('late-consent').remove()};
            return true;
        })()"#).unwrap();
        news_interaction(&mut s, "h1", &mut checks, false, false).unwrap();
        assert_eq!(s.evaluate("window.lateChoice").unwrap(), true);
        assert_eq!(s.evaluate("window.articleClick").unwrap(), Value::Null);
        assert_dom_disabled(&mut s);
        println!("PASS: late visible consent honored before unobstructed heading verification");

        println!("FIXTURE: closed-shadow overlay blocks intended link");
        s.evaluate(r#"(()=>{
            document.getElementById('vendors').remove();
            const host=document.createElement('div');host.id='closed-overlay';
            host.style='position:fixed;inset:0;background:white;z-index:10';document.body.append(host);
            const root=host.attachShadow({mode:'closed'});
            root.innerHTML='<button style="width:180px;height:44px">Reject All</button>';
            window.closedChoice=null;window.articleClick=null;
            root.querySelector('button').onclick=e=>{window.closedChoice=e.isTrusted;host.remove()};
            return true;
        })()"#).unwrap();
        let point = s.evaluate(&selector_point_expression("#story")).unwrap();
        assert_eq!(point["overlay"], true);
        let mut checks = vec![];
        news_interaction(&mut s, "#story", &mut checks, true, false).unwrap();
        assert_eq!(s.evaluate("window.closedChoice").unwrap(), true);
        assert_eq!(s.evaluate("window.articleClick").unwrap(), true);
        assert_dom_disabled(&mut s);
        println!(
            "PASS: closed-shadow search requires blocked intended link, real choice, and DOM cleanup"
        );
        drop(s);
        std::fs::remove_dir_all(profile).unwrap();
    }
}
