use cartridge_lua::api::register_json_api;
use mlua::Lua;

fn check(name: &str, scenario: &str) {
    let lua = Lua::new();
    register_json_api(&lua).unwrap();
    lua.load(r#"
        theme={ui='neo'}
        app={request_redraw=function() end}
        storage={load=function() return nil end,save=function() end}
        audio={beep=function() end}
        screen={get_text_width=function(s) return (utf8.len(s) or #s)*8 end,
            get_display_text_width=function(s) return (utf8.len(s) or #s)*8 end}
    "#).exec().unwrap();
    let helpers: mlua::Table=lua.load(include_str!("../src/app_ui.lua")).eval().unwrap();
    lua.globals().set("ui",helpers).unwrap();
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lua_cartridges");
    let source=std::fs::read_to_string(root.join(name).join("main.lua")).unwrap();
    lua.load(&(source+scenario)).set_name(name).exec().unwrap();
}

#[test]
fn calculator_controller_history_and_safe_evaluation() {
    check("calculator",include_str!("fixtures/utility/calculator.lua"));
}
#[test]
fn todo_keyboard_completion_reopen_and_persistence() {
    check("todo",include_str!("fixtures/utility/todo.lua"));
}
#[test]
fn pomodoro_pause_skip_round_completion_and_idle_redraw() {
    check("pomodoro",include_str!("fixtures/utility/pomodoro.lua"));
}
