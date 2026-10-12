use cartridge_lua::api::register_json_api;
use mlua::Lua;

// Execute the real cartridge and shared UI; HTTP/storage/drawing are observable mocks.
fn stock_app() -> Lua {
    let lua = Lua::new();
    register_json_api(&lua).unwrap();
    lua.load(
        r#"
        theme = {ui='neo'}
        drawn, sparks, stored = {}, {}, {}
        requests, responses, redraws, sync_calls = {}, {}, 0, 0
        app = {request_redraw=function() redraws=redraws+1 end}
        storage = {
            load=function(key) return stored[key] end,
            save=function(key, value) stored[key]=value end,
        }
        screen = setmetatable({
            draw_text=function(value) drawn[#drawn+1]=value end,
            draw_display_text=function(value) drawn[#drawn+1]=value end,
            get_text_width=function(value) return utf8.len(value)*7 end,
            get_display_text_width=function(value) return utf8.len(value)*7 end,
            draw_sparkline=function(data, x, y, w, h)
                local copy={}
                for i,value in ipairs(data) do copy[i]=value end
                sparks[#sparks+1]=copy
            end,
        }, {__index=function() return function() end end})
        http = {
            get_async=function(url) requests[#requests+1]=url; return #requests end,
            poll=function() local completed=responses; responses={}; return completed end,
        }
        for _,name in ipairs({'get','get_cached','post'}) do
            http[name]=function() sync_calls=sync_calls+1; error('blocking HTTP used') end
        end
        function respond(id, price, closes)
            local ok=price~=nil
            responses[#responses+1]={id=id, ok=ok, status=ok and 200 or 503,
                body=ok and json.encode({chart={result={{
                    meta={regularMarketPrice=price, chartPreviousClose=price-1, currency='USD'},
                    indicators={quote={{close=closes or {price-1,price}}}},
                }}}}) or 'offline', elapsed_ms=25}
        end
        function press(key) on_input(key,'press') end
        function render_text()
            drawn, sparks={},{}
            on_render()
            return table.concat(drawn,'\n')
        end
        function assert_history(expected)
            assert(#sparks==1, 'expected exactly one rendered price history')
            assert(#sparks[1]==#expected, 'history sample count changed')
            for i,value in ipairs(expected) do
                assert(sparks[1][i]==value, 'quote snapshot rendered another snapshot history')
            end
        end
        function assert_idle()
            local before=redraws
            assert(on_update(0.1)==false, 'unchanged state reported dirty')
            assert(redraws==before, 'unchanged state requested redraw')
            assert(sync_calls==0, 'stock request used blocking HTTP')
        end
        "#,
    )
    .exec()
    .unwrap();
    let ui: mlua::Table = lua.load(include_str!("../src/app_ui.lua")).eval().unwrap();
    lua.globals().set("ui", ui).unwrap();
    let main = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../lua_cartridges/stock_market/main.lua");
    lua.load(std::fs::read_to_string(main).unwrap())
        .exec()
        .unwrap();
    lua
}

#[test]
fn membership_change_rejects_the_old_batch_before_returning_from_browse() {
    stock_app()
        .load(
            r#"
        on_init(); assert(#requests==7)
        press('x'); assert(#requests==7, 'unchanged pending batch was duplicated')
        press('y'); press('x') -- remove AAPL while the initial batch is pending
        assert(#stored.watchlist.items==6 and stored.watchlist.items[1].symbol=='GOOGL')
        for id=1,7 do respond(id, 300+id) end
        assert(on_update(0.1)==false, 'old membership response must be ignored before B')
        press('b')
        assert(#requests==13, 'membership revision must replace the pending seven-symbol batch')
        assert(not render_text():find('AAPL',1,true), 'removed symbol reappeared while refreshing')
        press('x'); assert(#requests==13, 'revised pending batch was duplicated')
        for id=8,13 do respond(id, 100+id) end
        assert(on_update(0.1)==true)
        local text=render_text()
        assert(text:find('GOOGL',1,true) and not text:find('AAPL',1,true))
        assert(text:find('1 / 6',1,true), 'revised watchlist count is incorrect')
        assert_idle()
        "#,
        )
        .exec()
        .unwrap();
}

#[test]
fn removing_a_symbol_supersedes_pending_requests_and_late_results_cannot_restore_it() {
    stock_app()
        .load(
            r#"
        on_init(); assert(#requests==7)
        press('y'); press('x'); press('b')
        assert(#requests==13, 'return must request the revised six-symbol membership')
        for id=8,13 do respond(id, 100+id) end
        assert(on_update(0.1)==true)
        local before=render_text()
        assert(before:find('GOOGL',1,true) and not before:find('AAPL',1,true))
        for id=1,7 do respond(id, 300+id) end
        local redraw_before=redraws
        assert(on_update(0.1)==false, 'late replaced batch reported a visible change')
        assert(redraws==redraw_before and render_text()==before, 'late batch replaced current quotes')
        assert(#stored.watchlist.items==6, 'removed membership was not persisted')
        assert_idle()
        "#,
        )
        .exec()
        .unwrap();
}

#[test]
fn adding_a_symbol_during_pending_load_survives_a_late_initial_batch() {
    stock_app()
        .load(
            r#"
        on_init(); press('y')
        for i=1,7 do press('dpad_down') end -- AVGO, not in the default watchlist
        press('a'); press('b')
        assert(#stored.watchlist.items==8 and stored.watchlist.items[8].symbol=='AVGO')
        assert(#requests==15, 'new membership did not start an eight-symbol batch')
        press('x'); assert(#requests==15, 'pending membership refresh was duplicated')
        for id=8,15 do respond(id, 100+id) end
        assert(on_update(0.1)==true)
        for i=1,7 do press('dpad_down') end
        local before=render_text()
        assert(before:find('AVGO',1,true) and before:find('8 / 8',1,true))
        for id=1,7 do respond(id, 300+id) end
        assert(on_update(0.1)==false and render_text()==before, 'late batch dropped added symbol')
        assert_idle()
        "#,
        )
        .exec()
        .unwrap();
}

#[test]
fn cached_quote_keeps_its_history_across_tabs_ranges_and_failed_refreshes() {
    stock_app()
        .load(
            r#"
        stored.watchlist={items={{symbol='BTC-USD',name='Bitcoin'}}}
        on_init(); assert(#requests==1)
        assert(requests[1]:find('range=5d',1,true))
        respond(1,13,{11,12,13}); on_update(0.1)
        assert(render_text():find('1W / HISTORY',1,true)); assert_history({11,12,13})

        press('l1') -- Crypto starts a 1W batch
        assert(#requests==9)
        press('r2') -- supersede with Crypto 1M
        assert(#requests==17 and requests[10]:find('range=1mo',1,true))
        respond(10,103,{101,102,103})
        for id=11,17 do respond(id) end
        on_update(0.1)
        assert(render_text():find('1M / HISTORY',1,true)); assert_history({101,102,103})

        press('r1') -- Watchlist requests 1M; its cached snapshot is still 1W
        assert(#requests==18)
        respond(18); on_update(0.1)
        assert(render_text():find('1W / CACHED',1,true)); assert_history({11,12,13})
        press('a')
        assert(render_text():find('PRICE HISTORY / 1W',1,true)); assert_history({11,12,13})

        press('b'); press('x')
        assert(#requests==19)
        respond(19,203,{201,202,203}); on_update(0.1)
        assert(render_text():find('1M / HISTORY',1,true)); assert_history({201,202,203})
        press('l1') -- cached Crypto 1M must also keep its older same-range snapshot
        assert(#requests==19)
        render_text(); assert_history({101,102,103})
        for id=2,9 do respond(id,303,{301,302,303}) end
        assert(on_update(0.1)==false)
        render_text(); assert_history({101,102,103})
        assert_idle()
        "#,
        )
        .exec()
        .unwrap();
}

#[test]
fn removed_cached_symbol_stays_absent_while_the_revised_refresh_fails() {
    stock_app()
        .load(
            r#"
        stored.watchlist={items={{symbol='AAPL',name='Apple'},{symbol='MSFT',name='Microsoft'}}}
        on_init(); assert(#requests==2)
        respond(1,13,{11,12,13}); respond(2,23,{21,22,23}); on_update(0.1)
        assert(render_text():find('AAPL',1,true))
        press('x'); assert(#requests==4)
        press('y'); press('x'); press('b')
        assert(#requests==5 and requests[5]:find('/MSFT?',1,true))
        assert(#stored.watchlist.items==1 and stored.watchlist.items[1].symbol=='MSFT')
        local pending=render_text()
        assert(pending:find('MSFT',1,true) and not pending:find('AAPL',1,true),
            'removed cached symbol remained visible while refreshing')
        assert_history({21,22,23})
        respond(5); on_update(0.1)
        local failed=render_text()
        assert(failed:find('MSFT',1,true) and failed:find('1W / CACHED',1,true)
            and not failed:find('AAPL',1,true), 'failed refresh lost membership or retained quote')
        assert_history({21,22,23})
        respond(3,313,{311,312,313}); respond(4,323,{321,322,323})
        assert(on_update(0.1)==false and render_text()==failed)
        assert_history({21,22,23})
        assert_idle()
        "#,
        )
        .exec()
        .unwrap();
}
