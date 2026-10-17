# Mission Control: VibeBoy service integration

App: `lua_cartridges/mission_control`, ID `dev.cartridge.mission-control`. Entry point `main.lua`, pure protocol normalization in `model.lua`, fixed 720px renderer in `view.lua`. [Setup and controller reference](../lua_cartridges/mission_control/README.md).

## API contract

Validated against the current VibeBoy checkout's `vibeboy/http_api.py`, `websocket_server.py`, `models.py`, and `action_executor.py`.

`GET http://host:8766/api/state` returns an object whose `sessions` member is a map keyed by the **session ID**:

```json
{
  "sessions": {
    "cartridge:0.0": {
      "session_name": "cartridge",
      "pane_id": "cartridge:0.0",
      "pane_command": "claude",
      "session_type": "claude_code",
      "status": "waiting",
      "screen_content": ["Ready for your response"],
      "response_options": [{"text":"Run the tests.","category":"approve","shortcut":"A"}],
      "detected_choices": [{"label":"Approve","key_sequence":"1 Enter","is_default":true}],
      "permission_mode": "normal",
      "last_updated": 1790841600.0
    }
  }
}
```

The map key, not list position, is the command target. `pane_cwd`, `last_assistant_message`, transcript paths and executor success are not in this response. Mission Control groups by `session_name` and exposes process name and screen contents; it never synthesizes project directories or build outcomes.

Commands are JSON `POST /api/action`:

```json
{"action":"send_response","session_id":"cartridge:0.0","payload":{"text":"Run the tests."}}
```

| Action | Payload | Meaning |
|---|---|---|
| `send_response` | `{"text":"..."}` | Service sends text followed by Enter |
| `send_keys` | `{"keys":"1 Enter"}` | Exact detected choice's tmux key sequence |
| `accept_ghost` | `{}` | Tab; offered for Claude panes |
| `escape` | `{}` | Escape |
| `interrupt` | `{}` | Ctrl+C; disruptive |
| `suspend` | `{}` | Ctrl+Z; disruptive |

A successful handler response is HTTP 200 with `{"type":"ack","action":"...","session_id":"..."}`. Rejections use HTTP 400 with `{"type":"error","message":"..."}`. The app validates both acknowledgement identities and displays the server's error text. Invalid/absent responses have **unknown delivery** and are never automatically retried.

The current dispatcher does not propagate executor failure: it ignores the returned boolean, and `interrupt`, `suspend`, `escape`, and `accept_ghost` may acknowledge with no executor. UI success therefore says **acknowledged**, not completed. The next state poll is the operator's evidence of what happened.

## Request/state invariants

- Network work uses only `http.get_async`, `http.post_async`, and `http.poll`; none occurs in render. Drawing has no width/system/network measurements. Geometry is fixed for 720×720 and uses `draw_display_text` and shared `ui.card`.
- At most one outstanding state GET and one action POST; no new state poll while POST is outstanding. Existing GET and a newly authorized POST may overlap. Request handles survive disconnect/switch until drained, preventing queue flooding.
- Every response is checked against the selected connection generation. A mutation revision invalidates GETs started before a command, so stale state cannot roll back the post-command context.
- Selection tracks the map key through priority sorting. A vanished pane remains visibly closed in its inspector and cannot receive commands. Returning to the board chooses an available pane.
- Confirmations default to Cancel and freeze the target/payload. A changed suggestion or vanished target invalidates review. All actions require fresh state; stale/offline state cannot authorize commands. POST is single-flight and never auto-retried.
- GET failure uses exponential backoff capped at 30 seconds. X requests immediate retry without duplicating a pending request. Staleness threshold is `max(10, interval * 3)` seconds.
- Data retained: 128 panes, 240 output lines per pane, 512 Unicode characters per line, 24 suggestions and 24 detected choices per pane. Suggestions over 4096 bytes and key sequences over 1024 bytes are excluded instead of silently truncated. Terminal snapshots stay in memory and are cleared on disconnect.

## Saved settings / simulator seed

Storage key **`mission_control_settings`**, file under the app's `data/mission_control_settings.json`:

```json
{
  "version": 1,
  "servers": [{"name":"Local fixture","url":"http://127.0.0.1:8766"}],
  "selected": 1,
  "interval": 2
}
```

`selected` is one-based; up to eight stations; `interval` is 1, 2, 5 or 10 seconds. Launch automatically connects to the saved selected station. Invalid saved URLs are skipped. The shared keyboard supports 48-character names, 240-character server addresses and 1024-character custom responses. X cycles letters, capitals and symbols including quotes/brackets. Select cancels editing. Server suggestions may retain up to 4096 bytes. No credentials or terminal contents are persisted. Storage write failures are surfaced as a settings warning; writes replace the previous file only after the complete JSON has been written.

With that JSON saved as `/tmp/mission-control-settings.json`:

```sh
cargo run --bin app-check -- mission_control \
  --fixture sim/fixtures/mission-control.json \
  --seed mission_control_settings=/tmp/mission-control-settings.json \
  --press 10:a --capture 24 --frames 28 --out /tmp/mission-control-detail
```

The fixture includes current-model GET state plus POST replies with exact JSON `request_body` matchers. Only known fixture requests are acknowledged; an unmatched request fails offline. It has no live sockets.

## Verification

`crates/cartridge-lua/tests/mission_control.rs` exercises real Lua modules and runtime JSON conversion through mlua, with simulated async completion order. It covers setup, validation and persistence; non-overlapping polls; switching servers; retry/backoff; invalid/empty responses; stable selection; pane disappearance; stale confirmations; exact payloads and acknowledgements; no POST retries; terminal scrolling/panning; custom keyboard responses; bounded data and Unicode; and render calls without network, measurements or out-of-bounds rectangles.

The opt-in `current_vibeboy_http_service_integration` test starts the actual Python `HttpApi` handlers and `VibeBoyServer.dispatch_action` with real `SessionState` serialization and a recording executor on an ephemeral loopback port. VibeBoy source is read-only. It exercises real HTTP from the Rust async client through the Lua app. This verifies wire compatibility and dispatch, **not actual tmux key delivery**.

`python3 lua_cartridges/mission_control/tests/render.py` uses the shared `app-check` executable for native SDL screenshots of thirteen states in isolated storage, with PNG dimension checks. `renderer_screenshots` is an opt-in wrapper around that same helper; SDL runs on the subprocess's main thread for macOS compatibility. Shared runner/runtime/registry files are owned by the parent integration task.

Native screenshots are layout evidence, not RK3326 performance measurements. Real handheld performance/input and real tmux delivery need hardware/Linux verification.
