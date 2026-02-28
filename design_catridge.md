# Project Design Document

## The Problem

Cheap Linux handhelds (R36S, R36S Plus, Anbernic RG351 family, PowKiddy devices) are open Linux computers with WiFi, decent screens, d-pads, joysticks, and buttons — but the entire ecosystem treats them as single-purpose retro gaming devices. There is no app store, no SDK, and no community infrastructure for general-purpose apps. PortMaster proves the distribution model works, but is limited to game ports.

## The Vision

An open ecosystem that turns any Linux handheld into a general-purpose pocket device. A developer SDK, a curated app registry, and an on-device client that makes discovering, installing, and running apps as easy as browsing EmulationStation.

---

## 1. Naming

The name should: be short and memorable, evoke the handheld/pocket form factor, avoid being too "retro gaming" specific (this is about apps, not emulation), and work as both the platform name and the on-device client name.

### Candidates

| Name | Rationale | Pros | Cons |
|------|-----------|------|------|
| **Cartridge** | Apps are "cartridges" you slot in. Familiar metaphor for the community. | Instantly understood, strong brand, "load a cartridge" is natural language. SDK = Cartridge SDK, store = Cartridge Store | Slightly retro-gaming coded, which we're trying to move past |
| **Crank** | Short, mechanical, implies turning something on. Playdate uses a literal crank — this reclaims it for open devices. | Very short, great CLI name (`crank install hn-client`), verb-friendly ("crank it up") | May confuse with Playdate, slightly aggressive |
| **DPad** | Directly names what makes this unique: everything works with directional input. | Self-descriptive, memorable, clear differentiator | Too literal, might sound like an input library |
| **Pellet** | Small, compact, self-contained. Apps are pellets. | Unique, no conflicts, implies small/focused apps | Doesn't immediately communicate "handheld apps" |
| **Satchel** | A small bag you carry — your pocket full of apps. | Warm, approachable, "what's in your satchel?" | Maybe too soft/vague |

**Recommendation: Cartridge**

It bridges the retro gaming community (who already own these devices) with the new app ecosystem. "I installed a Cartridge for Hacker News" reads naturally. The metaphor extends well: apps are cartridges, the SDK builds cartridges, the store is where you browse cartridges, the on-device client loads cartridges.

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    DEVELOPER SIDE                        │
│                                                         │
│  ┌──────────────┐    ┌──────────────┐                   │
│  │ cartridge-sdk │    │  app-template │  ← clone & go   │
│  │  (framework)  │    │  (repo)       │                  │
│  └──────┬───────┘    └──────┬───────┘                   │
│         │                   │                            │
│         └───────┬───────────┘                            │
│                 ▼                                        │
│  ┌──────────────────────────┐                           │
│  │  Author's app repo       │                           │
│  │  tagged release v1.0     │                           │
│  └──────────┬───────────────┘                           │
│             │ PR to registry                             │
│             ▼                                            │
│  ┌──────────────────────────┐                           │
│  │  cartridge-registry      │                           │
│  │  (manifest + CI gates)   │──── CI builds from source │
│  └──────────┬───────────────┘     CI runs security scan │
│             │                     CI publishes artifact  │
│             ▼                     CI updates manifest    │
│  ┌──────────────────────────┐                           │
│  │  CDN / GitHub Releases   │                           │
│  │  (built artifacts)       │                           │
│  └──────────┬───────────────┘                           │
└─────────────┼───────────────────────────────────────────┘
              │
              │ HTTPS (manifest.json + .cart bundles)
              ▼
┌─────────────────────────────────────────────────────────┐
│                    DEVICE SIDE                           │
│                                                         │
│  ┌──────────────────────────┐                           │
│  │  cartridge-client        │                           │
│  │  (on-device store UI)    │                           │
│  │                          │                           │
│  │  Browse → Install → Run  │                           │
│  └──────────┬───────────────┘                           │
│             │                                            │
│             ▼                                            │
│  ┌──────────────────────────┐                           │
│  │  ~/.cartridges/          │                           │
│  │    hn-client/            │                           │
│  │    stock-ticker/         │                           │
│  │    reddit-browser/       │                           │
│  └──────────────────────────┘                           │
└─────────────────────────────────────────────────────────┘
```

### Repos

| Repo | Owner | Purpose |
|------|-------|---------|
| `cartridge-sdk` | You (core team) | Framework library, input abstraction, UI primitives, app lifecycle |
| `cartridge-template` | You (core team) | Cookiecutter/template repo developers clone to start a new app |
| `cartridge-registry` | You (core team) | The manifest of approved apps + CI pipelines for build/security |
| `cartridge-client` | You (core team) | The on-device app store and app launcher |
| `cartridge-*` | Individual authors | Each app in its own repo, author-maintained |

---

## 3. SDK Design (`cartridge-sdk`)

### Language Choice

**Python with SDL2** (via `pygame-ce` or `pysdl2`) for the first version.

Why:
- Lowest barrier to entry for contributors (and for vibecoding with LLMs)
- pygame-ce is actively maintained and cross-compiles well to ARM Linux
- The RK3326 can handle Python for UI apps (not gaming, but feed readers and dashboards, absolutely)
- Can always add Rust/C SDK bindings later for performance-critical apps

### Core Abstractions

```python
# cartridge_sdk/app.py

class CartridgeApp:
    """Base class every app extends."""

    def on_init(self, ctx: AppContext):
        """Called once at startup. Load config, set up state."""
        pass

    def on_input(self, event: InputEvent):
        """Called on every button press/release."""
        pass

    def on_update(self, dt: float):
        """Called every frame. Update state, fetch data."""
        pass

    def on_render(self, screen: Screen):
        """Called every frame. Draw to screen."""
        pass

    def on_suspend(self):
        """Called when user switches away from app."""
        pass

    def on_resume(self):
        """Called when user returns to app."""
        pass

    def on_destroy(self):
        """Called on exit. Save state, clean up."""
        pass
```

```python
# cartridge_sdk/input.py

class Button(Enum):
    DPAD_UP = "dpad_up"
    DPAD_DOWN = "dpad_down"
    DPAD_LEFT = "dpad_left"
    DPAD_RIGHT = "dpad_right"
    A = "a"            # Confirm / Open
    B = "b"            # Back / Cancel
    X = "x"            # Action 1
    Y = "y"            # Action 2
    L1 = "l1"          # Page up / Tab left
    R1 = "r1"          # Page down / Tab right
    L2 = "l2"          # Secondary action
    R2 = "r2"          # Secondary action
    START = "start"    # Menu / Settings
    SELECT = "select"  # Alt menu / Toggle
    LSTICK = "lstick"  # Left analog (as axis)
    RSTICK = "rstick"  # Right analog (as axis)

class InputEvent:
    button: Button
    action: Literal["press", "release", "hold", "repeat"]
    # For analog sticks:
    axis_x: float  # -1.0 to 1.0
    axis_y: float  # -1.0 to 1.0
```

```python
# cartridge_sdk/ui.py — built-in widget library

class ListView:
    """Scrollable list with highlight. D-pad up/down to navigate, A to select."""
    items: list[ListItem]
    on_select: Callable

class DetailView:
    """Scrollable text view with word wrap. B to go back."""
    title: str
    body: str

class TabBar:
    """Horizontal tabs. L1/R1 to switch."""
    tabs: list[Tab]
    active: int

class Table:
    """Columnar data display. Good for stock tickers, leaderboards."""
    columns: list[Column]
    rows: list[Row]

class TextInput:
    """On-screen keyboard navigated by d-pad. For search, login, etc."""
    # This is the hard one — but solvable with a grid keyboard layout
    # like game consoles have used forever (PS4 style)

class Toast:
    """Temporary notification overlay."""

class StatusBar:
    """Top bar showing time, WiFi, battery (provided by the client/OS)."""
```

### Screen Abstraction

```python
class Screen:
    width: int = 640    # R36S Plus
    height: int = 480
    
    def clear(self, color: Color = BLACK): ...
    def draw_text(self, text, x, y, font_size, color): ...
    def draw_rect(self, rect, color, filled): ...
    def draw_image(self, image, x, y, w, h): ...
    def draw_widget(self, widget, rect): ...
    
    # Theme-aware rendering
    theme: Theme  # colors, fonts, spacing from user preferences
```

### Networking

```python
# cartridge_sdk/net.py

class HttpClient:
    """Simple async HTTP client. Only available if app declares network permission."""
    
    async def get(self, url, headers=None) -> Response: ...
    async def post(self, url, body, headers=None) -> Response: ...
    
    # Built-in caching
    async def get_cached(self, url, ttl_seconds=300) -> Response: ...

# No raw sockets. No listening. No UDP. Just outbound HTTP(S).
# This is both a security boundary and a simplicity feature.
```

### Storage

```python
# cartridge_sdk/storage.py

class AppStorage:
    """Scoped to the app's own directory. Cannot access other apps or system files."""
    
    def save(self, key: str, data: dict): ...
    def load(self, key: str) -> dict | None: ...
    def delete(self, key: str): ...
    
    # For caching API responses, images, etc.
    cache_dir: Path  # auto-cleaned if device is low on space
    data_dir: Path   # persistent, user's data
```

---

## 4. App Manifest (`cartridge.toml`)

Every app has a `cartridge.toml` in its root:

```toml
[app]
id = "dev.stefano.hn-client"           # reverse domain, unique
name = "Hacker News"
description = "Browse Hacker News stories and comments"
version = "1.2.0"
author = "Stefano"
repo = "https://github.com/stefano/cartridge-hn"
license = "MIT"
icon = "assets/icon.png"               # 64x64 PNG
screenshots = ["assets/screen1.png", "assets/screen2.png"]

[app.entry]
main = "src/main.py"                   # entry point

[permissions]
network = true                         # can make outbound HTTP
storage = true                         # can persist data (always scoped)
# Future permissions:
# bluetooth = false
# audio = false
# camera = false  (if devices ever get one)

[compatibility]
min_sdk = "0.1.0"
screen_min_width = 320                 # works on smaller devices too
screen_min_height = 240
devices = ["rk3326", "rk3566", "h700"] # chip families, not specific models

[category]
primary = "news"                        # news, social, finance, tools, games, media, productivity
tags = ["hacker-news", "tech", "reader"]
```

---

## 5. App Bundle Format (`.cart`)

A `.cart` file is just a gzipped tar archive with a known structure:

```
hn-client-1.2.0.cart
├── cartridge.toml
├── src/
│   ├── main.py
│   ├── api.py
│   └── views.py
├── assets/
│   ├── icon.png
│   └── fonts/
└── CHECKSUMS.sha256
```

Built by CI, never by the author directly. The author pushes source code; the registry CI produces the `.cart`.

---

## 6. Registry Design (`cartridge-registry`)

### Structure

```
cartridge-registry/
├── registry.json               # auto-generated master manifest
├── apps/
│   ├── dev.stefano.hn-client/
│   │   └── entry.toml          # metadata for this app
│   ├── dev.someone.reddit/
│   │   └── entry.toml
│   └── ...
├── authors/
│   ├── stefano.toml            # author trust level, public key
│   └── someone.toml
├── policies/
│   ├── banned-syscalls.txt
│   ├── banned-imports.txt
│   └── review-checklist.md
└── .github/
    └── workflows/
        ├── validate-submission.yml
        ├── build-and-scan.yml
        └── publish-registry.yml
```

### `entry.toml` (per app in registry)

```toml
[source]
repo = "https://github.com/stefano/cartridge-hn"
branch = "main"
tag_pattern = "v*"                    # which tags trigger builds

[review]
status = "approved"                   # pending | approved | suspended
approved_by = "stefano"
approved_at = "2026-02-25T10:00:00Z"
trust_tier = "core"                   # core | trusted | community | new

[latest]
version = "1.2.0"
artifact_url = "https://cdn.example.com/carts/dev.stefano.hn-client-1.2.0.cart"
artifact_sha256 = "a1b2c3d4..."
build_log = "https://github.com/cartridge-registry/actions/runs/12345"
```

### `registry.json` (what the device fetches)

Auto-generated from all `entry.toml` files. This is the only file the on-device client needs to download to know what's available:

```json
{
  "version": 2,
  "updated_at": "2026-02-25T12:00:00Z",
  "apps": [
    {
      "id": "dev.stefano.hn-client",
      "name": "Hacker News",
      "description": "Browse Hacker News stories and comments",
      "version": "1.2.0",
      "author": "Stefano",
      "category": "news",
      "tags": ["hacker-news", "tech", "reader"],
      "icon_url": "https://cdn.example.com/icons/hn-client.png",
      "artifact_url": "https://cdn.example.com/carts/dev.stefano.hn-client-1.2.0.cart",
      "artifact_sha256": "a1b2c3d4...",
      "artifact_size_bytes": 245000,
      "permissions": ["network", "storage"],
      "compatibility": { "min_sdk": "0.1.0", "devices": ["rk3326", "rk3566", "h700"] },
      "trust_tier": "core"
    }
  ]
}
```

---

## 7. Security Model

### Layers of Defense

```
Layer 1: SOURCE-ONLY SUBMISSIONS
├── No one submits binaries. Ever.
├── CI clones the repo at the tagged commit
├── CI builds in a clean container
└── Eliminates: trojans, supply-chain binary injection

Layer 2: STATIC ANALYSIS (CI)
├── Scan Python AST for banned imports (os.system, subprocess, socket, ctypes)
├── Scan for filesystem access outside app sandbox
├── Verify all network calls go through cartridge_sdk.net (not raw urllib/requests)
├── Check that declared permissions match actual code behavior
└── Eliminates: obvious malware, privilege escalation attempts

Layer 3: PERMISSION ENFORCEMENT (Runtime)
├── App runs as restricted Linux user
├── Filesystem: only ~/.cartridges/<app-id>/ is writable
├── Network: iptables rules or seccomp to allow only outbound HTTPS
├── No shell access, no raw process spawning
└── Eliminates: sandbox escapes, lateral movement

Layer 4: TRUST TIERS (Human review)
├── "new" — requires manual review by a maintainer before first publish
├── "community" — after 2+ approved apps, auto-approved if CI passes
├── "trusted" — established contributors, can also review others
├── "core" — project maintainers
└── Eliminates: persistent bad actors, social engineering

Layer 5: TRANSPARENCY
├── Every build has a public build log
├── Every approval has a reviewer name and timestamp
├── Artifact hashes are in the public registry
├── Anyone can audit any app's source
└── Eliminates: silent tampering, backdoors
```

### Banned Python Imports (initial list)

```
os.system, os.exec*, os.spawn*, os.popen
subprocess.*
socket.* (raw sockets — the SDK provides HTTP)
ctypes.* (FFI — could call anything)
importlib (dynamic imports could bypass scanning)
eval, exec, compile (dynamic code execution)
shutil.rmtree (outside sandbox)
```

### What Authors CAN Do

- Outbound HTTPS via `cartridge_sdk.net.HttpClient`
- Read/write their own scoped storage
- Render to screen via SDL2 (through the SDK)
- Read input events
- Play audio (if permission declared) — future
- That's it. Intentionally constrained.

---

## 8. On-Device Client (`cartridge-client`)

The client itself is a CartridgeApp — it uses the same SDK. It's the "home screen" of the ecosystem.

### UX Flow

```
STORE (default view)
├── Featured         ← curated by maintainers
├── Categories       ← News, Social, Finance, Tools, Games, Media, Productivity
├── New              ← recently added
├── Updated          ← recently updated
└── Search           ← on-screen keyboard

D-pad: navigate          A: open app detail
L1/R1: switch category   B: back

APP DETAIL
├── Name, author, description
├── Screenshots (L/R to scroll)
├── Permissions list
├── Install / Update / Remove
└── Version, size, trust tier

INSTALLED (tab)
├── List of installed apps
├── A: launch
├── X: check for update
├── Y: remove
└── START: settings

SETTINGS
├── WiFi configuration
├── Storage usage
├── Check for client updates
├── About
└── Developer mode (SSH, logs)
```

### Integration with Existing CFW

The client should integrate with EmulationStation / the existing frontend as a new "system" entry (like how PortMaster appears under Tools). Users shouldn't need to replace their firmware — Cartridge sits alongside their existing retro gaming setup.

Installation: download a `.sh` installer (like PortMaster does), or eventually have CFW maintainers bundle it.

---

## 9. Developer Experience

### From Zero to Published App

```bash
# 1. Clone the template
git clone https://github.com/cartridge-project/cartridge-template my-app
cd my-app

# 2. Edit cartridge.toml (name, id, permissions)
# 3. Write your app in src/main.py

# 4. Test locally (desktop mode — SDL2 window simulating device screen)
cartridge run

# 5. Test on device (over WiFi)
cartridge deploy --device 192.168.1.42

# 6. Publish
git tag v1.0.0
git push origin v1.0.0
# Then open a PR to cartridge-registry adding your app entry
```

### The `cartridge` CLI tool (dev machine)

```
cartridge new <name>          # scaffold a new app
cartridge run                 # run locally in desktop simulation mode
cartridge run --resolution 640x480   # simulate specific device
cartridge deploy --device <ip>       # push to device over SSH
cartridge validate            # run the same checks CI will run
cartridge bundle              # produce a local .cart for testing
```

---

## 10. First Apps (Bootstrap the Ecosystem)

These should ship with or shortly after the initial release to demonstrate the platform's potential:

| App | Category | Complexity | Why |
|-----|----------|------------|-----|
| **Hacker News** | News | Medium | Perfect list→detail pattern, shows network + text rendering |
| **Stock Ticker** | Finance | Low | Table view, auto-refresh, shows real-time data |
| **Weather** | Tools | Low | Simple display, location-based, shows the "glanceable" use case |
| **Pomodoro Timer** | Productivity | Low | Minimal UI, shows the "always-on-desk" use case |
| **Reddit Reader** | Social | Medium | Similar to HN but with subreddit tabs, shows TabBar |
| **RSS Reader** | News | Medium | User-configurable feeds, shows storage/config |
| **System Monitor** | Tools | Low | CPU, memory, battery, WiFi — shows the device itself as content |

---

## 11. Roadmap

### Phase 1: Foundation (weeks 1-4)
- [ ] `cartridge-sdk` v0.1: App lifecycle, input, screen, ListView, DetailView, TabBar
- [ ] `cartridge-template`: clone-and-go starter
- [ ] `cartridge` CLI: `new`, `run`, `validate`
- [ ] Desktop simulation mode (test on PC without device)
- [ ] First app: Hacker News client

### Phase 2: Distribution (weeks 5-8)
- [ ] `cartridge-registry`: repo structure, CI pipeline, submission flow
- [ ] `cartridge-client` v0.1: on-device store, browse, install, launch
- [ ] Security: sandboxed execution, permission enforcement
- [ ] 3-4 more first-party apps
- [ ] `.cart` bundle format finalized

### Phase 3: Community (weeks 9-12)
- [ ] Public launch (Hacker News post, Reddit, retro handheld communities)
- [ ] Template repo documentation and tutorials
- [ ] Trust tier system operational
- [ ] First third-party app submissions

### Phase 4: Expansion
- [ ] Rust SDK bindings (for performance-critical apps)
- [ ] Multi-device support (test on Anbernic, PowKiddy, Miyoo)
- [ ] Theme engine (dark/light, custom colors)
- [ ] App-to-app communication (share data between apps)
- [ ] On-screen keyboard improvements
- [ ] Audio support
- [ ] Bluetooth support (for external input devices)

---

## 12. Open Questions

1. **Python distribution on device**: Should we bundle a Python runtime in each `.cart`, or assume a shared runtime installed by the client? Shared is smaller but creates version dependency headaches.

2. **Desktop simulation fidelity**: How closely should the desktop mode simulate device constraints (CPU speed, memory limits)? Or is "it renders correctly" enough for v1?

3. **Analog stick handling**: Should the SDK abstract analog sticks as d-pad input (with deadzone/threshold), or expose raw axis values and let apps decide?

4. **Versioning and updates**: Auto-update apps? Or always manual? PortMaster is manual, which is safer but less convenient.

5. **Monetization for authors**: Not needed for v1, but if the ecosystem grows — donations page in app metadata? No in-app purchases (too complex and against the spirit).

6. **GitHub dependency**: The entire CI/registry pipeline assumes GitHub. Is that fine, or should it be more platform-agnostic from the start?
