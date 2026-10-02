use cartridge_lua::api::{new_app_control, register_http_api};
use mlua::{Lua, Table};
use std::time::{Duration, Instant};

#[test]
fn async_queue_is_bounded_and_poll_releases_capacity() {
    let lua = Lua::new();
    let control = new_app_control();
    register_http_api(&lua, "http-runtime-test", control.clone()).unwrap();
    // Invalid URLs complete without network access. Unpolled completions still
    // count against the limit, so fast failures cannot grow the response queue.
    lua.load(
        r#"
        for i=1,64 do assert(http.get_async('invalid://test') == i) end
        local ok,err=pcall(http.get_async,'invalid://overflow')
        assert(not ok and tostring(err):find('queue full',1,true))
    "#,
    )
    .exec()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let result: Table = lua.load("return http.poll()").eval().unwrap();
        if result.raw_len() > 0 {
            assert!(result.raw_len() <= 8);
            let first: Table = result.get(1).unwrap();
            assert!(!first.get::<bool>("ok").unwrap());
            assert!(first.get::<f64>("elapsed_ms").unwrap() >= 0.0);
            assert!(control.take_redraw());
            lua.load("assert(http.get_async('invalid://after-poll') == 65)")
                .exec()
                .unwrap();
            break;
        }
        assert!(Instant::now() < deadline, "HTTP workers did not complete");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Dropping Lua closes both queues, including workers waiting to deliver.
}
