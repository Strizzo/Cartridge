use cartridge_lua::api::register_json_api;
use mlua::Lua;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn app() -> Lua {
    let lua = Lua::new();
    register_json_api(&lua).unwrap();
    lua.load(r#"
        theme = setmetatable({ui='neo'}, {__index=function() return {r=220,g=210,b=200} end})
        drawn, requests, responses, saved = {}, {}, {}, {}
        rendering = false
        local function draw(text) assert(rendering); drawn[#drawn+1]=tostring(text); return 0 end
        screen = setmetatable({draw_text=draw, draw_display_text=draw,
            get_text_width=function() error('render measurement') end,
            get_display_text_width=function() error('render measurement') end,
            draw_rect=function(x,y,w,h) assert(rendering); assert(x>=0 and y>=0 and x+w<=720 and y+h<=720, 'rectangle outside display') end
        }, {__index=function() return function() assert(rendering) end end})
        storage = {load=function(k) return saved[k] end,save=function(k,v) assert(not rendering);saved[k]=v end}
        app = {set_idle_fps=function(n) assert(n==5) end,request_redraw=function() end}
        http = {
            get_async=function(url) assert(not rendering); requests[#requests+1]={url=url,method='GET'}; return #requests end,
            post_async=function(url,body) assert(not rendering); requests[#requests+1]={url=url,method='POST',body=json.decode(body)}; return #requests end,
            poll=function() assert(not rendering);local r=responses;responses={};return r end,
            get=function() error('blocking network') end,post=function() error('blocking network') end,
        }
        text_input={show=function(label,value) keyboard_label=label; keyboard_default=value end,
            poll=function() local r=keyboard_value; keyboard_value=nil;return r end}
        function press(key) on_input(key,'press') end
        function enter(value) keyboard_value=value;on_update(0) end
        function render_text() drawn={};rendering=true;on_render();rendering=false;return table.concat(drawn,'\n') end
        function has(s) assert(render_text():find(s,1,true), 'missing text: '..s..' in '..render_text()) end
        function respond(id,body,ok,status)
            responses[#responses+1]={id=id,body=type(body)=='table' and json.encode(body) or body,ok=ok~=false,status=status or 200,elapsed_ms=23}
        end
        function seed()
            saved.mission_control_settings={version=1,servers={{name='Station one',url='http://127.0.0.1:8766'},
                {name='Station two',url='http://127.0.0.2:8766'}},selected=1,interval=2}
        end
        function boot() seed();on_init();on_update(0);respond(1,fixture);on_update(0.1) end
        function refresh(body) press('x');on_update(0);respond(#requests,body);on_update(0.1) end
    "#).exec().unwrap();
    let modules = lua.create_table().unwrap();
    lua.globals().set("modules", modules.clone()).unwrap();
    lua.load("function require(name) return assert(modules[name],name) end")
        .exec()
        .unwrap();
    let ui: mlua::Table = lua.load(include_str!("../src/app_ui.lua")).eval().unwrap();
    lua.globals().set("ui", ui).unwrap();
    for name in ["model", "view"] {
        let module: mlua::Table = lua
            .load(
                std::fs::read_to_string(
                    root().join(format!("lua_cartridges/mission_control/{name}.lua")),
                )
                .unwrap(),
            )
            .eval()
            .unwrap();
        modules.set(name, module).unwrap();
    }
    let fixture: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("sim/fixtures/mission-control.json")).unwrap(),
    )
    .unwrap();
    lua.globals()
        .set("fixture_json", fixture[0]["body"].to_string())
        .unwrap();
    lua.load("fixture=json.decode(fixture_json)")
        .exec()
        .unwrap();
    lua.load(
        std::fs::read_to_string(root().join("lua_cartridges/mission_control/main.lua")).unwrap(),
    )
    .exec()
    .unwrap();
    lua
}

#[test]
fn settings_keyboard_validation_persistence_and_disconnect() {
    app().load(r#"
        on_init();on_update(0);assert(#requests==0);has('ADD YOUR FIRST SERVER')
        press('a');press('a');assert(keyboard_label=='Server name');enter('Control room')
        press('down');press('a');enter('example.test:0');has('Enter host[:port]')
        press('a');enter('example.test:8766');press('down');press('a')
        assert(saved.mission_control_settings.servers[1].url=='http://example.test:8766')
        assert(saved.mission_control_settings.servers[1].name=='Control room')
        press('a');on_update(0);assert(requests[1].url=='http://example.test:8766/api/state')
        press('start');press('start');assert(saved.mission_control_settings.interval==5)
        press('r2');respond(1,fixture);on_update(20);assert(#requests==1);has('DISCONNECTED')
        press('l2');has('REMOVE STATION?');press('a');assert(#saved.mission_control_settings.servers==1)
        press('l2');press('right');press('a');assert(#saved.mission_control_settings.servers==0)
        has('ADD YOUR FIRST SERVER')
    "#).exec().unwrap();
}

#[test]
fn async_polling_does_not_overlap_and_ignores_previous_server() {
    app()
        .load(
            r#"
        seed();on_init();on_update(0);assert(#requests==1)
        for i=1,30 do press('x');on_update(.1) end
        assert(#requests==1)
        press('start');press('down');press('a');on_update(0);assert(#requests==1)
        respond(1,fixture);on_update(0);assert(#requests==2);has('ESTABLISHING LINK')
        assert(requests[2].url=='http://127.0.0.2:8766/api/state')
        respond(2,{sessions={}});on_update(.1);has('NO UNITS HERE');has('LIVE')
        assert(not render_text():find('cartridge:0.0',1,true))
        on_update(2);assert(#requests==3)
        respond(3,'offline',false,0);on_update(.1);has('OFFLINE')
        on_update(.5);assert(#requests==3)
        press('x');on_update(0);assert(#requests==4)
        respond(4,'{"unrelated":true}');on_update(.1);has('OFFLINE')
        on_update(4);assert(#requests==5);respond(5,fixture);on_update(.1);has('LIVE')
    "#,
        )
        .exec()
        .unwrap();
}

#[test]
fn pane_identity_survives_reordering_and_closed_panes_cannot_receive_commands() {
    app()
        .load(
            r#"
        boot();press('a');has('cartridge:0.0')
        fixture.sessions['cartridge:0.0'].status='idle'
        fixture.sessions['relay:0.0'].status='waiting'
        refresh(fixture);has('cartridge:0.0');has('UNIT INSPECTOR')
        press('a');has('TRANSMIT COMMAND?')
        fixture.sessions['cartridge:0.0']=nil
        on_update(2);respond(#requests,fixture);on_update(.1);has('PANE CLOSED')
        local n=#requests;press('a');press('right');press('a');assert(#requests==n)
        press('b');has('MISSION CONTROL');press('a');has('relay:0.0')
    "#,
        )
        .exec()
        .unwrap();
}

#[test]
fn confirms_exact_supported_actions_and_validates_ack_without_retry() {
    app()
        .load(
            r#"
        boot();press('a');press('a');has('TRANSMIT COMMAND?')
        press('a');assert(#requests==1,'confirmation must default to cancel')
        press('a');press('right');press('a');assert(#requests==2)
        local sent=requests[2].body
        assert(sent.action=='send_response' and sent.session_id=='cartridge:0.0')
        assert(sent.payload.text==fixture.sessions['cartridge:0.0'].response_options[1].text)
        on_input('a','repeat');on_update(10);assert(#requests==2)
        respond(2,{type='ack',action='send_response',session_id='wrong-pane'});on_update(0)
        has('Delivery unknown');assert(#requests==3 and requests[3].method=='GET')
        respond(3,fixture);on_update(0)
        press('r2');press('r2');press('a');press('right');press('a')
        assert(requests[4].body.action=='send_keys' and requests[4].body.payload.keys=='1 Enter')
        respond(4,{type='ack',action='send_keys',session_id='cartridge:0.0'});on_update(.1)
        has('Server acknowledged send_keys')
        respond(5,fixture);on_update(.1)
        for i=1,5 do press('r2') end
        has('DISRUPTIVE');press('a');has('TRANSMIT COMMAND?');press('right');press('a')
        assert(requests[6].body.action=='interrupt')
        respond(6,{type='error',message='Unknown session'},false,400);on_update(0)
        has('Server rejected command: Unknown session')
    "#,
        )
        .exec()
        .unwrap();
}

#[test]
fn stale_prompt_and_pre_command_state_cannot_overwrite_new_context() {
    app()
        .load(
            r#"
        boot();press('a');press('a')
        fixture.sessions['cartridge:0.0'].response_options={}
        on_update(2);respond(2,fixture);on_update(0);has('Prompt or target changed')
        has('UNIT INSPECTOR');press('a');press('right');press('a')
        assert(requests[3].method=='POST')
        respond(3,{type='ack',action='send_keys',session_id='cartridge:0.0'});on_update(0)
        respond(4,fixture);on_update(0)
        press('x');on_update(0);assert(requests[5].method=='GET')
        press('a');press('right');press('a');assert(requests[6].method=='POST')
        respond(5,{sessions={}});on_update(0);has('UNIT INSPECTOR')
        assert(not render_text():find('PANE CLOSED',1,true))
        respond(6,'network lost',false,0);on_update(0);has('Delivery unknown')
        assert(requests[7].method=='GET')
    "#,
        )
        .exec()
        .unwrap();
}

#[test]
fn output_scrolling_filters_custom_keyboard_and_stale_health() {
    app().load(r#"
        for i=1,300 do fixture.sessions['cartridge:0.0'].screen_content[i]='line '..i..' '..string.rep('x',700) end
        boot();press('right');has('AGENTS');press('a');has('line 300')
        press('y');has('TERMINAL FEED');press('up');assert(not render_text():find('line 300',1,true))
        press('right');has('COL 13');press('a');has('line 300');has('COL 1')
        press('b');for i=1,4 do press('r2') end;press('a');assert(keyboard_label:find('Response to',1,true))
        enter('printf "ready"');has('TRANSMIT COMMAND?');press('right');press('a')
        assert(requests[2].body.payload.text=='printf "ready"')
        respond(2,{type='ack',action='send_response',session_id='cartridge:0.0'});on_update(0)
        on_update(11);has('STALE');local n=#requests;press('a');assert(#requests==n)
        press('start');press('r2');on_update(1);has('DISCONNECTED')
    "#).exec().unwrap();
}

#[test]
fn protocol_bounds_utf8_and_address_validation() {
    app().load(r#"
        local m=require('model')
        assert(m.url('[::1]:8766')=='http://[::1]:8766')
        assert(m.url('::1')=='http://[::1]:8766')
        assert(m.url('https://station.test')=='https://station.test:8766')
        for _,v in ipairs({'','host:99999','http://host/path','http://user:pass@host','host?x','host:abc'}) do assert(not m.url(v),v) end
        assert(m.clean('\27[31mHELLO\0',100)=='HELLO')
        assert(m.slice('aébc',2,2)=='éb')
        assert(not m.parse({sessions={bad=false}}))
        local data={sessions={}}
        for i=1,150 do data.sessions['p'..i]={session_name='p',pane_id='p'..i,screen_content={}} end
        local rows,err,total=m.parse(data);assert(#rows==128 and total==150 and not err)
        local s=fixture.sessions['cartridge:0.0']
        s.screen_content={};for i=1,300 do s.screen_content[i]=string.rep('é',700) end
        s.response_options={{text=string.rep('a',5000)}}
        rows=m.parse(fixture)
        assert(#rows[1].screen_content==240 and utf8.len(rows[1].screen_content[1])==512)
        assert(#rows[1].response_options==0,'oversized commands must never be truncated and sent')
        s.screen_content={'important output', '', '   ', ''}
        rows=m.parse(fixture);assert(#rows[1].screen_content==1 and rows[1].screen_content[1]=='important output')
        s.screen_content={'', '  '};rows=m.parse(fixture);assert(#rows[1].screen_content==0)
        assert(not m.url(string.rep('a',241)), 'never silently truncate an endpoint')
    "#).exec().unwrap();
}

#[test]
fn every_view_renders_without_measurements_network_or_out_of_bounds_primitives() {
    app()
        .load(
            r#"
        on_init();render_text();press('y');render_text();press('b');seed()
    "#,
        )
        .exec()
        .unwrap();
    app()
        .load(
            r#"
        boot();render_text();press('a');render_text();press('y');render_text();press('b')
        press('a');render_text();press('b');press('start');render_text();press('x');render_text()
    "#,
        )
        .exec()
        .unwrap();
}

// Explicit opt-in because this uses an external, read-only VibeBoy checkout and its Python environment.
#[test]
#[ignore = "set VIBEBOY_SOURCE and VIBEBOY_PYTHON to exercise the real Python HTTP service"]
fn current_vibeboy_http_service_integration() {
    use cartridge_lua::api::{new_app_control, register_http_api};
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let source = std::env::var("VIBEBOY_SOURCE").expect("VIBEBOY_SOURCE");
    let python = std::env::var("VIBEBOY_PYTHON").unwrap_or_else(|_| "python3".into());
    let records = std::env::temp_dir().join(format!(
        "mission-control-actions-{}.json",
        std::process::id()
    ));
    let child = Command::new(python)
        .arg(root().join("lua_cartridges/mission_control/tests/service_fixture.py"))
        .args(["--source", &source, "--records", records.to_str().unwrap()])
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    struct Service(std::process::Child);
    impl Drop for Service {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut service = Service(child);
    let mut address = String::new();
    BufReader::new(service.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    assert!(
        address.trim().starts_with("http://127.0.0.1:"),
        "service startup: {address}"
    );
    let lua = app();
    register_http_api(&lua, "mission-control-integration", new_app_control()).unwrap();
    lua.globals().set("service_url", address.trim()).unwrap();
    lua.load(
        "seed();saved.mission_control_settings.servers[1].url=service_url;on_init();on_update(0)",
    )
    .exec()
    .unwrap();
    let wait_text = |needle: &str| {
        for _ in 0..250 {
            lua.load("on_update(.02)").exec().unwrap();
            if lua
                .load("render_text()")
                .eval::<String>()
                .unwrap()
                .contains(needle)
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!(
            "timeout waiting for {needle}: {}",
            lua.load("render_text()").eval::<String>().unwrap()
        );
    };
    wait_text("LIVE");
    lua.load("press('a');press('a');press('right');press('a')")
        .exec()
        .unwrap();
    wait_text("Server acknowledged send_response");
    let recorded: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&records).unwrap()).unwrap();
    assert_eq!(recorded[0]["action"], "send_response");
    assert_eq!(recorded[0]["pane_id"], "cartridge:0.0");
    assert_eq!(
        recorded[0]["value"],
        "Run the focused tests, then report any failures."
    );
    // Exercise a parsed prompt choice through the real dispatcher; no actual tmux is controlled.
    lua.load("press('r2');press('r2');press('a');press('right');press('a')")
        .exec()
        .unwrap();
    wait_text("Server acknowledged send_keys");
    let recorded: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&records).unwrap()).unwrap();
    assert_eq!(recorded[1]["value"], "1 Enter");
    let _ = std::fs::remove_file(records);
}

// Run individually with --ignored --test-threads=1 on a desktop with SDL/WindowServer.
#[test]
#[ignore = "requires SDL display; captures real 720px renderer via app-check"]
fn renderer_screenshots() {
    let status = std::process::Command::new("python3")
        .arg(root().join("lua_cartridges/mission_control/tests/render.py"))
        .status()
        .unwrap();
    assert!(status.success(), "standalone renderer failed");
}
