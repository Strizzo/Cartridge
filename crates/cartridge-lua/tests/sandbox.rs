use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use cartridge_core::theme::Theme;
use cartridge_lua::runner::LuaAppRunner;

struct AppFixture {
    root: PathBuf,
    app: PathBuf,
}

impl AppFixture {
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "cartridge-sandbox-test-{}-{nonce}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        let fixture = Self { root, app };
        fixture.write("main.lua", source);
        fixture
    }

    fn write(&self, relative: &str, source: &str) {
        self.write_bytes(relative, source.as_bytes());
    }

    fn write_bytes(&self, relative: &str, source: &[u8]) {
        let path = self.app.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }

    fn run(&self) -> Result<LuaAppRunner, String> {
        LuaAppRunner::new(
            &self.app,
            "main.lua",
            "dev.test.sandbox",
            &Theme::default(),
            &[],
        )
    }

    fn assert_runs(&self) {
        if let Err(error) = self.run() {
            panic!("fixture failed: {error}");
        }
    }
}

impl Drop for AppFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn standard_io_cannot_be_restored_from_package_loaded() {
    AppFixture::new(
        r#"
        assert(io == nil)
        local restored_io = package and package.loaded and package.loaded.io
        assert(restored_io == nil,
            'standard I/O remains reachable through package.loaded.io')
        "#,
    )
    .assert_runs();
}

#[test]
fn standard_package_loaders_and_dangerous_functions_are_unavailable() {
    AppFixture::new(
        r#"
        assert(package == nil, 'standard package loaders must not be exposed')
        assert(io == nil and debug == nil and loadfile == nil and dofile == nil)
        for _, name in ipairs({
            'execute', 'rename', 'remove', 'tmpname', 'exit', 'getenv', 'setlocale'
        }) do
            assert(os[name] == nil, 'unsafe os function: '..name)
        end
        assert(http == nil and storage == nil and ssh == nil and audio == nil)
        assert(type(os.time()) == 'number')
        assert(type(os.date('%Y')) == 'string')
        assert(math.floor(1.9) == 1 and string.upper('lua') == 'LUA')
        assert(utf8.len('hello') == 5)
        local co = coroutine.create(function() return 42 end)
        local ok, value = coroutine.resume(co)
        assert(ok and value == 42)
        "#,
    )
    .assert_runs();
}

#[test]
fn load_accepts_text_strings_and_preserves_optional_environments() {
    AppFixture::new(
        r#"
        text_load_value = 42
        assert(assert(load('return text_load_value'))() == 42)
        assert(assert(load('return text_load_value', 'text', 't'))() == 42)
        local env = setmetatable({}, {__index = {answer = 42}})
        assert(assert(load('return answer', 'custom-env', 'bt', env))() == 42)
        assert(assert(load('return _ENV', 'explicit-nil', 't', nil))() == nil)
        assert(assert(load('return _ENV', 'explicit-false', 't', false))() == false)
        local restricted = assert(load('return io, package, debug, loadfile, dofile'))
        local host_io, host_package, host_debug, file_loader, do_file = restricted()
        assert(host_io == nil and host_package == nil and host_debug == nil)
        assert(file_loader == nil and do_file == nil)
        local bad, err = load('local =', 'invalid-text', 't')
        assert(bad == nil and type(err) == 'string')
        local binary_only, mode_err = load('return 42', 'text', 'b')
        assert(binary_only == nil and type(mode_err) == 'string')
        "#,
    )
    .assert_runs();
}

#[test]
fn load_accepts_text_readers_and_preserves_optional_environments() {
    AppFixture::new(
        r#"
        local parts, index = {'return ', 'answer'}, 0
        local reader = function() index = index + 1; return parts[index] end
        local chunk = assert(load(reader, 'text-reader', 'bt', {answer = 42}))
        assert(chunk() == 42 and index == 3)
        local calls = 0
        local empty_terminated = function()
            calls = calls + 1
            return calls == 1 and 'return 7' or ''
        end
        assert(assert(load(empty_terminated))() == 7 and calls == 2)
        local read_once = false
        local nil_env_reader = function()
            if read_once then return nil end
            read_once = true
            return 'return _ENV'
        end
        assert(assert(load(nil_env_reader, 'nil-env-reader', 't', nil))() == nil)
        "#,
    )
    .assert_runs();
}

#[test]
fn load_rejects_self_generated_binary_strings_for_all_modes() {
    AppFixture::new(
        r#"
        local bytecode = string.dump(function() return 42 end)
        for _, mode in ipairs({false, 't', 'bt', 'b'}) do
            local chunk, err = load(bytecode, 'harmless-self-dump', mode or nil)
            assert(chunk == nil and type(err) == 'string',
                'binary string was accepted for mode '..tostring(mode))
        end
        "#,
    )
    .assert_runs();
}

#[test]
fn load_rejects_binary_readers_for_all_modes() {
    AppFixture::new(
        r#"
        local bytecode = string.dump(function() return 42 end)
        for _, mode in ipairs({false, 't', 'bt', 'b'}) do
            for _, step in ipairs({1, #bytecode}) do
                local index = 1
                local reader = function()
                    if index > #bytecode then return nil end
                    local piece = bytecode:sub(index, index + step - 1)
                    index = index + step
                    return piece
                end
                local chunk, err = load(reader, 'harmless-binary-reader', mode or nil)
                assert(chunk == nil and type(err) == 'string',
                    'binary reader was accepted for mode '..tostring(mode))
            end
        end
        "#,
    )
    .assert_runs();
}

fn harmless_bytecode() -> Vec<u8> {
    mlua::Lua::new()
        .load("return 42")
        .set_mode(mlua::ChunkMode::Text)
        .into_function()
        .unwrap()
        .dump(true)
}

#[test]
fn entry_file_rejects_bytecode() {
    let fixture = AppFixture::new("");
    fixture.write_bytes("main.lua", &harmless_bytecode());
    assert!(fixture.run().is_err(), "binary entry point was accepted");
}

#[test]
fn require_rejects_module_bytecode() {
    let fixture = AppFixture::new(
        r#"
        local ok = pcall(require, 'binary')
        assert(not ok, 'binary module was accepted')
        "#,
    );
    fixture.write_bytes("binary.lua", &harmless_bytecode());
    fixture.assert_runs();
}

#[test]
fn bundled_module_apps_load_without_standard_package_library() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lua_cartridges");
    // Load entry points and their real local dependencies, without invoking
    // lifecycle callbacks or granting storage, network, SSH, or device APIs.
    for app in ["frequency", "mission_control", "outside", "vibeboy"] {
        if let Err(error) = LuaAppRunner::new(
            &root.join(app),
            "main.lua",
            "dev.test.sandbox-bundled",
            &Theme::default(),
            &[],
        ) {
            panic!("{app} failed to load: {error}");
        }
    }
}

#[test]
fn app_local_require_caches_modules_and_nested_dependencies() {
    let fixture = AppFixture::new(
        r#"
        local detail = require('screens.detail')
        assert(detail == require('screens.detail'))
        assert(detail.model == require('model'))
        assert(model_loads == 1 and detail_loads == 1)
        assert(detail.model.answer == 42)
        "#,
    );
    fixture.write(
        "model.lua",
        "model_loads = (model_loads or 0) + 1; return {answer = 42}",
    );
    fixture.write(
        "screens/detail.lua",
        "detail_loads = (detail_loads or 0) + 1; return {model = require('model')}",
    );
    fixture.assert_runs();
    // A second runner gets its own module cache.
    fixture.assert_runs();
}

#[test]
fn require_preserves_nil_and_false_module_caching() {
    let fixture = AppFixture::new(
        r#"
        assert(require('nothing') == nil)
        assert(require('nothing') == true)
        assert(require('false_value') == false)
        assert(require('false_value') == false)
        assert(nothing_loads == 1 and false_loads == 1)
        "#,
    );
    fixture.write("nothing.lua", "nothing_loads = (nothing_loads or 0) + 1");
    fixture.write(
        "false_value.lua",
        "false_loads = (false_loads or 0) + 1; return false",
    );
    fixture.assert_runs();
}

#[test]
fn app_created_package_table_cannot_replace_require_cache() {
    let fixture = AppFixture::new(
        r#"
        package = {loaded = {model = {answer = -1}}}
        local model = require('model')
        assert(model.answer == 42)
        package.loaded.model = {answer = -2}
        package = nil
        assert(require('model') == model and model_loads == 1)
        "#,
    );
    fixture.write(
        "model.lua",
        "model_loads = (model_loads or 0) + 1; return {answer = 42}",
    );
    fixture.assert_runs();
}

#[test]
fn require_does_not_resolve_standard_libraries_or_outside_paths() {
    let fixture = AppFixture::new(
        r#"
        for _, name in ipairs({'io', 'package', 'debug', '../outside', 'missing'}) do
            local ok = pcall(require, name)
            assert(not ok, 'unexpected module: '..name)
        end
        assert(outside_loaded == nil)
        "#,
    );
    let outside = fixture.root.join("outside.lua");
    std::fs::write(&outside, "outside_loaded = true; return {}").unwrap();
    let absolute_module = outside.with_extension("");
    assert_outside_module_rejected(&fixture, &absolute_module);
}

fn assert_outside_module_rejected(fixture: &AppFixture, module: &Path) {
    let source = std::fs::read_to_string(fixture.app.join("main.lua")).unwrap();
    fixture.write(
        "main.lua",
        &format!(
            "{source}\nlocal ok, err = pcall(require, {:?})\n\
             assert(not ok and tostring(err):find('outside the app directory', 1, true))\n\
             assert(outside_loaded == nil)",
            module.to_str().unwrap()
        ),
    );
    fixture.assert_runs();
}

#[cfg(unix)]
#[test]
fn require_rejects_symlinks_outside_app_directory() {
    let fixture = AppFixture::new(
        r#"
        local ok, err = pcall(require, 'escape')
        assert(not ok and tostring(err):find('outside the app directory', 1, true))
        assert(outside_loaded == nil)
        "#,
    );
    let outside = fixture.root.join("outside.lua");
    std::fs::write(&outside, "outside_loaded = true; return {}").unwrap();
    std::os::unix::fs::symlink(outside, fixture.app.join("escape.lua")).unwrap();
    fixture.assert_runs();
}
