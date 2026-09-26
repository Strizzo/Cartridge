use cartridge_lua::api::register_json_api;
use mlua::Lua;

fn app(name: &str) -> Lua {
    let lua = Lua::new();
    register_json_api(&lua).unwrap();
    lua.load(r#"
        theme = setmetatable({}, {__index=function() return {r=220,g=210,b=200} end})
        drawn = {}
        storage = {load=function() return nil end,save=function() end}
        screen = setmetatable({
            draw_text=function(text) drawn[#drawn+1]=text; return #text*7 end,
            draw_pill=function(text) drawn[#drawn+1]=text; return #text*7 end,
            draw_image=function(path) drawn[#drawn+1]=path end,
            get_text_width=function(text) return #text*7 end,
            get_line_height=function() return 18 end,
        }, {__index=function() return function() return 0 end end})
        requests, responses, sync_calls = {}, {}, 0
        http = {
            get_async=function(url) requests[#requests+1]=url; return #requests end,
            poll=function() local r=responses; responses={}; return r end,
        }
        for _,name in ipairs({'get','get_cached','post'}) do
            http[name]=function() sync_calls=sync_calls+1; error('blocking HTTP used') end
        end
        function respond(id, body, ok, elapsed)
            responses[#responses+1]={id=id,body=body,ok=ok ~= false,status=ok == false and 0 or 200,elapsed_ms=elapsed or 25}
        end
        function press(key) on_input(key,'press') end
        function render_text() drawn={}; on_render(); return table.concat(drawn,'\n') end
    "#).exec().unwrap();
    let ui: mlua::Table = lua.load(include_str!("../src/app_ui.lua")).eval().unwrap();
    lua.globals().set("ui", ui).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lua_cartridges");
    lua.load(std::fs::read_to_string(root.join(name).join("main.lua")).unwrap())
        .exec()
        .unwrap();
    lua
}

#[test]
fn net_tool_keeps_tabs_live_and_reports_incremental_wall_time_results() {
    app("network_tool").load(r#"
        on_init(); assert(#requests==1)
        for i=1,20 do press('x') end
        assert(#requests==1,'refresh must not duplicate pending requests')
        press('r1'); press('r1'); press('a') -- DNS while IP pending
        assert(#requests==6)
        press('x'); assert(#requests==6)
        respond(3, '{"Status":0,"Answer":[{"type":1,"data":"192.0.2.7"}]}')
        on_update(0.1)
        assert(render_text():find('192.0.2.7',1,true),'completed DNS rows must appear before the slowest request')
        press('r1'); press('a'); assert(#requests==12)
        press('x'); assert(#requests==12)
        respond(7,'',true,4321.8); on_update(0.1)
        assert(render_text():find('4321ms',1,true),'probe must use monotonic worker timing')
        for i=8,12 do respond(i,'offline',false,10) end
        on_update(0.1); press('x'); assert(#requests==18,'completed failures must be retryable')
        assert(sync_calls==0)
    "#).exec().unwrap();
}

#[test]
fn papers_ignore_cancelled_reader_results_and_allow_failed_load_retry() {
    app("ai_papers").load(r#"
        on_init(); press('x'); assert(#requests==1)
        respond(1,'offline',false); on_update(0.1)
        assert(render_text():find('Failed to load papers',1,true))
        press('x'); assert(#requests==2)
        respond(2,'[{"paper":{"id":"first","title":"First","authors":[]}},{"paper":{"id":"second","title":"Second","authors":[]}}]')
        on_update(0.1); press('a'); press('x'); assert(#requests==3)
        press('b'); press('b'); press('dpad_down'); press('a'); press('x'); assert(#requests==4)
        respond(4,'<p>NEW &#945; result</p>'); on_update(0.1)
        assert(render_text():find('NEW α result',1,true))
        respond(3,'<p>OLD cancelled result</p>'); on_update(0.1)
        local text=render_text()
        assert(text:find('NEW α result',1,true) and not text:find('OLD',1,true))
        assert(sync_calls==0)
    "#).exec().unwrap();
}

#[test]
fn weather_retries_without_duplicates_and_ignores_previous_city() {
    app("weather")
        .load(
            r#"
        on_init(); assert(#requests==2)
        for i=1,20 do press('x') end
        assert(#requests==2,'weather refresh duplicated pending requests')
        respond(1,'offline',false);respond(2,'offline',false);on_update(0.1)
        press('x');assert(#requests==4,'failed weather request must be retryable')
        press('r1');press('r1');press('dpad_down');press('a');assert(#requests==6)
        function current(temp)
            return json.encode({current={temperature_2m=temp,apparent_temperature=temp,
                relative_humidity_2m=60,wind_speed_10m=12,surface_pressure=1013,weather_code=61}})
        end
        respond(5,current(23));respond(6,'{"daily":{"time":[]}}');on_update(0.1)
        press('l1');press('l1')
        local before=render_text()
        assert(before:find('London',1,true) and before:find('+23',1,true))
        assert(before:find('assets/conditions/rain.png',1,true))
        respond(3,current(99));respond(4,'{"daily":{"time":[]}}');on_update(0.1)
        local after=render_text()
        assert(after:find('+23',1,true) and not after:find('+99',1,true))
        assert(sync_calls==0)
    "#,
        )
        .exec()
        .unwrap();
}
