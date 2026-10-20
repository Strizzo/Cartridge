use mlua::Lua;

fn ui() -> Lua {
    let lua = Lua::new();
    lua.load(r#"
        theme={ui='neo'}
        draws={}; measures=0
        screen={
            draw_rect=function(x,y,w,h,opts) draws[#draws+1]={kind='rect',x=x,y=y,w=w,h=h} end,
            draw_text=function(s,x,y,opts) draws[#draws+1]={kind='text',value=s,x=x,y=y,size=opts.size,w=opts.max_width} end,
            draw_display_text=function(s,x,y,size,color) draws[#draws+1]={kind='display',value=s,x=x,y=y,size=size} end,
            get_text_width=function(s) measures=measures+1; return utf8.len(s)*8 end,
            get_display_text_width=function(s) measures=measures+1; return utf8.len(s)*8 end,
            clear=function() end,
            draw_image=function() error('missing icon fixture') end,
            draw_sparkline=function() end,
        }
    "#).exec().unwrap();
    let helpers: mlua::Table = lua.load(include_str!("../src/app_ui.lua")).eval().unwrap();
    lua.globals().set("ui",helpers).unwrap();
    lua
}

#[test]
fn footer_bounds_survive_long_labels_without_tiny_fonts() {
    ui().load(r#"
        ui.public.footer({{'DIRECTION PAD','Navigate every single row'},
            {'START / SELECT','Open or close the settings menu'},
            {'A','Confirm the selected action'}, {'B','Return to the preceding page'},
            {'L1 / R1','Choose a different category'}})
        for _,d in ipairs(draws) do
            if d.kind=='rect' then assert(d.x>=0 and d.x+d.w<=720.01) end
            if d.kind=='text' then
                assert(d.size>=12,'controller hints became too small')
                assert(d.x>=0 and d.x+d.w<=720.01,'footer hint escaped the screen')
            end
        end
    "#).exec().unwrap();
}

#[test]
fn display_fits_utf8_and_header_survives_a_missing_icon() {
    ui().load(r#"
        local U=ui.public
        U.display('αβγδεζηθ Unicode heading',24.5,120.5,42,U.C.paper,64)
        local d=draws[#draws]
        assert(d.kind=='display' and utf8.len(d.value) and utf8.len(d.value)*8<=64)
        assert(d.x==24 and d.y==120,'display positions must be integer SDL coordinates')
        U.header('VERY LONG APP TITLE / MANY WORDS','CARTRIDGE / UTILITY','OFFLINE')
        local saw_icon=false
        for _,d in ipairs(draws) do
            if d.kind=='rect' and d.x==24 and d.y==36 and d.w==44 then saw_icon=true end
        end
        assert(saw_icon,'missing icon must use the geometric fallback')
        local s=U.clean('a'..string.char(0)..'b'..string.char(255))
        assert(utf8.len(s) and s:find('?',1,true),'invalid provider bytes must not crash text drawing')
    "#).exec().unwrap();
}

#[test]
fn wrapped_reading_layout_preserves_utf8_and_reuses_measured_lines() {
    ui().load(r#"
        local U=ui.public
        local input='αβγδεζηθ café\nSecond paragraph with a longerword'
        local lines=U.wrap(input,48,16,64)
        for _,line in ipairs(lines) do assert(utf8.len(line)*8<=48) end
        local joined=table.concat(lines,''):gsub('%s','')
        assert(joined==input:gsub('%s',''),'wrapped text dropped article characters')
        local before=measures
        assert(U.wrap(input,48,16,64)==lines and measures==before,'unchanged layout was measured again')
        assert(#U.wrap(input,48,16,2)==2,'explicit reading bound was ignored')
    "#).exec().unwrap();
}
