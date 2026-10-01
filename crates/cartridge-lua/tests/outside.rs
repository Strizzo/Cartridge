use cartridge_lua::api::register_json_api;
use mlua::Lua;

fn app() -> Lua {
    let lua = Lua::new();
    register_json_api(&lua).unwrap();
    let modules = lua.create_table().unwrap();
    modules
        .set(
            "model",
            include_str!("../../../lua_cartridges/outside/model.lua"),
        )
        .unwrap();
    modules
        .set(
            "view",
            include_str!("../../../lua_cartridges/outside/view.lua"),
        )
        .unwrap();
    lua.globals().set("sources", modules).unwrap();
    lua.globals()
        .set(
            "fixture_json",
            include_str!("../../../sim/fixtures/outside.json"),
        )
        .unwrap();
    lua.load(r#"
        local modules={}
        function require(name)
            if not modules[name] then modules[name]=assert(load(sources[name],name))() end
            return modules[name]
        end
        os.time=function() return 1790856900 end
        app={request_redraw=function() end,set_idle_fps=function() end}
        store={};writes=0;rendering=false;drawn={};requests={};responses={};keyboard_result=nil
        storage={load=function(key) assert(not rendering);return store[key] end,
            save=function(key,value) assert(not rendering);store[key]=json.decode(json.encode(value));writes=writes+1 end}
        text_input={show=function() keyboard_shown=true end,poll=function() local r=keyboard_result;keyboard_result=nil;return r end,
            is_active=function() return false end}
        screen=setmetatable({
            clear=function() assert(rendering) end,
            draw_text=function(s) assert(rendering);drawn[#drawn+1]=tostring(s) end,
            draw_display_text=function(s) assert(rendering);drawn[#drawn+1]=tostring(s) end,
            get_text_width=function() error('render measurements forbidden') end,
            get_display_text_width=function() error('render measurements forbidden') end,
        },{__index=function() return function() assert(rendering) end end})
        http={get_async=function(url) assert(not rendering);if fail_submit then error('queue full') end;requests[#requests+1]=url;return #requests end,
            poll=function() assert(not rendering);local r=responses;responses={};return r end,
            get=function() error('blocking request') end,get_cached=function() error('blocking request') end}
        function respond(id,body,ok)
            responses[#responses+1]={id=id,ok=ok~=false,status=ok==false and 0 or 200,body=type(body)=='string' and body or json.encode(body)}
        end
        function press(key) on_input(key,'press') end
        function render_text()
            drawn={};local before=#requests;rendering=true;on_render();rendering=false
            assert(#requests==before,'render started HTTP');return table.concat(drawn,'\n')
        end
        function contains(s) assert(render_text():find(s,1,true),'missing text: '..s..'\n'..render_text()) end
        fixture=json.decode(fixture_json)[1].body
        function ready() on_init();respond(1,fixture);on_update(.1) end
        function prefs()
            store.preferences={version=1,units='metric',places={
                {name='Luxembourg',lat=49.6116,lon=6.1319,country='LU'},
                {name='Tokyo',lat=35.6762,lon=139.6503,country='JP'}},selected='49.6116,6.1319'}
        end
    "#).exec().unwrap();
    lua.load(include_str!("../../../lua_cartridges/outside/main.lua"))
        .exec()
        .unwrap();
    lua
}

#[test]
fn outside_live_contract_and_controller_scrubbing_are_render_pure() {
    app().load(r#"
        on_init();contains('LOOKING OUT');assert(#requests==1)
        assert(requests[1]:find('timezone=auto',1,true));assert(requests[1]:find('forecast_days=7',1,true))
        assert(requests[1]:find('wind_gusts_10m',1,true));assert(requests[1]:find('sunrise,sunset',1,true))
        for i=1,30 do press('x') end;assert(#requests==1)
        respond(1,fixture);on_update(.1);contains('CURRENT CONDITIONS');contains('CLEAR SKY')
        for _,day in ipairs({'THU','FRI','SAT','SUN','MON','TUE','WED'}) do contains(day) end
        press('r1');contains('15:00');contains('FORECAST')
        press('dpad_right');contains('RAIN');press('dpad_right');contains('SNOW')
        press('a');contains('CLOSER LOOK');contains('07:35');contains('19:20');contains('UV MAX')
        press('b');press('b');contains('CURRENT CONDITIONS')
        assert(#requests==1,'navigation must reuse fetched hours')
        local before=writes;render_text();render_text();assert(writes==before)
    "#).exec().unwrap();
}

#[test]
fn outside_late_city_results_never_replace_the_selected_location() {
    app().load(r#"
        prefs();on_init();press('dpad_down');assert(#requests==2)
        local tokyo=json.decode(json.encode(fixture));tokyo.timezone='Asia/Tokyo';tokyo.current.temperature_2m=31
        respond(2,tokyo);on_update(.1);contains('TOKYO');contains('31')
        fixture.current.temperature_2m=-88;respond(1,fixture);on_update(.1)
        contains('TOKYO');assert(not render_text():find('-88',1,true))
        press('dpad_up');contains('-88');assert(#requests==2,'fresh per-place cache should be reused')
        assert(store.forecasts.entries['49.6116,6.1319']);assert(store.forecasts.entries['35.6762,139.6503'])
    "#).exec().unwrap();
}

#[test]
fn outside_offline_cache_retry_and_restart_restore_real_data() {
    let lua = app();
    lua.load(
        r#"
        ready();press('y');press('r2');press('b');press('x')
        respond(2,'offline',false);on_update(.1);contains('STALE CACHE');contains('Offline')
        saved=json.encode(store)
    "#,
    )
    .exec()
    .unwrap();
    let saved: String = lua.globals().get("saved").unwrap();
    let restarted = app();
    restarted.globals().set("saved", saved).unwrap();
    restarted.load(r#"
        store=json.decode(saved);on_init();contains('CACHED FORECAST');contains('°F');assert(#requests==1)
        respond(1,'offline',false);on_update(.1);contains('STALE CACHE');contains('CLEAR SKY')
        press('x');assert(#requests==2);respond(2,fixture);on_update(.1)
        assert(not render_text():find('STALE CACHE',1,true));contains('OPEN-METEO')
    "#).exec().unwrap();
}

#[test]
fn outside_search_keyboard_late_results_encoding_add_and_duplicates() {
    app().load(r#"
        ready();press('y');press('y');assert(keyboard_shown)
        keyboard_result='São Paulo & coast';on_update(.1)
        assert(requests[2]:find('S%C3%A3o%20Paulo%20%26%20coast',1,true))
        press('b');press('y');keyboard_result='Tokyo';on_update(.1);assert(#requests==3)
        respond(2,{results={{name='Old result',latitude=1,longitude=2}}});on_update(.1)
        assert(not render_text():find('Old result',1,true))
        local results={results={{name='Tokyo',latitude=35.6762,longitude=139.6503,country_code='JP',admin1='Tokyo'}}}
        respond(3,results);on_update(.1);contains('Tokyo');press('a');contains('TOKYO')
        assert(#store.preferences.places==2);assert(#requests==4)
        press('y');press('y');keyboard_result='Tokyo';on_update(.1)
        respond(5,results);on_update(.1);press('a');assert(#store.preferences.places==2)
        assert(#requests==5,'same place must not duplicate pending forecast')
    "#).exec().unwrap();
}

#[test]
fn outside_saved_places_reorder_remove_cancel_and_empty_persist() {
    let lua = app();
    lua.load(r#"
        prefs();ready();press('y');press('dpad_down');press('l1')
        assert(store.preferences.places[1].name=='Tokyo');assert(store.preferences.selected=='49.6116,6.1319')
        press('x');contains('REMOVE PLACE?');press('b');assert(#store.preferences.places==2)
        press('x');press('a');assert(#store.preferences.places==1)
        press('x');press('a');assert(#store.preferences.places==0)
        press('b');contains('FIND YOUR OUTSIDE');press('select');contains('FIND YOUR OUTSIDE')
        saved=json.encode(store)
    "#).exec().unwrap();
    let restarted = app();
    restarted
        .globals()
        .set("saved", lua.globals().get::<String>("saved").unwrap())
        .unwrap();
    restarted
        .load(
            "store=json.decode(saved);on_init();contains('FIND YOUR OUTSIDE');assert(#requests==0)",
        )
        .exec()
        .unwrap();
}

#[test]
fn outside_defends_against_malformed_null_partial_and_failed_responses() {
    app().load(r#"
        on_init();respond(1,'{');on_update(.1);contains('NO FORECAST')
        press('x');respond(2,'{"hourly":{"time":[]},"daily":{"time":[]}}');on_update(.1);contains('NO FORECAST')
        press('x');fixture.hourly.temperature_2m[16]=false;fixture.hourly.weather_code[16]=false
        fixture.daily.sunrise[1]=false;fixture.current.apparent_temperature=false
        respond(3,fixture);on_update(.1);contains('Feels --');contains('RISE --:--')
        press('r1');contains('UNAVAILABLE');press('a');contains('--')
        press('x');respond(4,'{"error":true,"reason":"service"}');on_update(.1);contains('STALE CACHE')
        press('y');press('y');keyboard_result='Nope';on_update(.1)
        respond(5,{results={false,{name='Missing coordinates'},{name='Bad latitude',latitude=999,longitude=0}}});on_update(.1)
        contains('No places found');press('y');keyboard_result=false;on_update(.1);contains('YOUR PLACES')
    "#).exec().unwrap();
}

#[test]
fn outside_local_dates_do_not_depend_on_device_timezone_or_fixed_24_hour_days() {
    app().load(r#"
        local M=require('model');assert(M.weekday('2026-10-01')=='THU');assert(M.weekday('2024-02-29')=='THU')
        -- Simulate DST: duplicate a wall-clock hour. Each day's indices come
        -- from its actual timestamps, never day*24 indexing.
        for k,v in pairs(fixture.hourly) do table.insert(v,27,v[27]) end
        fixture.timezone='Pacific/Auckland';fixture.current.time='2026-10-02T00:15'
        on_init();respond(1,fixture);on_update(.1);contains('2026-10-02');contains('Pacific/Auckland')
        press('r1');contains('FRI  /  01:00');press('dpad_right');contains('SAT  /  01:00')
        press('l1');contains('SAT  /  00:00');press('l1');contains('FRI  /  23:00')
        for i=1,220 do press('r2') end;contains('WED  /  23:00')
        for i=1,220 do press('l2') end;contains('THU  /  00:00')
    "#).exec().unwrap();
}

#[test]
fn outside_submission_failure_timeout_and_late_retry_are_safe() {
    app().load(r#"
        fail_submit=true;on_init();contains('Request unavailable');assert(#requests==0)
        fail_submit=false;press('x');assert(#requests==1);on_update(36);contains('Offline')
        press('x');assert(#requests==2)
        local newer=json.decode(json.encode(fixture));newer.current.temperature_2m=33
        respond(2,newer);on_update(.1);respond(1,fixture);on_update(.1);contains('33')
        press('y');press('y');keyboard_result='Tokyo';on_update(.1);on_update(36);contains('Search failed')
        press('x');assert(#requests==4)
    "#).exec().unwrap();
}

#[test]
fn outside_capacity_cached_payloads_and_corrupt_storage_are_bounded() {
    app().load(r#"
        prefs();for i=3,40 do store.preferences.places[i]={name='City '..i,lat=i,lon=i} end
        store.forecasts={version=1,entries={bad='not a forecast'}}
        on_init();press('y');press('r2');assert(#store.preferences.places==8)
        press('y');keyboard_result='Another';on_update(.1)
        respond(2,{results={{name='Another',latitude=-20,longitude=100}}});on_update(.1);press('a');contains('Eight places saved')
        local huge=json.decode(json.encode(fixture))
        for i=1,300 do huge.hourly.time[#huge.hourly.time+1]='2099-01-01T01:00' end
        respond(1,huge);on_update(.1)
        assert(#store.forecasts.entries['49.6116,6.1319'].raw.hourly.time<=180)
        local n=0;for _ in pairs(store.forecasts.entries) do n=n+1 end;assert(n<=8)
    "#).exec().unwrap();
}

#[test]
fn outside_expired_cache_preserves_scrub_selection_during_background_refresh() {
    app().load(r#"
        ready();press('r1');press('dpad_right');contains('FRI  /  15:00')
        os.time=function() return 1790858900 end;on_update(30);assert(#requests==2);contains('CACHED FORECAST')
        respond(2,fixture);on_update(.1);contains('FRI  /  15:00')
        press('a');contains('CLOSER LOOK');on_input('a','repeat');contains('CLOSER LOOK')
    "#).exec().unwrap();
}

#[test]
fn outside_silent_storage_failure_is_visible_and_data_stays_usable() {
    app()
        .load(
            r#"
        local real_save=storage.save;storage.save=function() end
        ready();contains('Storage unavailable');contains('CLEAR SKY')
        press('r1');contains('15:00')
        storage.save=real_save;press('x');respond(2,fixture);on_update(.1)
        assert(not render_text():find('Storage unavailable',1,true));assert(store.forecasts)
    "#,
        )
        .exec()
        .unwrap();
}
