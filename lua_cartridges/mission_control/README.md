# Mission Control

A finished controller-only operations room for the current **VibeBoy** service on a 720×720 CartridgeOS handheld. A vermilion, ink and warm-paper graphic system surrounds real pane status, agent replies and terminal output. The app includes its own original geometric station icon.

Mission Control is an evolution of the VibeBoy cartridge: async state polling and command acknowledgements, stable pane selection, project/session grouping, a full output reader and complete saved-server setup.

## Connect a real server

On the computer hosting your tmux sessions, use the current VibeBoy checkout (the one containing `vibeboy/http_api.py`):

```sh
python3 -m venv .venv
.venv/bin/pip install -e .
.venv/bin/vibeboy --host 0.0.0.0 --http-port 8766
```

VibeBoy requires tmux and discovers its panes automatically. Start your agent or build inside tmux on that computer. The default daemon bind address is localhost; the explicit host option above makes it reachable from the handheld. Restrict this unauthenticated service to a trusted private network or VPN. For a daemon kept on localhost, establish a tunnel outside the app and enter its forwarded address; the app never starts blocking SSH work.

1. Open Mission Control and press **A** or **Y** to add a station.
2. Select **Name**, press **A**, and enter a friendly name using the controller keyboard.
3. Select **Address**, press **A**, and enter `computer.local:8766` or `192.168.1.10:8766`.
4. Select **Save**, press **A**, then press **A** on that station to connect.

Addresses accept HTTP/HTTPS, DNS names, IPv4, bare IPv6 (default port) and bracketed IPv6, with ports 1–65535. The default scheme is HTTP and the default port is 8766, including when no explicit port is supplied for HTTPS. No path, credentials or query string is accepted. The last connected station reconnects on launch. Up to eight stations and the polling interval are saved.

## Controls

**Select is reserved for the runtime.** Normal app screens never handle it; it exits to CartridgeOS. While the shared keyboard is visible, follow its on-screen hints (the runtime owns keyboard cancellation).

| Screen | Controls |
|---|---|
| Operations board | Up/Down: pane; Left/Right: All / Agents / Processes / Needs input; L1/R1: project group; A: inspect; X: refresh/reconnect; Start: servers; B: servers |
| Inspector | Up/Down: output history; L1/R1: adjacent pane; Left/Right or L2/R2: command; A: review command or compose; Y: full output; X: refresh; B: board; Start: servers |
| Full terminal | Up/Down: scroll 3 lines; Left/Right: pan 12 columns; A: latest; Y/B: inspector; X: refresh; Start: servers |
| Command review | Left: cancel (default); Right: send; A: confirm choice; B: cancel; Up/Down: page through long command text |
| Server stations | Up/Down: station; A: connect; X: edit; Y: add; L2: remove with confirmation; R2: disconnect; Start: cycle polling 1/2/5/10 seconds; B: board |
| Station editor | Up/Down: name/address/save; A: keyboard or save; B: discard changes |

Every command is reviewed with its target and exact payload. This includes suggestions, detected prompt choices, typed responses, Tab/accept ghost text (Claude panes), Escape, Ctrl+C and Ctrl+Z. Sending a typed response also sends Enter, as defined by VibeBoy. The shared runtime keyboard currently limits typed input to 64 characters and its available key set (no quote or bracket keys). For IPv6, enter a bare host such as `::1` to use port 8766; bracketed addresses with custom ports can also be imported through settings. Server-provided suggestions can be longer and retain their exact content. Suggestions and detected choices come from the service; the app does not generate them. Repeated A events cannot duplicate a command.

## What the data means

- **Projects** are groups of the API's `session_name` (tmux session names). The service omits working-directory/project-path metadata.
- **Agents** are panes explicitly reported as `claude_code`. **Processes** are the remaining pane types, including shells and builds. Build progress/result is visible only when the terminal itself reports it; there is no CI/build-result API.
- **In progress** counts `thinking` and `running`; **needs input** counts `waiting`. Status is supplied by the server, not inferred from text.
- Output is a recent screen snapshot, not an append-only terminal transcript. The app retains at most 128 panes, 240 lines per pane, 512 Unicode characters per line, and 24 suggestions plus 24 choices per pane. Long terminal lines can be panned; trailing blank tmux rows are hidden. There is no unbounded history/cache or stored terminal output.
- **LIVE** means a recent valid response; **STALE/OFFLINE** disables commands but keeps the last snapshot for reading. Failed polls retry after 2/4/8/16/30 seconds. X requests an immediate retry. Requests never overlap other state polls, including while changing stations.
- Only an exact acknowledgement matching action and session produces “Server acknowledged…”. VibeBoy currently acknowledges even when its executor returns false, and some actions can acknowledge with no executor. An acknowledgement is **not proof of execution**. Inspect the refreshed terminal. Transport failures report delivery as unknown; POST is never automatically retried.
- Disconnect stops scheduling requests and clears pane data. In-flight requests cannot be cancelled by the runtime; old responses are drained and ignored. A command already submitted can still reach its old server.

## Protocol and persistence

See [the protocol and testing guide](../../docs/mission-control.md). This app uses only async `GET /api/state` and `POST /api/action`. It requires `network` and `storage`, with no SSH permission. The service has no authentication, project/build management, log-stream endpoint, pagination, or terminal resize API.

## Run the checks

```sh
cargo test -p cartridge-lua --test mission_control
python3 lua_cartridges/mission_control/tests/render.py
```

The capture helper uses the shared `app-check` runner and `sim/fixtures/mission-control.json`; it checks 720×720 PNGs in `/tmp/mission-control-captures`. It includes board, inspector, output, command review, acknowledged command, settings/editor/keyboard, first launch, disconnect, offline, empty and loading states. SDL needs a desktop display on macOS (or SDL dummy video on Linux).

Exercise the actual VibeBoy Python HTTP handlers and dispatcher, with recording executor and no live tmux changes:

```sh
VIBEBOY_SOURCE=/path/to/VibeBoy \
VIBEBOY_PYTHON=/path/to/VibeBoy/.venv/bin/python \
CARTRIDGE_HOME=/tmp/mission-control-service-home \
cargo test -p cartridge-lua --test mission_control current_vibeboy_http_service_integration -- --ignored --nocapture --test-threads=1
```

The opt-in service test imports the existing checkout read-only and disables Python bytecode writes. Its real model serialization must match the fixture. Real tmux delivery, RK3326 frame times, physical controls and LAN reliability still require a device/Linux test.
