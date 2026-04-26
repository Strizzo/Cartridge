# CartridgeOS Lua API Reference

This is the complete reference for the Lua API exposed to cartridges
running on CartridgeOS. Each cartridge is a Lua script that defines a few
lifecycle callbacks and uses the global tables documented here.

## Table of contents

- [Cartridge structure](#cartridge-structure)
- [Lifecycle callbacks](#lifecycle-callbacks)
- [Input events](#input-events)
- [`screen.*` — drawing](#screen)
- [`theme.*` — colors and dimensions](#theme)
- [`storage.*` — persistence](#storage)
- [`http.*` — network](#http)
- [`json.*` — JSON encode/decode](#json)
- [`text_input.*` — on-screen keyboard](#text_input)
- [`audio.*` — sound playback](#audio)
- [`system.*` — device info](#system)
- [`ssh.*` — SSH tunnel (advanced)](#ssh)
- [Sandboxing](#sandboxing)

---

## Cartridge structure

A cartridge is a directory with at least:

```
my-cartridge/
├── cartridge.json     # manifest
├── main.lua           # entry script (or whatever entry points to)
└── ...                # any other Lua modules and assets
```

`cartridge.json` example:

```json
{
  "id": "dev.example.my-cartridge",
  "name": "My Cartridge",
  "version": "0.1.0",
  "author": "Your Name",
  "description": "A short description",
  "category": "tools",
  "tags": ["util", "demo"],
  "permissions": ["network", "storage"],
  "entry": "main.lua"
}
```

Permissions currently inform the user but are **not enforced** by the
runtime. All APIs are always available. (Enforcement is on the roadmap.)

## Lifecycle callbacks

Define any subset. Missing callbacks are no-ops.

```lua
function on_init()
  -- Called once at startup. Initialize state.
end

function on_update(dt)
  -- Called every frame with delta time in seconds.
  -- Use for timers, animations, deferred actions.
end

function on_input(button, action)
  -- Called for each input event.
  -- button: "a", "b", "x", "y", "l1", "l2", "r1", "r2",
  --         "dpad_up", "dpad_down", "dpad_left", "dpad_right",
  --         "start", "select"
  -- action: "press", "release", "repeat"
end

function on_render()
  -- Called every frame after on_update.
  -- ONLY place where screen.* methods may be called.
  -- Always start with screen.clear() if you want a known background.
end

function on_destroy()
  -- Called when the cartridge is exiting (Select pressed, etc).
  -- Clean up resources (e.g. ssh.close()).
end
```

The cartridge runs at 30fps. Note that `Select` exits the cartridge and is
filtered out before `on_input`, so apps cannot intercept it.

## Input events

The available buttons match a typical handheld layout:

| Lua name         | Physical button (R36S Plus)                |
| ---------------- | ------------------------------------------ |
| `a`              | A (right face button, primary action)      |
| `b`              | B (bottom face button, back/cancel)        |
| `x`              | X (top face button)                        |
| `y`              | Y (left face button)                       |
| `l1` / `r1`      | Top shoulder buttons                       |
| `l2` / `r2`      | Bottom shoulder triggers                   |
| `dpad_up/down/...` | Directional pad                          |
| `start`          | Start                                      |
| `select`         | Select — **filtered out**, exits cartridge |

Action states:
- `"press"` — button just pressed
- `"release"` — button released
- `"repeat"` — held long enough to trigger key repeat (every ~80ms after a 400ms initial delay)

## `screen.*`

Drawing API. All methods are only valid inside `on_render`.

Screen dimensions are exposed as globals: `SCREEN_WIDTH = 720`, `SCREEN_HEIGHT = 720`. Also `screen.width` and `screen.height`.

### `screen.clear(r, g, b)`

Clear the entire screen with an RGB color (0..255 each).

### `screen.draw_text(text, x, y, opts?)`

Draw text. Returns the rendered width.

```lua
screen.draw_text("Hello", 10, 20, {
  color = {255, 100, 50},   -- RGB table; default = theme.text
  size = 16,                -- font size in pixels; default = 16
  bold = true,              -- default = false
  max_width = 200,          -- truncate with ".." if exceeds; default = nil
})
```

### `screen.draw_rect(x, y, w, h, opts?)`

Filled or outlined rectangle.

```lua
screen.draw_rect(10, 10, 200, 100, {
  color = {100, 200, 255},  -- default = theme.border
  filled = true,            -- default = true
  radius = 8,               -- corner radius; default = 0
})
```

### `screen.draw_line(x1, y1, x2, y2, opts?)`

```lua
screen.draw_line(0, 0, 720, 720, {color = theme.accent, width = 2})
```

### `screen.draw_circle(cx, cy, radius, r, g, b)`

Filled circle. RGB args are individual numbers.

### `screen.draw_card(x, y, w, h, opts?)`

Stylized rounded card with optional background, border, and shadow. The
preferred container for content blocks.

```lua
screen.draw_card(10, 10, 300, 200, {
  bg = theme.card_bg,       -- default = theme.card_bg
  border = theme.card_border,
  radius = 8,               -- default = 8
  shadow = true,            -- default = true
})
```

### `screen.draw_rounded_rect(x, y, w, h, r, g, b, radius, shadow)`

Lower-level rounded rectangle.

### `screen.draw_gradient_rect(x, y, w, h, r1, g1, b1, r2, g2, b2)`

Vertical color gradient.

### `screen.draw_pill(text, x, y, bg_r, bg_g, bg_b, opts?)`

Pill/badge with background color and centered text. Returns total pill width.

```lua
local w = screen.draw_pill("ACTIVE", 100, 50, 80, 200, 100, {
  text_color = {20, 20, 30},  -- default = dark
  size = 11,                  -- default = 11
})
```

### `screen.draw_button_hint(label, action, x, y, opts?)`

Draw a footer-style button hint like `[A] Open`. Returns total width.

```lua
screen.draw_button_hint("A", "Open", 12, 690, {
  color = theme.btn_a,
  size = 12,
})
```

### `screen.draw_progress_bar(x, y, w, h, progress, opts?)`

`progress` is 0.0..1.0.

```lua
screen.draw_progress_bar(10, 100, 500, 8, 0.65, {
  fill_color = {80, 200, 100},
  bg_color = {40, 40, 60},
  radius = 4,
})
```

### `screen.draw_sparkline(data, x, y, w, h, opts?)`

`data` is a sequence of numbers (Lua array). Auto-scales to min/max.

```lua
screen.draw_sparkline({10, 25, 35, 28, 40, 22}, 50, 100, 200, 80, {
  color = theme.accent,
  baseline_color = theme.text_dim,  -- optional center line
})
```

### `screen.draw_image(path, x, y, opts?)`

Draw a PNG/JPG from the cartridge's directory. Path is sandboxed — must
be inside the cartridge.

```lua
screen.draw_image("assets/icon.png", 50, 50, {
  w = 64, h = 64,           -- destination size; nil = use native
  src_x = 0, src_y = 0,     -- source rect for sprite sheets
  src_w = 32, src_h = 32,
})
```

### `screen.get_text_width(text, size, bold)` → `number`

Pixel width for layout calculations.

### `screen.get_line_height(size, bold)` → `number`

Line height for the given font size.

## `theme.*`

A read-only table of colors and dimensions tracking the system theme.
Use these so your cartridge looks consistent.

Colors (each is an `{r, g, b}` table):

| Name               | Use                        |
| ------------------ | -------------------------- |
| `bg`, `bg_lighter`, `bg_selected`, `bg_header` | Backgrounds |
| `card_bg`, `card_border`, `card_highlight`     | Card containers |
| `text`, `text_dim`, `text_accent`              | Text colors |
| `text_error`, `text_success`, `text_warning`   | Status text |
| `accent`, `border`                             | Highlights and lines |
| `btn_a`, `btn_b`, `btn_x`, `btn_y`, `btn_l`, `btn_r` | Button hint colors |
| `positive`, `negative`, `orange`               | Status colors |

Numeric fields:

`shadow_offset`, `border_radius`, `border_radius_small`, `padding`,
`item_height`, `header_height`, `footer_height`,
`font_size_normal`, `font_size_small`, `font_size_large`, `font_size_title`.

## `storage.*`

Per-cartridge key-value persistence. Values are JSON-encoded.

```lua
storage.save("settings", {volume = 0.8, last_seen = "2026-04-26"})
local data = storage.load("settings")  -- returns table or nil

storage.delete("old_key")
local keys = storage.list_keys()        -- {"settings", ...}
```

## `http.*`

HTTP client. Two flavors: synchronous (blocks the render thread) and
asynchronous (non-blocking with polling).

### Synchronous (simple)

```lua
local resp = http.get("https://api.example.com/data")
-- resp = {ok = true, status = 200, body = "..."}

local resp = http.get_cached("https://...", 60)  -- TTL in seconds
local resp = http.post("https://...", '{"key":"value"}')
```

Use these for one-off calls. They block, so don't poll an endpoint
once per second this way — use the async API.

### Asynchronous (recommended for polling)

```lua
-- Kick off a request; returns an integer request id immediately.
local id = http.get_async("https://...")
local id = http.post_async("https://...", body)

-- Drain completed responses. Returns an array of:
--   {id = N, ok = true, status = 200, body = "..."}
function on_update(dt)
  local responses = http.poll()
  for _, resp in ipairs(responses) do
    if resp.id == my_request_id then
      handle_response(resp)
    end
  end
end
```

The async API runs requests on a background thread, so `on_render`
keeps running smoothly while a request is in flight.

## `json.*`

```lua
local t = json.decode('{"name":"X","tags":["a","b"]}')
local s = json.encode({name = "X", tags = {"a", "b"}})
```

## `text_input.*`

Show an on-screen keyboard for text entry. While visible, all input is
routed to the keyboard widget (your `on_input` is not called).

```lua
function on_input(button, action)
  if button == "y" and action == "press" then
    text_input.show("Enter your name", "default value", false)
    -- args:  label, default text (optional), masked (optional bool)
  end
end

function on_update(dt)
  if text_input.is_active() then
    -- Drain results without blocking
    local r = text_input.poll()
    if type(r) == "string" then
      print("Got:", r)
    elseif r == false then
      print("Cancelled")
    end
    -- nil = still active, keep polling next frame
  end
end
```

For password fields, pass `masked = true` and characters render as `*`.

## `audio.*`

Pure-Rust audio playback. Supports WAV and OGG Vorbis.

```lua
audio.play("assets/alarm.wav")     -- plays from cartridge dir (sandboxed)
audio.beep(880, 200)               -- 880 Hz sine wave for 200ms
audio.beep(440, 400)
audio.set_volume(0.5)              -- 0..1
audio.stop()                       -- clear any queued sounds
```

Sounds queue up — multiple calls play in sequence.

## `system.*`

Read device info. Backed by a background poller; values update every
~2 seconds.

```lua
local cpu = system.cpu_percent()       -- 0..100
local mem = system.mem_percent()       -- 0..100
local mem_used = system.mem_used_mb()
local mem_total = system.mem_total_mb()
local disk_used = system.disk_used_gb()
local disk_total = system.disk_total_gb()
local battery = system.battery_percent() -- -1 if unknown
local charging = system.battery_charging() -- bool
local uptime = system.uptime_secs()
local host = system.hostname()
local ssid = system.wifi_ssid()        -- string or nil
local procs = system.process_count()
local rx = system.net_rx_rate()        -- KB/s
local tx = system.net_tx_rate()

-- History arrays (last ~30 samples each):
local cpu_history = system.cpu_history()
local mem_history = system.mem_history()
```

## `ssh.*`

SSH tunnel for cartridges that need to reach services on a remote
machine securely. Used by the VibeBoy cartridge.

```lua
local result = ssh.tunnel({
  host = "myserver.example.com",  -- or IP
  user = "alice",                 -- optional; leave nil for SSH config default
  remote_port = 8000,             -- the port to forward
  key_path = "/path/to/key",      -- optional
  key_dir = "/roms/Cartridge/ssh", -- optional dir to scan for keys
})
-- result = {ok = true, local_port = 12345}  -- or {ok = false, error = "..."}

ssh.is_alive()  -- bool
ssh.close()
```

Connect to `127.0.0.1:result.local_port` — traffic is forwarded through
SSH to the remote `host:remote_port`.

## Sandboxing

For safety, the following Lua features are removed:

- `os.execute`, `os.rename`, `os.remove`, `os.tmpname`, `os.exit`, `os.getenv`, `os.setlocale`
- The entire `io` library
- The `debug` library
- `loadfile` and `dofile` (a restricted `require` is provided instead)

The custom `require` only loads `.lua` files from inside your cartridge
directory. `require("ui")` resolves to `ui.lua`, `require("screens.detail")`
to `screens/detail.lua`. Path traversal (`../`) is blocked.

`screen.draw_image` and `audio.play` paths are also sandboxed: they must
canonically resolve inside the cartridge directory.

## Lifecycle exit conditions

A cartridge exits when:

1. The user presses `Select` (filtered before reaching `on_input`)
2. `on_init`, `on_input`, `on_update`, `on_render`, or `on_destroy` raises an unrecoverable Lua error (an error screen is shown until exit)

There is no programmatic "quit" yet — apps run until the user dismisses them.
