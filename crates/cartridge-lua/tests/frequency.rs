use cartridge_lua::api::register_json_api;
use mlua::Lua;

fn app() -> Lua {
    let lua = Lua::new();
    register_json_api(&lua).unwrap();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lua_cartridges/frequency");
    for module in ["station", "directory", "view"] {
        let source = std::fs::read_to_string(root.join(format!("{module}.lua"))).unwrap();
        let value: mlua::Value = lua.load(source).set_name(module).eval().unwrap();
        let package: mlua::Table = lua.globals().get("package").unwrap();
        let loaded: mlua::Table = package.get("loaded").unwrap();
        loaded.set(module, value).unwrap();
    }
    lua.load(r#"
        saved, drawn, requests, responses, plays, pauses = {}, {}, {}, {}, {}, {}
        stops, sync_calls, redraws, clicks = 0, 0, 0, 0
        native = {state='stopped',url='',error='',volume=0.7,seconds=0}
        storage={load=function(k) return saved[k] and json.decode(saved[k]) end,
            save=function(k,v) if save_error then error('disk full') end; saved[k]=json.encode(v) end}
        app={set_idle_fps=function(n) idle_fps=n end,request_redraw=function() redraws=redraws+1 end}
        screen=setmetatable({draw_text=function(t) drawn[#drawn+1]=t end,
            draw_display_text=function(t) drawn[#drawn+1]=t end,
            draw_image=function(p) drawn[#drawn+1]=p end}, {__index=function() return function() end end})
        http={get_async=function(url)
            if queue_full then error('request queue full') end
            local id=#requests+1;requests[id]=url
            if url:find('/json/url/',1,true) then clicks=clicks+1 end
            return id
        end,poll=function() local r=responses;responses={};return r end}
        for _,k in ipairs({'get','get_cached','post'}) do http[k]=function() sync_calls=sync_calls+1;error('sync HTTP') end end
        audio={stream=function(url)
            if audio_throw then error('invalid stream URL') end
            plays[#plays+1]=url;native={state='connecting',url=url,error='',volume=0.7}
        end,stream_status=function() return native end,
        pause_stream=function(p) pauses[#pauses+1]=p;native.state=p and 'paused' or 'playing' end,
        stop_stream=function() stops=stops+1;native={state='stopped',url='',error=''} end,
        set_stream_volume=function(v) assert(v>=0 and v<=1);native.volume=v;last_volume=v end}
        text_input={show=function(label,initial) keyboard_label=label;keyboard_initial=initial;keyboard_active=true end,
            is_active=function() return keyboard_active end,
            poll=function() local r=keyboard_result;keyboard_result=nil;return r end}
        function submit(t) keyboard_active=false;keyboard_result=t;on_update(0.3) end
        function press(k) on_input(k,'press') end
        function render_text() drawn={};on_render();return table.concat(drawn,'\n') end
        function respond(id,body,ok)
            responses[#responses+1]={id=id,ok=ok~=false,status=ok==false and 503 or 200,body=type(body)=='string' and body or json.encode(body)}
        end
        function latest(part)
            for i=#requests,1,-1 do if requests[i]:find(part,1,true) then return i end end
            error('missing request: '..part)
        end
        function station(id,name,codec,lat,lon)
            return {stationuuid=id,name=name or id,codec=codec or 'MP3',hls=0,lastcheckok=1,
                url='https://example.invalid/original.pls',url_resolved='https://example.invalid/'..id,
                country='United Kingdom',countrycode='GB',geo_lat=lat,geo_long=lon}
        end
        function init_directory()
            on_init();assert(#plays==0,'startup must not autoplay')
            respond(latest('dns.google'),{Status=0,Answer={{type=33,data='1 1 443 nl1.api.radio-browser.info.'}}})
            on_update(0.1)
            assert(requests[latest('/stations/search')]:find('nl1.api.radio-browser.info',1,true))
        end
        function populate(rows)
            init_directory();respond(latest('/stations/search'),rows);on_update(0.1)
        end
        function menu(n)
            press('start');for i=2,n do press('dpad_down') end;press('a')
        end
    "#).exec().unwrap();
    let source = std::fs::read_to_string(root.join("main.lua")).unwrap();
    lua.globals().set("main_source", source.clone()).unwrap();
    lua.load(source)
        .set_name("frequency/main.lua")
        .exec()
        .unwrap();
    lua
}

#[test]
fn resolved_streams_native_states_clicks_and_teardown() {
    app().load(r#"
        populate({station('aac','AAC station','AAC'),station('mp3','MP3 station','MP3',51,0)})
        assert(#plays==0 and clicks==0 and sync_calls==0)
        assert(render_text():find('2 candidates',1,true))
        press('a');assert(plays[1]=='https://example.invalid/mp3','MP3 comes first; use resolved URL')
        assert(clicks==1)
        native.state='playing';on_update(0.1);assert(render_text():find('PLAYING',1,true))
        press('a');on_update(0.1);assert(pauses[1]==true and render_text():find('PAUSED',1,true))
        press('a');on_update(0.1);assert(pauses[2]==false and clicks==1 and #plays==1)
        native.state='buffering';on_update(0.1);assert(render_text():find('BUFFERING',1,true))
        native.state='error';native.error='unsupported station codec';on_update(0.1)
        assert(render_text():find('unsupported station codec',1,true))
        press('a');assert(#plays==2 and clicks==2)
        assert(#json.decode(saved['frequency.v1']).recent==1,'retry deduplicates history')
        press('b');assert(stops==1)
        press('select');assert(stops==1,'Select is runtime-owned')
        on_destroy();assert(stops==2)
        assert(idle_fps==5 and sync_calls==0)
    "#).exec().unwrap();
}

#[test]
fn rejects_unsupported_streams_invalid_geo_and_invented_locations() {
    app().load(r#"
        local hls=station('hls');hls.hls=1
        local unresolved=station('playlist');unresolved.url_resolved=nil
        local suffix=station('suffix');suffix.url_resolved='https://example.invalid/a.m3u8?token=1'
        populate({hls,unresolved,suffix,station('he','HE','AAC+'),station('opus','Opus','OPUS'),
            station('unknown','Unknown',''),station('aac','AAC','AAC',999,10),station('known','Known','MP3',0,0)})
        local rendered=render_text();assert(rendered:find('2 candidates / 8 checked / 6 omitted',1,true))
        assert(rendered:find('1 reported locations  /  1 without coordinates',1,true))
        local m=require('station')
        local x,y=m.project(m.normalize(station('zero','Zero','MP3',0,0)))
        assert(x==360 and y==276,'zero coordinates are valid and correctly projected')
        assert(m.project(m.normalize(station('none')))==nil)
        assert(m.normalize(station('bad','Bad','MP3',0/0,180)).geo_lat==nil)
        press('dpad_down');press('dpad_right');assert(render_text():find('MAP / D-PAD',1,true))
        press('a');assert(plays[1]=='https://example.invalid/known','map chooses actual pin only')
    "#).exec().unwrap();
}

#[test]
fn generations_keyboard_submission_debounce_and_pagination() {
    app().load(r#"
        init_directory();local original=latest('/stations/search')
        press('y');submit('a & b?');on_update(0.3)
        local current=latest('/stations/search');assert(current~=original)
        assert(requests[current]:find('&name=a%20%26%20b%3F',1,true),'search must be percent encoded')
        respond(original,{station('late','Wrong old query')});on_update(0.1)
        assert(not render_text():find('Wrong old query',1,true))
        respond(current,{station('fresh','Fresh result')});on_update(0.1)
        assert(render_text():find('Fresh result',1,true))
        local count=#requests
        for i=1,30 do press('r1');on_update(0.01) end
        assert(#requests==count,'held genre changes are debounced')
        on_update(0.4);assert(#requests==count+1)
        local older=latest('/stations/search')
        press('y');submit('new');on_update(0.3);local newer=latest('/stations/search')
        count=#requests;respond(older,'offline',false);on_update(0.1)
        assert(#requests==count,'failed stale queries must not start failover')
        local rows={};for i=1,100 do rows[i]=station('page'..i) end
        respond(newer,rows);on_update(0.1)
        menu(7);on_update(0.3);assert(requests[latest('/stations/search')]:find('limit=100&offset=100',1,true))
        respond(latest('/stations/search'),'[]');on_update(0.1)
        assert(render_text():find('No stations found.',1,true))
        menu(8);on_update(0.3);assert(requests[latest('/stations/search')]:find('offset=0',1,true))
        press('y');submit(false);assert(sync_calls==0)
    "#).exec().unwrap();
}

#[test]
fn mirror_validation_failover_bad_json_offline_and_retry() {
    app().load(r#"
        local D=require('directory')
        for _,h in ipairs({'evilradio-browser.info','x.radio-browser.info.attacker.test','x.radio-browser.info/path','x..radio-browser.info','https://de1.api.radio-browser.info','x.radio-browser.info:443','-x.radio-browser.info'}) do assert(not D.valid_host(h),h) end
        assert(D.valid_host('DE1.API.RADIO-BROWSER.INFO.')=='de1.api.radio-browser.info')
        on_init();respond(latest('dns.google'),{Status=0,Answer={{type=33,data='1 1 443 attacker.test.'},{type=33,data='1 1 80 de1.api.radio-browser.info.'}}});on_update(0.1)
        local first=latest('/stations/search');respond(first,'{"error":"bad shape"}');on_update(0.1)
        local second=latest('/stations/search');assert(first~=second and requests[first]~=requests[second])
        respond(second,'{bad json');on_update(0.1)
        respond(latest('/stations/search'),'offline',false);on_update(0.1)
        assert(render_text():find('OFFLINE / Directory unavailable',1,true))
        press('a');on_update(0.3);respond(latest('/stations/search'),{station('recovered','Recovered')});on_update(0.1)
        assert(render_text():find('Recovered',1,true))
        menu(6);on_update(0.3)
        for i=1,3 do respond(latest('/stations/search'),'failed',false);on_update(0.1) end
        assert(render_text():find('OFFLINE / Saved page may be stale',1,true))
        assert(render_text():find('Recovered',1,true),'same-query failure preserves data')
        assert(#plays==0)
    "#).exec().unwrap();
}

#[test]
fn favorites_recent_volume_and_last_selection_survive_restart_without_autoplay() {
    app().load(r#"
        populate({station('one','One'),station('two','Two')})
        press('dpad_down');press('x');press('a');native.state='playing';on_update(0.1)
        for i=1,25 do press('r2') end;assert(last_volume==1)
        for i=1,25 do press('l2') end;assert(last_volume==0)
        local data=json.decode(saved['frequency.v1'])
        assert(data.version==1 and data.volume==0 and #data.favorites==1 and #data.recent==1 and data.last_station.stationuuid=='two')
        on_destroy();plays={};assert(load(main_source))();on_init()
        assert(#plays==0 and last_volume==0)
        menu(3);assert(render_text():find('Two',1,true));press('x')
        assert(render_text():find('Your collection starts here.',1,true))
        assert(#json.decode(saved['frequency.v1']).favorites==0)
        press('b');menu(4);assert(render_text():find('Two',1,true))
        press('a');assert(plays[1]=='https://example.invalid/two')
    "#).exec().unwrap();
}

#[test]
fn country_filters_custom_streams_errors_and_static_idle() {
    app().load(r#"
        populate({station('one','One')})
        press('dpad_left');respond(latest('/json/countries'),{{name='France',iso_3166_1='FR',stationcount=7},{name='Japan',iso_3166_1='JP',stationcount=5}});on_update(0.1)
        press('dpad_down');press('a');on_update(0.3)
        assert(requests[latest('/stations/search')]:find('&countrycode=FR',1,true))
        respond(latest('/stations/search'),'[]');on_update(0.1)
        menu(5);press('dpad_down');press('a');submit('file:///etc/passwd')
        assert(#plays==0 and render_text():find('direct HTTP(S)',1,true))
        press('a');submit('https://example.invalid/custom.mp3')
        assert(plays[1]=='https://example.invalid/custom.mp3' and clicks==0)
        native.state='playing';on_update(0.1)
        local r=redraws;on_update(0.1);assert(redraws==r,'static idle must not invalidate frames')
        menu(5);press('a');audio_throw=true;submit('https://example.invalid/broken')
        assert(render_text():find('invalid stream URL',1,true))
        on_update(0.1);assert(render_text():find('invalid stream URL',1,true),'local error cannot be overwritten by stale status')
    "#).exec().unwrap();
}

#[test]
fn backpressure_corrupt_storage_empty_compatibility_and_geographic_navigation() {
    app().load(r#"
        saved['frequency.v1']='{"version":1,"volume":"bad","favorites":42,"recent":[null,false,{}],"last_station":false}'
        queue_full=true;on_init();assert(render_text():find('OFFLINE',1,true))
        queue_full=false;press('a');on_update(0.3)
        respond(latest('/stations/search'),{station('bad','HE AAC','HE-AAC')});on_update(0.1)
        assert(render_text():find('No compatible streams',1,true))
        assert(render_text():find('HLS, Opus and HE-AAC',1,true))
        local M=require('station');local rows={M.normalize(station('w','West','MP3',0,-100)),M.normalize(station('e','East','MP3',0,100)),M.normalize(station('n','North','MP3',60,0))}
        assert(M.neighbor(rows,1,'dpad_right')==3)
        assert(M.neighbor(rows,3,'dpad_down')~=3)
        assert(sync_calls==0)
    "#).exec().unwrap();
}

#[test]
fn country_iso_queries_page_bounds_timeouts_and_late_results() {
    app().load(r#"
        populate({station('first')})
        press('dpad_left');local c=latest('/json/countries')
        respond(c,{{name='France',iso_3166_1='FR',stationcount=50}});on_update(0.1)
        press('dpad_down');press('a');on_update(0.3)
        local old=latest('/stations/search')
        assert(requests[old]:find('&countrycode=FR',1,true))
        on_update(15);local next_id=latest('/stations/search');assert(next_id~=old,'timeout must fail over')
        respond(old,{station('late','Late timed-out result')});on_update(0.1)
        assert(not render_text():find('Late timed-out result',1,true))
        local rows={};for i=1,150 do rows[i]=station('bounded'..i) end
        respond(next_id,rows);on_update(0.1)
        assert(render_text():find('100 candidates / 100 checked',1,true))
        assert(#json.decode(saved['frequency.cache.v1']).stations==100,'oversized replies must stay bounded')
        menu(7);on_update(0.3)
        assert(requests[latest('/stations/search')]:find('offset=100',1,true))
        assert(requests[latest('/stations/search')]:find('countrycode=FR',1,true))
        on_destroy();respond(latest('/stations/search'),{station('dead')});on_update(1)
        assert(#plays==0)
    "#).exec().unwrap();
}

#[test]
fn recent_reordering_keeps_playing_selection_and_metadata_is_honest() {
    app().load(r#"
        populate({station('first','First','MP3'),station('second','Second','MP3')})
        press('a');native.state='playing';on_update(0.1)
        press('dpad_down');press('a');native.state='playing';on_update(0.1)
        menu(4);press('dpad_down');press('a');native.state='playing';on_update(0.1)
        local count=#plays
        press('a');on_update(0.1)
        assert(#plays==count and pauses[#pauses]==true,'history reorder must preserve the selected playing station')
        menu(9);assert(render_text():find('Station notes',1,true))
        assert(render_text():find('First',1,true))
        assert(render_text():find('Not provided / available in country lists only',1,true))
        assert(render_text():find('No station logo is shown',1,true))
        assert(not render_text():find('latitude',1,true),'do not invent coordinates')
    "#).exec().unwrap();
}
