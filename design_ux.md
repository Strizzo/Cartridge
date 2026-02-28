# Cartridge OS -- UX Design Document

## Table of Contents

1. Inspiration and Visual Language
2. Home Screen / Launcher Design
3. Navigation Model
4. Typography and Color Palette
5. Widget / Component Library
6. App Windowing Model

---

## 1. Inspiration and Visual Language

### The Aesthetic: "Cassette Futurism meets UNIX"

Cartridge should feel like a piece of fictional hardware from a 1980s sci-fi film that was
somehow manufactured for real. Not a raw terminal. Not a toy. A purpose-built operating
system for a handheld computer -- the kind of thing a character in Alien or Blade Runner
might pull out of a cargo pocket.

The key reference points, ranked by relevance:

**Primary inspirations:**

- Cassette Futurism (Alien 1979, Blade Runner, Signalis) -- chunky hardware,
  amber/green phosphor displays, utilitarian layouts with subtle warmth. The Alien
  Romulus UI work by Territory Studio is the single best modern reference: low-res
  CRT feel, monospaced type, scan-line texture, but completely functional and
  legible. Cartridge should take the *spirit* of this without the literal CRT
  emulation (no scan-line overlays -- we need every pixel).

- OnionOS / MiniUI (Miyoo Mini) -- proof that small-screen handhelds can have
  clean, fast, navigable UIs. Onion's main contribution is the "recently played"
  quick-access pattern and the absolute focus on fast boot and instant resume.
  Cartridge should steal the *speed* but not the look (Onion is too toy-like for
  our audience).

- Steam Deck UI -- the best large-scale reference for gamepad-driven navigation
  on a general-purpose OS. Key lessons: every element has a clear focus state,
  L1/R1 for tabs is now an established convention, and the universal back button
  (B) must work on *every* screen.

**Secondary inspirations:**

- PICO-8 -- not the resolution (128x128 is too constrained), but the *attitude*:
  deliberate limitations breeding a distinctive aesthetic. PICO-8 proves that a
  fixed palette and monospaced type create instant visual cohesion across apps
  made by different developers. Cartridge should achieve the same.

- Hacker/cyberdeck culture -- the r/cyberdeck community builds real hardware that
  runs Linux with custom UIs. The aesthetic leans utilitarian: dark backgrounds,
  monospaced type, status readouts, minimal decoration. Cartridge is literally
  this -- a cyberdeck OS.

- Bento grid layouts -- the modern design trend of information density organized
  into a grid of cards at varying sizes. At 640x480, a 2-column or 3-column
  bento grid is viable and maps well to gamepad navigation.

**What Cartridge is NOT:**

- Not a retro game launcher skin (no pixelated console artwork, no scanlines)
- Not a raw terminal (no green-on-black, no blinking cursor, no fake hacker movie)
- Not a phone OS (no swipe gestures, no rounded iOS widgets)
- Not a children's UI (no large bubbly icons, no emoji-driven navigation)

### Visual Language Summary

| Attribute         | Direction                                              |
|-------------------|--------------------------------------------------------|
| Mood              | Competent, utilitarian, quietly beautiful               |
| Surface treatment | Flat with subtle depth (1-2px shadows, border accents)  |
| Corners           | Rounded (6-8px radius), never sharp, never pill-shaped  |
| Density           | Medium-high -- respect the 640x480 constraint           |
| Motion            | Fast, functional -- no decorative animation             |
| Ornament          | Minimal -- colored strips, dots, pills for status       |

---

## 2. Home Screen / Launcher Design

### Layout Architecture

The 640x480 screen is divided into three persistent zones, shared by all screens:

```
+--[ HEADER BAR ]-------------------------------------------+ y=0
| Cartridge            WiFi *   12:34                        | h=36
+------------------------------------------------------------+ y=36
|                                                            |
|                                                            |
|                     CONTENT AREA                           |
|                    (408px tall)                             |
|                                                            |
|                                                            |
|                                                            |
|                                                            |
+--[ FOOTER BAR ]-------------------------------------------+ y=444
| [A] Open   [B] Back   [Y] Store   [START] Settings        | h=36
+------------------------------------------------------------+ y=480
```

**Header bar** (36px): App/screen title (left), status indicators (right). Status
indicators from right to left: clock, battery percentage, WiFi signal dot + SSID.
The header uses a subtle gradient from bg_header down to bg to avoid a hard line.

**Content area** (408px): Changes per screen. This is where the launcher grid,
app store list, settings, or running app lives.

**Footer bar** (36px): Context-sensitive button hints. Always present. Shows which
face buttons do what on the current screen. Uses colored badge pills matching the
physical button colors on the device (A=green, B=red, X=blue, Y=yellow).

### Home Screen: The Dock

The home screen is NOT a traditional app grid. It is a **horizontal dock** of
installed apps with a detail pane, inspired by the PS Vita home screen and the
Apple TV top shelf.

```
+--[ Cartridge ]---------------------[ * WiFi  12:34 ]-------+
|                                                            |
|  +------+  +------+  +------+  +------+  +------+         |
|  | _/\_ |  | $$$  |  | **** |  |      |  | >_   |         |
|  | HN   |  | Stk  |  | Wthr |  | Pomo |  | Sys  |         |
|  +------+  +------+  +------+  +------+  +------+         |
|         ^-- currently focused (accent border)               |
|                                                            |
|  +------------------------------------------------------+  |
|  | Stock Market                                    v0.1  |  |
|  | Track stocks, indices, and your watchlist             |  |
|  |                                                       |  |
|  | Category: FINANCE        Author: Cartridge Team       |  |
|  | Permissions: network, storage                         |  |
|  +------------------------------------------------------+  |
|                                                            |
|  Recently opened:                                          |
|    Hacker News (2 min ago)   Stock Market (1 hr ago)       |
|                                                            |
+--[ A Open   B --   Y Store   X Uninstall   START Set ]-----+
```

#### Dock Details

- **Icon row** (y=50, h=100): Horizontally scrolling row of app icons. Each icon
  is an 80x80 card with a 48x48 icon image centered above a 12px label. The
  focused card has the accent border (2px), a subtle glow/shadow, and scales up
  slightly (not animated -- just drawn at 88x88 instead of 80x80).

- **Detail pane** (y=170, h=180): Shows the name, description, version, category,
  author, and permissions of the currently focused app. This changes instantly as
  the user scrolls left/right through the dock. No animation delay.

- **Recent strip** (y=370, h=60): The last 3-4 opened apps as small 40x40 icons
  with timestamps. D-pad down from the dock focuses this row. Pressing A launches.

- **Empty state**: If no apps are installed, the content area shows a centered
  message: "No cartridges installed. Press Y to browse the store."

#### Navigation on Home

| Input   | Action                                  |
|---------|-----------------------------------------|
| D-left  | Move focus left in dock                  |
| D-right | Move focus right in dock                 |
| D-down  | Move focus to recent strip               |
| D-up    | Move focus back to dock from recent      |
| A       | Launch focused app                       |
| B       | (no action on home -- already at root)   |
| Y       | Open Cartridge Store                     |
| X       | Uninstall focused app (confirm dialog)   |
| START   | Open Settings                            |
| SELECT  | Open boot selector (ES vs Cartridge)     |
| L1/R1   | Page through dock (jump 5 icons)         |

### The "Games" / EmulationStation Shortcut

This is handled through the **boot selector**, not as an app in the dock. Pressing
SELECT anywhere shows a minimal overlay:

```
+------------------------------------------------------------+
|                                                            |
|          +----------------------------------+              |
|          |       Switch Environment         |              |
|          |                                  |              |
|          |   > EmulationStation             |              |
|          |     Cartridge OS                 |              |
|          |                                  |              |
|          |   [A] Select    [B] Cancel       |              |
|          +----------------------------------+              |
|                                                            |
+------------------------------------------------------------+
```

This is a system-level overlay, not a screen transition. Selecting EmulationStation
writes a flag file and restarts the frontend process. This keeps the mental model
clean: Cartridge and ES are peers, not parent-child.

### Background / Wallpaper System

For v1, **no wallpaper**. The dark background IS the design. This is intentional:

- Wallpapers at 640x480 look muddy and hurt text legibility
- The dark flat background is the Cartridge identity
- It keeps rendering simple and fast on low-end SoCs

For a future version, consider a subtle pattern system (like repeating geometric
tiles at 10-15% opacity) rather than photographic wallpapers. Patterns scale to
any resolution and maintain legibility.

---

## 3. Navigation Model

### Screen Hierarchy

```
HOME (dock)
 |
 +--- [A] Launch App ----------> RUNNING APP (fullscreen)
 |                                   |
 |                                   +--- [HOME combo] -> HOME
 |
 +--- [Y] Store ---------------> STORE
 |                                   |
 |                                   +--- [A] App Detail -> APP DETAIL
 |                                   |                         |
 |                                   |                         +--- [A] Install
 |                                   |                         +--- [B] Back to Store
 |                                   |
 |                                   +--- [B] Back to HOME
 |
 +--- [START] Settings ---------> SETTINGS
 |                                   |
 |                                   +--- [B] Back to HOME
 |
 +--- [SELECT] Boot Selector ---> OVERLAY (ES vs Cartridge)
```

### Universal Navigation Rules

These rules apply to EVERY screen in Cartridge OS, including third-party apps.
The SDK enforces them at the runner level, not at the app level.

1. **B is always Back.** Every screen must handle B to return to the previous
   screen. If the user is at the root of an app, B suspends the app and returns
   to HOME. Apps cannot override this -- the runner intercepts it.

2. **START is always Settings/Menu.** On HOME, it opens system settings. Inside
   an app, it opens the app's own settings or menu (if the app defines one).
   If the app has no menu, START is a no-op.

3. **SELECT + START (held 1s) is always Home.** This is the system-level escape
   hatch. No matter what an app is doing, this combo suspends it and returns to
   HOME. The runner handles this, not the app.

4. **L1/R1 is always tabs/pages.** If the current screen has tabs, L1/R1 switch
   them. If the current screen has pages, L1/R1 paginate. Apps should not use
   L1/R1 for other purposes.

5. **D-pad is always spatial navigation.** Up/down scrolls lists or moves between
   rows. Left/right moves within a row or adjusts values. Apps should not
   reassign D-pad to non-navigational functions (exception: games).

### Flow Detail: Home -> Store -> Install

```
[HOME]
  User presses Y
    |
[STORE]  -- loads catalog from registry.json (cached, with pull-to-refresh via X)
  L1/R1 switches category tabs: All | News | Finance | Tools | ...
  D-up/D-down scrolls app list
  User highlights an app, presses A
    |
[APP DETAIL]
  Shows: name, description, screenshots, permissions, size, version, trust tier
  L/R scrolls screenshots (if any)
  User presses A on "Install" button
    |
  [INSTALL PROGRESS]  -- overlay on detail screen
    Download progress bar
    Extraction status
    "Installed!" confirmation
    Auto-return to detail screen (now showing "Launch" and "Remove" buttons)
  User presses A on "Launch"
    |
[RUNNING APP]  -- fullscreen, app takes over rendering
  User presses SELECT+START (held)
    |
[HOME]  -- app suspended, can be resumed
```

### Flow Detail: Boot Selector

```
[ANY SCREEN]
  User presses SELECT
    |
[BOOT SELECTOR OVERLAY]
  Two options: EmulationStation, Cartridge OS
  Currently active one is marked with a bullet
  D-up/D-down to choose, A to confirm, B to cancel
    |
  If user selects ES:
    Write /tmp/.cartridge_switch_to_es flag
    Kill cartridge-client process
    Init system launches ES instead
  If user selects Cartridge:
    Dismiss overlay (already in Cartridge)
```

---

## 4. Typography and Color Palette

### Typography

**Primary font: A good monospaced font.** Monospace is the heart of the cyberdeck
aesthetic. It evokes terminals, code, data readouts, and technical competence. It
also solves a practical problem: at 640x480, proportional fonts create uneven
text columns that look messy when multiple apps render text differently.

Recommended font stack (in order of preference, matching current implementation):

1. **Cascadia Mono** -- Microsoft's terminal font. Clean, highly legible at small
   sizes, excellent weight variants. Free and open source (SIL OFL). This should
   be bundled with the SDK so all devices render identically.

2. **JetBrains Mono** -- alternative if licensing changes. Similar characteristics.

3. **DejaVu Sans Mono** -- fallback, widely available on Linux.

4. **Liberation Mono** -- last resort fallback.

**Why not a pixel/bitmap font?** While PICO-8 and retro consoles use bitmap fonts,
640x480 is high enough resolution that TTF monospace fonts are more legible and
more flexible. Bitmap fonts would look authentic but hurt readability for
long-form text (news articles, comments, descriptions).

**Why not a proportional font for body text?** Consistency. Every app, every
screen, every widget uses the same monospace font. This is a deliberate constraint
that creates the Cartridge identity -- just like PICO-8's palette creates its
identity. The moment you add a proportional font, the UI loses its character.

### Font Size Scale

Sizes are calibrated for 640x480 at the typical viewing distance of a handheld
(~30cm / 12 inches from face):

| Token            | Size (px) | Use                                       |
|------------------|-----------|-------------------------------------------|
| `font_title`     | 24        | Screen titles only (rarely used)           |
| `font_large`     | 20        | Section headers, modal titles              |
| `font_normal`    | 16        | Primary body text, list items              |
| `font_small`     | 13        | Secondary text, metadata, timestamps       |
| `font_tiny`      | 11        | Badges, pills, footer hints, status bar    |

Maximum of 5 sizes. Apps should not create arbitrary sizes -- the SDK should
provide these as named constants.

### Color Palette

The palette is organized into three tiers: **surfaces**, **text**, and **signals**.

#### Surfaces (backgrounds, cards, panels)

| Token                 | RGB               | Hex       | Use                         |
|-----------------------|-------------------|-----------|-----------------------------|
| `bg`                  | (18, 18, 24)      | `#121218` | Main background              |
| `bg_lighter`          | (30, 30, 42)      | `#1E1E2A` | Elevated surfaces, inputs    |
| `bg_header`           | (24, 24, 36)      | `#181824` | Header/footer bars           |
| `card_bg`             | (28, 28, 40)      | `#1C1C28` | Card backgrounds             |
| `card_highlight`      | (45, 55, 85)      | `#2D3755` | Focused/selected card bg     |
| `shadow`              | (8, 8, 12)        | `#08080C` | Drop shadow color            |

The background is a very dark blue-black, not pure black. This is critical -- pure
`#000000` feels dead on screen. The slight blue undertone gives it depth and reads
as "powered on" rather than "off."

#### Text

| Token              | RGB               | Hex       | Use                          |
|--------------------|-------------------|-----------|------------------------------|
| `text`             | (220, 220, 230)   | `#DCDCE6` | Primary text                  |
| `text_dim`         | (120, 120, 140)   | `#78788C` | Secondary/metadata text       |
| `text_accent`      | (100, 180, 255)   | `#64B4FF` | Links, interactive text       |

Primary text is NOT pure white. `#DCDCE6` is a warm off-white that reduces eye
strain in dark environments (these devices are often used in bed or in dim rooms).
The contrast ratio against `#121218` is approximately 14:1, exceeding WCAG AAA.

#### Signals (status, feedback, category accents)

| Token              | RGB               | Hex       | Use                          |
|--------------------|-------------------|-----------|------------------------------|
| `accent`           | (100, 180, 255)   | `#64B4FF` | Focus rings, active tabs, links |
| `positive`         | (80, 210, 120)    | `#50D278` | Success, installed, online   |
| `negative`         | (240, 80, 90)     | `#F0505A` | Error, remove, offline       |
| `warning`          | (255, 200, 60)    | `#FFC83C` | Caution, update available    |

The accent color is a medium-saturation blue. Not electric neon (too aggressive for
prolonged use), not pastel (too soft, doesn't pop on dark bg). It is the same blue
used in the existing theme.py and should remain the "brand color" of Cartridge.

#### Category Colors

Each app category has a unique accent color, used for pills, strips, and tab
highlights. These provide visual wayfinding without adding noise:

| Category     | RGB               | Hex       |
|--------------|-------------------|-----------|
| News         | (74, 158, 255)    | `#4A9EFF` |
| Finance      | (74, 222, 128)    | `#4ADE80` |
| Tools        | (167, 139, 250)   | `#A78BFA` |
| Productivity | (251, 191, 36)    | `#FBBF24` |
| Games        | (239, 68, 68)     | `#EF4444` |
| Social       | (249, 146, 60)    | `#F9923C` |
| Media        | (236, 72, 153)    | `#EC4899` |

These match the existing `CATEGORY_COLORS` in the codebase and should be
standardized in the theme.

#### Button Hint Colors

Physical face buttons on the device have colors. The UI badges must match:

| Button  | RGB               | Hex       |
|---------|-------------------|-----------|
| A       | (80, 200, 80)     | `#50C850` |
| B       | (220, 80, 80)     | `#DC5050` |
| X       | (80, 140, 240)    | `#508CF0` |
| Y       | (230, 200, 60)    | `#E6C83C` |
| L/R     | (140, 140, 160)   | `#8C8CA0` |

### Readability Rules

1. **Minimum font size is 11px.** Nothing smaller. At 640x480 on a 3.5" screen,
   11px monospace is already pushing the limit.

2. **Minimum contrast ratio is 4.5:1** for body text, 3:1 for large text (WCAG AA).
   The current palette exceeds this comfortably.

3. **No thin font weights.** At this resolution, thin/light weights disappear.
   Use regular and bold only.

4. **Text truncation with ".."** rather than "..." -- saves one character of width,
   which matters at 640 pixels. (Already implemented in the SDK.)

5. **Maximum line length: ~60 characters.** At font_normal (16px) in monospace,
   this fills about 580px, leaving room for padding and a scrollbar.

---

## 5. Widget / Component Library

### Design Principles for All Widgets

1. **Every interactive widget must have a visible focus state.** The focused
   element must be visually distinguishable from unfocused elements without
   relying on color alone (use border + background change + optional accent strip).

2. **Every widget must declare which buttons it consumes.** This prevents input
   conflicts when widgets are composed (e.g., TabBar consumes L1/R1, ListView
   consumes D-up/D-down, and they coexist on the same screen).

3. **No widget should assume screen position.** Widgets receive a `rect` parameter
   and render within it. This allows apps to compose layouts freely.

4. **Animation budget: max 2 simultaneous animations per screen.** The RK3326
   at 30fps cannot handle complex compositing. Acceptable animations: fade
   in/out (alpha), slide (translate), and pulse (scale oscillation for loading).
   No rotation, no blur, no particle effects.

### Widget Catalog

#### Existing Widgets (already implemented, with design refinements)

**StatusBar** -- top chrome bar
- Height: 36px (reduced from 40px to save vertical space)
- Content: title left, status indicators right (wifi dot, clock)
- Recommendation: Add battery percentage. Add a thin 1px accent-colored line at
  the very top of the screen (y=0) as a subtle "brand bar" -- just a colored
  line that says "this is Cartridge."

**TabBar** -- horizontal tab switcher (L1/R1)
- Height: 30px
- Currently renders tabs with equal width -- this wastes space when tab labels
  vary in length. Recommendation: use auto-width tabs (label width + 24px
  padding), left-aligned, with horizontal scrolling if they overflow.

**ListView** -- scrollable list with card items
- Currently the most-used widget. Design is solid.
- Recommendation: Add a `compact` mode (32px row height) for dense lists like
  settings menus where secondary text is not needed. The current 48px default
  is correct for content lists (news stories, apps).

**DetailView** -- scrollable word-wrapped text
- Good for article reading, app descriptions, comment threads.
- Recommendation: Support inline bold/dim spans via a simple markup system
  (e.g., `*bold*` and `~dim~`), parsed at wrap time. This avoids apps needing
  to build custom renderers for rich text.

**Table** -- columnar data display
- Used by the Stock Market and System Monitor apps.
- Recommendation: Add alternating row backgrounds (card_bg and bg_lighter) for
  easier scanning. Add column sorting via X button.

**Toast** -- transient notification
- Currently renders bottom-center. Good position.
- Recommendation: Add an icon/dot on the left side using the semantic color
  (green dot for success, red for error, yellow for warning) so toasts are
  distinguishable at a glance without reading the text.

**LoadingIndicator** -- animated dots overlay
- Functional but visually basic.
- Recommendation: Replace the "Loading..." text with a small spinning/pulsing
  indicator plus text. A simple approach: cycle through characters
  `|`, `/`, `-`, `\` (classic terminal spinner) at 4fps, rendered in accent
  color. This is more visually engaging and fits the aesthetic.

**SparkLine / LineChart** -- mini data visualization
- Used in System Monitor. Clean implementation.
- No changes needed.

#### New Widgets Needed

**GridView** -- 2D navigable grid of cards

For the home screen dock and potentially for an icon-based store view:

```
+--------+  +--------+  +--------+  +--------+
|  icon  |  |  icon  |  |  icon  |  |  icon  |
|  label |  |  label |  | [label]|  |  label |
+--------+  +--------+  +--------+  +--------+
                          ^focused
```

Specification:
- Cell size: configurable, default 120x100
- Columns: auto-calculated from available width (640px / 120px = 5 columns)
- D-pad: left/right moves within row, up/down moves between rows
- A: select, B: back
- L1/R1: page up/down (jump one full screen of rows)
- Focus state: accent border (2px) + card_highlight background
- Scroll: vertical, smooth (snap to row boundaries)

**ConfirmDialog** -- modal yes/no confirmation

For destructive actions (uninstall, clear data, switch to ES):

```
+----------------------------------------------+
|                                              |
|    +------------------------------------+    |
|    |  Remove "Hacker News"?             |    |
|    |                                    |    |
|    |  This will delete all app data.    |    |
|    |                                    |    |
|    |    [ Cancel ]      [> Remove ]     |    |
|    +------------------------------------+    |
|                                              |
+----------------------------------------------+
```

Specification:
- Centered modal, max 400x200
- Dark semi-transparent overlay behind it (bg at 70% opacity)
- Title (bold, 16px) + body (normal, 14px) + two buttons
- D-left/D-right to switch between buttons
- A to confirm, B to cancel (always cancels, even if "Confirm" is focused)
- Destructive action button highlighted in `negative` color
- Non-destructive button uses `card_bg`

**ProgressBar** -- download/install progress

Already exists as `screen.draw_progress_bar()` but should be promoted to a
proper widget with:
- Label above: "Downloading..." or "Installing..."
- Percentage text right-aligned: "67%"
- Estimated time remaining (optional)
- Uses accent color for fill, bg_lighter for track
- Height: 12px bar + 16px label = 28px total

**TextInput** -- on-screen keyboard

The hard widget. Required for search in the store, WiFi passwords, and any app
that needs text entry. Design:

```
+------------------------------------------------------------+
|  Search: hacker new_                                       |
|                                                            |
|  +------------------------------------------------------+  |
|  |  1  2  3  4  5  6  7  8  9  0  -  =  <Bksp>         |  |
|  |  q  w  e  r  t  y  u  i  o  p  [  ]                  |  |
|  |  a  s  d  f  g  h  j  k  l  ;  '  <Enter>            |  |
|  |  z  x  c  v  b  n  m  ,  .  /  <Space>               |  |
|  +------------------------------------------------------+  |
+------------------------------------------------------------+
```

Specification:
- QWERTY grid layout, 12-13 columns x 4 rows
- D-pad navigates the grid cell by cell
- A types the focused character
- B deletes last character (backspace)
- X toggles uppercase/symbols
- Y inserts space
- L1: move cursor left in input field
- R1: move cursor right in input field
- START: submit/confirm input
- Each key cell: 44x36px, fits 14 columns in 616px (with margins)
- Focused key: accent border + card_highlight bg
- Input field at top: full width, 36px tall, shows current text with blinking
  cursor (underscore, toggled every 500ms)

**Sidebar** -- slide-in panel

For settings, filters, or context menus that do not warrant a full screen change:

```
+---------------------------+------------------------+
|                           |  Settings              |
|   (dimmed main content)   |                        |
|                           |  > WiFi                |
|                           |    Storage              |
|                           |    Display              |
|                           |    About                |
|                           |                        |
|                           |  [B] Close              |
+---------------------------+------------------------+
```

Specification:
- Width: 280px (right-aligned, leaves 360px of dimmed content visible)
- Full height of content area (408px)
- Slide-in from right (simple translate animation, 150ms)
- Contains a ListView for menu items
- B closes the sidebar
- Overlay darkens the left portion (bg at 50% opacity)

### Focus State Design (Universal)

The focus state is the single most important visual element in a gamepad-driven UI.
Every focusable element must implement this consistently:

```
UNFOCUSED:                    FOCUSED:
+------------------+         +------------------+
|  card_bg         |         |##card_highlight ##|
|  text_dim text   |         |## text (bright) ##|
|  no border       |         |## accent border ##|
+------------------+         +------------------+
                              2px accent border
                              bg shifts to card_highlight
                              text shifts to full brightness
```

Rules:
- Border: 2px, accent color (#64B4FF)
- Background: shifts from card_bg to card_highlight
- Text: shifts from text_dim to text
- Optional: 2px shadow_offset drop shadow appears
- Transition: instant (no fade) -- at 30fps, fades look choppy

### Layout Constants

Standard spacing values that all widgets and apps should use:

| Token              | Value | Use                                      |
|--------------------|-------|------------------------------------------|
| `padding`          | 10px  | Standard padding inside cards/panels      |
| `padding_small`    | 6px   | Compact padding for dense layouts         |
| `margin`           | 8px   | Space between cards/sections              |
| `margin_small`     | 4px   | Tight spacing between related elements    |
| `header_height`    | 36px  | Status bar height                         |
| `footer_height`    | 36px  | Button hint bar height                    |
| `tab_height`       | 30px  | Tab bar height                            |
| `item_height`      | 48px  | Standard list item height                 |
| `item_height_compact` | 32px | Compact list item height              |
| `card_radius`      | 6px   | Border radius for cards                   |
| `button_radius`    | 4px   | Border radius for buttons/pills           |
| `icon_size`        | 48px  | Standard app icon size                    |
| `icon_size_small`  | 32px  | Small icon (recent strip, lists)          |
| `icon_size_large`  | 64px  | Large icon (detail page, home dock)       |
| `content_top`      | 36px  | Y position where content starts           |
| `content_bottom`   | 444px | Y position where content ends             |
| `content_height`   | 408px | Available height for content              |
| `screen_width`     | 640px | Total screen width                        |
| `screen_height`    | 480px | Total screen height                       |

---

## 6. App Windowing Model

### Apps Run Fullscreen

Every app gets the full 640x480 surface. There is no windowing, no split-screen,
no picture-in-picture. This is a deliberate constraint:

- 640x480 is too small to split meaningfully
- Fullscreen simplifies the rendering model (one app renders at a time)
- It matches user expectations from game consoles and existing handheld launchers
- It eliminates an entire class of complexity (window management, z-ordering, resize)

### App Lifecycle States

```
                    +----------+
                    |  STOPPED |  (not running, no process)
                    +----+-----+
                         |
                    [user launches]
                         |
                    +----v-----+
                    |  RUNNING |  (owns the screen, receives input)
                    +----+-----+
                         |
              [SELECT+START or B-at-root]
                         |
                    +----v------+
                    | SUSPENDED |  (state preserved, no rendering)
                    +----+------+
                         |
                    [user re-selects app from home]
                         |
                    +----v-----+
                    |  RUNNING |  (on_resume() called, state restored)
                    +----------+
```

Only ONE app is in RUNNING state at any time. The home screen / launcher is itself
an app (cartridge-client) and follows the same lifecycle.

When an app is SUSPENDED:
- Its Python process is paused (SIGSTOP) or its game loop is halted
- Its last frame is captured as a thumbnail (used on the home screen)
- Its memory footprint is preserved (the R36S Plus has 1GB RAM -- enough for
  2-3 suspended Python apps)
- on_suspend() is called so the app can save state
- on_resume() is called when the user returns

When an app is STOPPED:
- Its process is killed
- on_destroy() is called for cleanup
- Only happens on explicit "close" or on low-memory pressure

### System Overlay Layer

Above the running app, there is a thin system overlay layer managed by the runner,
not by the app. This handles:

1. **Toast notifications from the system** (e.g., "WiFi disconnected",
   "App update available", "Low battery"). Rendered at the bottom of the screen,
   above the app's content, with a semi-transparent background.

2. **The boot selector overlay** (SELECT press). Rendered centered, modal, with
   darkened background. App is paused while this is visible.

3. **The "returning home" indicator** (SELECT+START hold). A small progress bar
   or countdown rendered at top-center while the user holds the combo, confirming
   they want to go home. Disappears if they release early.

4. **Screenshot capture** (L2+R2 simultaneous press, future feature). Brief flash
   effect + "Screenshot saved" toast.

Apps CANNOT draw on the overlay layer. It is reserved for the system.

### Switching Between Running Apps

For v1, there is no app switcher. The flow is:

1. User is in App A
2. Presses SELECT+START to go HOME
3. App A is SUSPENDED
4. User launches App B from HOME
5. App B is RUNNING
6. User presses SELECT+START to go HOME
7. App B is SUSPENDED
8. User navigates to App A in the dock (it shows a small "suspended" indicator)
9. Presses A to resume
10. App A is RUNNING again (on_resume() called)

For a future version, a quick-switcher (hold SELECT, D-left/D-right to cycle
through suspended apps, release to switch) would be a natural evolution. But for
v1, the explicit HOME route is simpler and more predictable.

### What the SDK Enforces vs. What Apps Control

| Concern                | Handled by           |
|------------------------|----------------------|
| Screen resolution      | Runner (fixed 640x480) |
| Frame rate             | Runner (30fps cap)    |
| Header/footer chrome   | App (optional, SDK provides StatusBar/footer) |
| B = back behavior      | Runner (intercepts at root, app handles in sub-screens) |
| SELECT+START = home    | Runner (always, app cannot override) |
| SELECT = boot selector | Runner (always, app cannot override) |
| Toast overlay          | Runner (system toasts) + App (app toasts via ToastManager) |
| Input routing          | Runner (passes to app after intercepting system combos) |
| Suspend/resume         | Runner (manages process state, calls lifecycle hooks) |
| Theme/colors           | SDK (provides theme, app can read but should not override) |

---

## Appendix A: Screen Layout Reference (Pixel-Exact)

### Home Screen

```
Y=0    +----------------------------------------------------------+ x=0
       | HEADER: "Cartridge"              WiFi *  BAT 73%  12:34  | 36px
Y=36   +----------------------------------------------------------+
       |                                                          |
Y=56   |  [80x80] [80x80] [80x80] [80x80] [80x80]               | dock row
       |   icon    icon    icon   >icon<    icon                  | 100px
Y=156  |                                                          |
       |  +----------------------------------------------------+  |
Y=170  |  |  App Name                                  v0.1.0  |  | detail
       |  |  Description text goes here and can wrap to        |  | pane
       |  |  multiple lines if needed                          |  | 180px
       |  |  Category: TOOLS     Author: Cartridge Team        |  |
       |  |  Permissions: network, storage                     |  |
Y=350  |  +----------------------------------------------------+  |
       |                                                          |
Y=370  |  Recently opened:                                        | recent
       |  [40] Hacker News 2m  [40] Stocks 1h  [40] Weather 3h  | strip
Y=430  |                                                          | 60px
       |                                                          |
Y=444  +----------------------------------------------------------+
       | FOOTER: [A] Open  [Y] Store  [X] Remove  [START] Set    | 36px
Y=480  +----------------------------------------------------------+
```

### Store Screen (current implementation, refined)

```
Y=0    +----------------------------------------------------------+
       | HEADER: "Cartridge"              WiFi *  6 apps   12:34  | 36px
Y=36   +----------------------------------------------------------+
       | TABS: [All] [News] [Finance] [Tools] [Prod] [Games] ... | 30px
Y=66   +----------------------------------------------------------+
       |  +----------------------------------------------------+  |
       |  | > App Name                     INSTALLED   FINANCE  |  | 76px
       |  |   Description text here...                          |  | per
       |  |   Author  v0.1.0                                   |  | row
       |  +----------------------------------------------------+  |
       |  +----------------------------------------------------+  |
       |  |   App Name                                  TOOLS  |  |
       |  |   Description text here...                          |  |
       |  |   Author  v0.1.0                                   |  |
       |  +----------------------------------------------------+  |
       |  +----------------------------------------------------+  |
       |  |   App Name                                  NEWS   |  |
       |  |   Description text here...                          |  |
       |  |   Author  v0.1.0                                   |  |
       |  +----------------------------------------------------+  |
       |                                                     [|]  | scroll
Y=444  +----------------------------------------------------------+
       | FOOTER: [L1/R1] Cat  [A] Detail  [Y] Installed  [X] Ref | 36px
Y=480  +----------------------------------------------------------+
```

### App Detail Screen

```
Y=0    +----------------------------------------------------------+
       | HEADER: < Back                           WiFi *   12:34  | 36px
Y=36   +----------------------------------------------------------+
       |                                                          |
       |  App Name                                        v0.1.0  |
       |  by Author Name                                         |
       |                                                          |
       |  +----------------------------------------------------+  |
       |  |                                                    |  |
       |  |              Screenshot carousel                   |  | 200px
       |  |              (L/R to scroll)                       |  |
       |  |                                                    |  |
       |  +----------------------------------------------------+  |
       |                                                          |
       |  Description text here, word-wrapped across              |
       |  multiple lines. Scrollable with D-up/D-down.           |
       |                                                          |
       |  Category: NEWS       Size: 245 KB                       |
       |  Trust: CORE          Permissions: network, storage      |
       |                                                          |
       |  [ Install ]  or  [ Launch ]  [ Remove ]                 |
       |                                                          |
Y=444  +----------------------------------------------------------+
       | FOOTER: [A] Install  [B] Back  [L/R] Screenshots        | 36px
Y=480  +----------------------------------------------------------+
```

---

## Appendix B: Recommended Font -- Bundling Strategy

The SDK should bundle a single monospace font family as part of the package. The
recommended approach:

```
cartridge_sdk/
  assets/
    fonts/
      CascadiaMono-Regular.ttf
      CascadiaMono-Bold.ttf
```

The font loading code (`screen.py`) should check for bundled fonts FIRST, then
fall back to system fonts. This ensures visual consistency across devices regardless
of what fonts the firmware ships with.

The font files add approximately 400KB to the SDK -- negligible for distribution.

---

## Appendix C: Theme Variations (Future)

While v1 ships with a single dark theme, the architecture should support theme
switching. Potential future themes:

| Theme Name   | Background      | Accent          | Character              |
|--------------|-----------------|-----------------|------------------------|
| Default      | Dark blue-black | Medium blue     | The standard           |
| Amber        | Dark warm black | Amber/orange    | CRT terminal feel      |
| Phosphor     | Dark green-black| Terminal green   | Classic hacker         |
| Slate        | Dark gray       | Teal            | Modern neutral         |
| High Contrast| Pure black      | White + yellow  | Accessibility          |

The theme system should swap ONLY the color tokens listed in Section 4.
Typography, spacing, and layout constants remain fixed. This prevents themes
from breaking layouts.
