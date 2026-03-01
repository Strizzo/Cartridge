-- System Monitor for Cartridge OS (Lua)
-- Real-time CPU, memory, disk, and network monitoring with cyberdeck aesthetic

-- ── Constants ────────────────────────────────────────────────────────────────

local SPARKLINE_LENGTH = 40
local UPDATE_INTERVAL = 0.5     -- sample every 500ms
local SMOOTH_FACTOR = 0.15      -- low-pass filter for realistic drift

-- Metric card identifiers
local METRIC_CPU = "cpu"
local METRIC_MEM = "memory"
local METRIC_DISK = "disk"
local METRIC_NET = "network"

local METRICS_ORDER = {METRIC_CPU, METRIC_MEM, METRIC_DISK, METRIC_NET}

local METRIC_LABELS = {
    [METRIC_CPU] = "CPU",
    [METRIC_MEM] = "MEMORY",
    [METRIC_DISK] = "DISK",
    [METRIC_NET] = "NETWORK",
}

local METRIC_UNITS = {
    [METRIC_CPU] = "%",
    [METRIC_MEM] = "%",
    [METRIC_DISK] = "%",
    [METRIC_NET] = "KB/s",
}

local METRIC_COLORS = {
    [METRIC_CPU] = {80, 200, 255},
    [METRIC_MEM] = {200, 120, 255},
    [METRIC_DISK] = {255, 180, 60},
    [METRIC_NET] = {80, 220, 160},
}

local CPU_CORES = 4

-- ── Pseudo-random number generator (deterministic, seedable) ────────────────

local prng_state = 0

local function prng_seed(s)
    prng_state = s
end

local function prng_next()
    -- xorshift32
    prng_state = prng_state ~ (prng_state << 13)
    prng_state = prng_state ~ (prng_state >> 17)
    prng_state = prng_state ~ (prng_state << 5)
    prng_state = prng_state & 0x7FFFFFFF
    return prng_state
end

local function prng_float()
    return (prng_next() % 10000) / 10000.0
end

-- ── Simulation Engine ────────────────────────────────────────────────────────
-- Generates realistic-looking system metrics that evolve smoothly over time.
-- Each metric has a "target" value that drifts via a random walk, and the
-- current value chases the target with exponential smoothing.

local sim = {
    -- CPU overall
    cpu_target = 35,
    cpu_current = 35,
    cpu_history = {},
    -- CPU per-core
    core_targets = {},
    core_currents = {},
    core_histories = {},
    -- Memory
    mem_target = 62,
    mem_current = 62,
    mem_history = {},
    mem_total_mb = 2048,
    -- Disk
    disk_target = 54,
    disk_current = 54,
    disk_history = {},
    disk_total_gb = 32,
    -- Network
    net_target = 120,
    net_current = 120,
    net_history = {},
    net_rx_total_kb = 0,
    net_tx_total_kb = 0,
    net_rx_rate = 80,
    net_tx_rate = 40,
    -- Lua VM
    lua_mem_kb = 0,
    -- Timing
    uptime_start = 0,
    sample_timer = 0,
    tick = 0,
}

local function clamp(v, lo, hi)
    if v < lo then return lo end
    if v > hi then return hi end
    return v
end

local function drift_target(current_target, min_val, max_val, volatility)
    local delta = (prng_float() - 0.5) * 2.0 * volatility
    -- Mean-revert toward center
    local center = (min_val + max_val) / 2
    local revert = (center - current_target) * 0.02
    return clamp(current_target + delta + revert, min_val, max_val)
end

local function smooth(current, target, factor)
    return current + (target - current) * factor
end

local function push_history(history, value)
    history[#history + 1] = value
    if #history > SPARKLINE_LENGTH then
        table.remove(history, 1)
    end
end

local function init_simulation()
    prng_seed(os.time() * 1000 + math.floor(os.clock() * 1000))

    sim.uptime_start = os.time()

    -- Initialize per-core data
    for c = 1, CPU_CORES do
        local base = 20 + prng_float() * 40
        sim.core_targets[c] = base
        sim.core_currents[c] = base
        sim.core_histories[c] = {}
    end

    -- Pre-fill histories with some initial data so sparklines look populated
    for i = 1, SPARKLINE_LENGTH do
        prng_next()
        -- CPU
        sim.cpu_target = drift_target(sim.cpu_target, 5, 95, 6)
        sim.cpu_current = smooth(sim.cpu_current, sim.cpu_target, 0.3)
        push_history(sim.cpu_history, sim.cpu_current)
        -- Cores
        for c = 1, CPU_CORES do
            sim.core_targets[c] = drift_target(sim.core_targets[c], 2, 98, 8)
            sim.core_currents[c] = smooth(sim.core_currents[c], sim.core_targets[c], 0.3)
            push_history(sim.core_histories[c], sim.core_currents[c])
        end
        -- Memory (slow drift)
        sim.mem_target = drift_target(sim.mem_target, 40, 85, 1.5)
        sim.mem_current = smooth(sim.mem_current, sim.mem_target, 0.2)
        push_history(sim.mem_history, sim.mem_current)
        -- Disk (very slow drift)
        sim.disk_target = drift_target(sim.disk_target, 30, 80, 0.3)
        sim.disk_current = smooth(sim.disk_current, sim.disk_target, 0.1)
        push_history(sim.disk_history, sim.disk_current)
        -- Network (bursty)
        sim.net_target = drift_target(sim.net_target, 0, 800, 40)
        sim.net_current = smooth(sim.net_current, sim.net_target, 0.25)
        push_history(sim.net_history, sim.net_current)
    end
end

local function update_simulation(dt)
    sim.sample_timer = sim.sample_timer + dt
    if sim.sample_timer < UPDATE_INTERVAL then return end
    sim.sample_timer = sim.sample_timer - UPDATE_INTERVAL
    sim.tick = sim.tick + 1

    prng_next()

    -- Occasional "spike" events for realism
    local spike = prng_float()

    -- CPU overall
    local cpu_vol = 4
    if spike < 0.03 then cpu_vol = 25 end  -- occasional spike
    sim.cpu_target = drift_target(sim.cpu_target, 5, 95, cpu_vol)
    sim.cpu_current = smooth(sim.cpu_current, sim.cpu_target, SMOOTH_FACTOR)
    push_history(sim.cpu_history, sim.cpu_current)

    -- Per-core
    for c = 1, CPU_CORES do
        local core_vol = 6
        if prng_float() < 0.05 then core_vol = 30 end
        sim.core_targets[c] = drift_target(sim.core_targets[c], 2, 98, core_vol)
        sim.core_currents[c] = smooth(sim.core_currents[c], sim.core_targets[c], SMOOTH_FACTOR * 1.2)
        push_history(sim.core_histories[c], sim.core_currents[c])
    end

    -- Memory (slow, stable)
    sim.mem_target = drift_target(sim.mem_target, 40, 85, 1.0)
    sim.mem_current = smooth(sim.mem_current, sim.mem_target, SMOOTH_FACTOR * 0.5)
    push_history(sim.mem_history, sim.mem_current)

    -- Disk (very slow)
    sim.disk_target = drift_target(sim.disk_target, 30, 80, 0.2)
    sim.disk_current = smooth(sim.disk_current, sim.disk_target, SMOOTH_FACTOR * 0.3)
    push_history(sim.disk_history, sim.disk_current)

    -- Network (bursty with quiet periods)
    local net_vol = 20
    if prng_float() < 0.1 then
        sim.net_target = drift_target(sim.net_target, 200, 800, 100) -- burst
    elseif prng_float() < 0.15 then
        sim.net_target = drift_target(sim.net_target, 0, 50, 30)     -- quiet
    else
        sim.net_target = drift_target(sim.net_target, 10, 500, net_vol)
    end
    sim.net_current = smooth(sim.net_current, sim.net_target, SMOOTH_FACTOR)
    sim.net_current = math.max(0, sim.net_current)
    push_history(sim.net_history, sim.net_current)

    -- Network totals accumulate
    sim.net_rx_rate = sim.net_current * 0.65
    sim.net_tx_rate = sim.net_current * 0.35
    sim.net_rx_total_kb = sim.net_rx_total_kb + sim.net_rx_rate * UPDATE_INTERVAL
    sim.net_tx_total_kb = sim.net_tx_total_kb + sim.net_tx_rate * UPDATE_INTERVAL

    -- Lua VM memory (real measurement)
    sim.lua_mem_kb = collectgarbage("count")
end

-- ── State ────────────────────────────────────────────────────────────────────

local state = {
    screen_stack = {"dashboard"},
    -- Dashboard
    selected_card = 1,     -- 1-4 for the four metric cards
    -- Animation
    pulse_t = 0,
    blink_on = true,
    blink_timer = 0,
    -- Tab navigation (dashboard tabs via L1/R1)
    active_tab = 1,        -- 1=Dashboard, 2=CPU, 3=Memory, 4=Processes
    tab_labels = {"DASH", "CPU", "MEM", "SYS"},
}

-- ── Utility Functions ────────────────────────────────────────────────────────

local function status_color(pct)
    if pct < 50 then
        return {80, 220, 120}    -- green
    elseif pct < 75 then
        return {255, 200, 60}    -- yellow
    else
        return {255, 80, 80}     -- red
    end
end

local function format_uptime(seconds)
    local days = math.floor(seconds / 86400)
    local hours = math.floor((seconds % 86400) / 3600)
    local mins = math.floor((seconds % 3600) / 60)
    local secs = seconds % 60
    if days > 0 then
        return string.format("%dd %02d:%02d:%02d", days, hours, mins, secs)
    end
    return string.format("%02d:%02d:%02d", hours, mins, secs)
end

local function format_bytes(kb)
    if kb >= 1048576 then
        return string.format("%.1f GB", kb / 1048576)
    elseif kb >= 1024 then
        return string.format("%.1f MB", kb / 1024)
    else
        return string.format("%.0f KB", kb)
    end
end

-- ── Drawing Helpers ──────────────────────────────────────────────────────────

local function draw_header(title, right_text, right_color)
    screen.draw_gradient_rect(0, 0, 720, 40,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 720, 0, {color=theme.accent})

    -- Title with monospace feel
    screen.draw_text(title, 12, 10, {color=theme.text, size=20, bold=true})

    -- Right side status
    if right_text then
        local rc = right_color or theme.text_dim
        local rw = screen.get_text_width(right_text, 12, false)
        screen.draw_text(right_text, 704 - rw, 14, {color=rc, size=12})
    end
end

local function draw_footer(hints)
    screen.draw_rect(0, 684, 720, 36, {color=theme.bg_header, filled=true})
    screen.draw_line(0, 684, 720, 684, {color=theme.border})
    local x = 10
    for _, h in ipairs(hints) do
        local w = screen.draw_button_hint(h[1], h[2], x, 692, {color=h[3], size=12})
        x = x + w + 14
    end
end

local function draw_tab_bar(labels, active_idx, y)
    screen.draw_rect(0, y, 720, 28, {color=theme.bg_header, filled=true})
    local tx = 10
    for i, label in ipairs(labels) do
        local is_active = (i == active_idx)
        local tw = screen.get_text_width(label, 11, is_active)
        local tab_w = tw + 16
        if is_active then
            screen.draw_rect(tx, y + 3, tab_w, 22, {color=theme.accent, filled=true, radius=4})
            screen.draw_text(label, tx + 8, y + 6, {color={20, 20, 30}, size=11, bold=true})
        else
            screen.draw_rect(tx, y + 3, tab_w, 22, {color=theme.card_bg, filled=true, radius=4})
            screen.draw_text(label, tx + 8, y + 6, {color=theme.text_dim, size=11})
        end
        tx = tx + tab_w + 6
    end
    screen.draw_line(0, y + 28, 720, y + 28, {color=theme.border})
end

local function draw_scanline_overlay(y_start, height, alpha_step)
    -- Subtle scanline effect for cyberdeck aesthetic
    local step = alpha_step or 4
    for sy = y_start, y_start + height, step do
        screen.draw_line(0, sy, 720, sy, {color={0, 0, 0}, width=1})
    end
end

-- ── Dashboard Screen ─────────────────────────────────────────────────────────

local function draw_metric_card(x, y, w, h, metric_id, is_selected)
    local label = METRIC_LABELS[metric_id]
    local color = METRIC_COLORS[metric_id]
    local unit = METRIC_UNITS[metric_id]

    local value, history
    if metric_id == METRIC_CPU then
        value = sim.cpu_current
        history = sim.cpu_history
    elseif metric_id == METRIC_MEM then
        value = sim.mem_current
        history = sim.mem_history
    elseif metric_id == METRIC_DISK then
        value = sim.disk_current
        history = sim.disk_history
    elseif metric_id == METRIC_NET then
        value = sim.net_current
        history = sim.net_history
    end

    local sc = status_color(metric_id == METRIC_NET and (value / 8) or value)

    -- Card background
    local border_color = is_selected and theme.accent or theme.border
    local bg = is_selected and theme.card_highlight or theme.card_bg
    screen.draw_card(x, y, w, h, {bg=bg, border=border_color, radius=8, shadow=true})

    -- Selection indicator - glowing left edge
    if is_selected then
        screen.draw_rect(x, y + 4, 3, h - 8, {color=color, filled=true, radius=1})
    end

    -- Label (top-left, small caps feel)
    screen.draw_text(label, x + 12, y + 8, {color=color, size=11, bold=true})

    -- Status dot
    local dot_x = x + 12 + screen.get_text_width(label, 11, true) + 8
    screen.draw_circle(dot_x, y + 14, 3, sc[1], sc[2], sc[3])

    -- Current value (large)
    local val_str
    if metric_id == METRIC_NET then
        val_str = string.format("%.0f", value)
    else
        val_str = string.format("%.1f", value)
    end
    screen.draw_text(val_str, x + 12, y + 26, {color=theme.text, size=24, bold=true})

    -- Unit
    local val_w = screen.get_text_width(val_str, 24, true)
    screen.draw_text(unit, x + 14 + val_w, y + 34, {color=theme.text_dim, size=13})

    -- Sparkline (right portion of card)
    local spark_x = x + w * 0.42
    local spark_w = w * 0.54
    local spark_y = y + 8
    local spark_h = h - 16
    if history and #history >= 2 then
        screen.draw_sparkline(history, spark_x, spark_y, spark_w, spark_h, {color=color})
    end

    -- Subtle progress bar at bottom
    local bar_y = y + h - 6
    local bar_w = w - 24
    local progress = metric_id == METRIC_NET and clamp(value / 800, 0, 1) or clamp(value / 100, 0, 1)
    screen.draw_progress_bar(x + 12, bar_y, bar_w, 3, progress, {
        fill_color=color,
        bg_color=theme.bg_lighter,
        radius=1,
    })
end

local function draw_dashboard_screen()
    local uptime_secs = os.time() - sim.uptime_start
    local uptime_str = "UP " .. format_uptime(uptime_secs)

    -- Blinking status indicator
    local status_str = state.blink_on and "LIVE" or "    "
    draw_header("System Monitor", uptime_str, theme.text_dim)
    draw_tab_bar(state.tab_labels, 1, 40)

    local content_y = 72

    -- Status bar with system summary
    screen.draw_rect(0, content_y, 720, 22, {color=theme.bg_lighter, filled=true})
    screen.draw_text(status_str, 10, content_y + 4, {color={80, 220, 120}, size=11, bold=true})

    local time_str = os.date("%H:%M:%S")
    local tw = screen.get_text_width(time_str, 11, false)
    screen.draw_text(time_str, 704 - tw, content_y + 4, {color=theme.text_dim, size=11})

    -- Lua VM memory in status bar
    local lua_str = string.format("LUA %.0fKB", sim.lua_mem_kb)
    local lw = screen.get_text_width(lua_str, 11, false)
    screen.draw_text(lua_str, 360 - lw / 2, content_y + 4, {color=theme.text_dim, size=11})

    screen.draw_line(0, content_y + 22, 720, content_y + 22, {color=theme.border})
    content_y = content_y + 26

    -- Four metric cards in a 2x2 grid
    local card_w = 304
    local card_h = 90
    local gap = 8
    local grid_x = (720 - (card_w * 2 + gap)) / 2

    for i, metric_id in ipairs(METRICS_ORDER) do
        local col = (i - 1) % 2
        local row = math.floor((i - 1) / 2)
        local cx = grid_x + col * (card_w + gap)
        local cy = content_y + row * (card_h + gap)
        local is_selected = (state.selected_card == i)
        draw_metric_card(cx, cy, card_w, card_h, metric_id, is_selected)
    end

    -- Bottom info section
    local info_y = content_y + 2 * (card_h + gap) + 8

    -- System info card
    screen.draw_card(10, info_y, 700, 78, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})

    -- Column 1: Memory details
    local used_mb = math.floor(sim.mem_current / 100 * sim.mem_total_mb)
    local free_mb = sim.mem_total_mb - used_mb
    screen.draw_text("RAM", 24, info_y + 8, {color=METRIC_COLORS[METRIC_MEM], size=11, bold=true})
    screen.draw_text(string.format("%d / %d MB", used_mb, sim.mem_total_mb), 24, info_y + 24, {color=theme.text, size=13})
    screen.draw_text(string.format("Free: %d MB", free_mb), 24, info_y + 44, {color=theme.text_dim, size=11})
    screen.draw_progress_bar(24, info_y + 62, 160, 4, sim.mem_current / 100, {
        fill_color=METRIC_COLORS[METRIC_MEM], bg_color=theme.bg_lighter, radius=2,
    })

    -- Column 2: Disk details
    local disk_used_gb = sim.disk_current / 100 * sim.disk_total_gb
    local disk_free_gb = sim.disk_total_gb - disk_used_gb
    screen.draw_text("DISK", 248, info_y + 8, {color=METRIC_COLORS[METRIC_DISK], size=11, bold=true})
    screen.draw_text(string.format("%.1f / %d GB", disk_used_gb, sim.disk_total_gb), 248, info_y + 24, {color=theme.text, size=13})
    screen.draw_text(string.format("Free: %.1f GB", disk_free_gb), 248, info_y + 44, {color=theme.text_dim, size=11})
    screen.draw_progress_bar(248, info_y + 62, 180, 4, sim.disk_current / 100, {
        fill_color=METRIC_COLORS[METRIC_DISK], bg_color=theme.bg_lighter, radius=2,
    })

    -- Column 3: Network totals
    screen.draw_text("NET I/O", 488, info_y + 8, {color=METRIC_COLORS[METRIC_NET], size=11, bold=true})
    screen.draw_text("RX: " .. format_bytes(sim.net_rx_total_kb), 488, info_y + 24, {color=theme.text, size=13})
    screen.draw_text("TX: " .. format_bytes(sim.net_tx_total_kb), 488, info_y + 44, {color=theme.text, size=13})
    screen.draw_text(string.format("%.0f / %.0f KB/s", sim.net_rx_rate, sim.net_tx_rate), 488, info_y + 60, {color=theme.text_dim, size=11})

    draw_footer({
        {"A", "Detail", theme.btn_a},
        {"L1/R1", "Tab", theme.btn_l},
    })
end

-- ── CPU Detail Screen ────────────────────────────────────────────────────────

local function draw_cpu_detail_screen()
    local uptime_secs = os.time() - sim.uptime_start
    draw_header("CPU Monitor", format_uptime(uptime_secs), theme.text_dim)
    draw_tab_bar(state.tab_labels, 2, 40)

    local content_y = 72

    -- Overall CPU usage bar
    screen.draw_card(10, content_y, 700, 60, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})

    local cpu_color = METRIC_COLORS[METRIC_CPU]
    local sc = status_color(sim.cpu_current)

    screen.draw_text("CPU TOTAL", 24, content_y + 6, {color=cpu_color, size=11, bold=true})
    screen.draw_circle(24 + screen.get_text_width("CPU TOTAL", 11, true) + 8, content_y + 12, 3, sc[1], sc[2], sc[3])

    local pct_str = string.format("%.1f%%", sim.cpu_current)
    local pct_w = screen.get_text_width(pct_str, 20, true)
    screen.draw_text(pct_str, 696 - pct_w, content_y + 4, {color=theme.text, size=20, bold=true})

    -- Large progress bar
    screen.draw_progress_bar(24, content_y + 28, 672, 10, sim.cpu_current / 100, {
        fill_color=cpu_color, bg_color=theme.bg_lighter, radius=4,
    })

    -- Load description
    local load_desc
    if sim.cpu_current < 20 then load_desc = "IDLE"
    elseif sim.cpu_current < 50 then load_desc = "NORMAL"
    elseif sim.cpu_current < 75 then load_desc = "MODERATE"
    elseif sim.cpu_current < 90 then load_desc = "HIGH"
    else load_desc = "CRITICAL" end
    screen.draw_text(load_desc, 24, content_y + 44, {color=sc, size=11})

    content_y = content_y + 68

    -- Overall sparkline card
    screen.draw_card(10, content_y, 700, 80, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})
    screen.draw_text("CPU HISTORY", 24, content_y + 6, {color=theme.text_dim, size=10})

    -- Min/max labels
    if #sim.cpu_history > 0 then
        local lo, hi = sim.cpu_history[1], sim.cpu_history[1]
        for _, v in ipairs(sim.cpu_history) do
            if v < lo then lo = v end
            if v > hi then hi = v end
        end
        screen.draw_text(string.format("%.0f%%", hi), 656, content_y + 4, {color={255, 180, 80}, size=10})
        screen.draw_text(string.format("%.0f%%", lo), 656, content_y + 64, {color={120, 190, 255}, size=10})
    end

    if #sim.cpu_history >= 2 then
        screen.draw_sparkline(sim.cpu_history, 24, content_y + 20, 628, 54, {color=cpu_color, baseline_color=theme.bg_lighter})
    end

    content_y = content_y + 88

    -- Per-core cards
    local core_card_w = 148
    local core_card_h = 100
    local core_gap = 8
    local cores_x = (720 - (core_card_w * CPU_CORES + core_gap * (CPU_CORES - 1))) / 2

    for c = 1, CPU_CORES do
        local cx = cores_x + (c - 1) * (core_card_w + core_gap)
        local cy = content_y
        local core_val = sim.core_currents[c]
        local core_sc = status_color(core_val)

        screen.draw_card(cx, cy, core_card_w, core_card_h, {bg=theme.card_bg, border=theme.border, radius=6, shadow=true})

        -- Core label
        screen.draw_text("CORE " .. (c - 1), cx + 8, cy + 6, {color=cpu_color, size=10, bold=true})

        -- Status dot
        local label_w = screen.get_text_width("CORE " .. (c - 1), 10, true)
        screen.draw_circle(cx + 10 + label_w + 6, cy + 12, 3, core_sc[1], core_sc[2], core_sc[3])

        -- Percentage
        local core_pct = string.format("%.0f%%", core_val)
        screen.draw_text(core_pct, cx + 8, cy + 22, {color=theme.text, size=18, bold=true})

        -- Per-core sparkline
        if #sim.core_histories[c] >= 2 then
            screen.draw_sparkline(sim.core_histories[c], cx + 8, cy + 48, core_card_w - 16, 36, {color=cpu_color})
        end

        -- Progress bar
        screen.draw_progress_bar(cx + 8, cy + 90, core_card_w - 16, 3, core_val / 100, {
            fill_color=cpu_color, bg_color=theme.bg_lighter, radius=1,
        })
    end

    draw_footer({
        {"B", "Back", theme.btn_b},
        {"L1/R1", "Tab", theme.btn_l},
    })
end

-- ── Memory Detail Screen ─────────────────────────────────────────────────────

local function draw_memory_detail_screen()
    local uptime_secs = os.time() - sim.uptime_start
    draw_header("Memory Monitor", format_uptime(uptime_secs), theme.text_dim)
    draw_tab_bar(state.tab_labels, 3, 40)

    local content_y = 72
    local mem_color = METRIC_COLORS[METRIC_MEM]
    local used_mb = math.floor(sim.mem_current / 100 * sim.mem_total_mb)
    local free_mb = sim.mem_total_mb - used_mb
    local sc = status_color(sim.mem_current)

    -- Overall memory usage card
    screen.draw_card(10, content_y, 700, 60, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})
    screen.draw_text("RAM USAGE", 24, content_y + 6, {color=mem_color, size=11, bold=true})
    screen.draw_circle(24 + screen.get_text_width("RAM USAGE", 11, true) + 8, content_y + 12, 3, sc[1], sc[2], sc[3])

    local pct_str = string.format("%.1f%%", sim.mem_current)
    local pct_w = screen.get_text_width(pct_str, 20, true)
    screen.draw_text(pct_str, 696 - pct_w, content_y + 4, {color=theme.text, size=20, bold=true})

    screen.draw_progress_bar(24, content_y + 28, 672, 10, sim.mem_current / 100, {
        fill_color=mem_color, bg_color=theme.bg_lighter, radius=4,
    })

    screen.draw_text(string.format("%d MB / %d MB", used_mb, sim.mem_total_mb), 24, content_y + 44, {color=theme.text_dim, size=11})

    content_y = content_y + 68

    -- Memory history sparkline
    screen.draw_card(10, content_y, 700, 80, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})
    screen.draw_text("MEMORY HISTORY", 24, content_y + 6, {color=theme.text_dim, size=10})

    if #sim.mem_history > 0 then
        local lo, hi = sim.mem_history[1], sim.mem_history[1]
        for _, v in ipairs(sim.mem_history) do
            if v < lo then lo = v end
            if v > hi then hi = v end
        end
        screen.draw_text(string.format("%.0f%%", hi), 656, content_y + 4, {color={255, 180, 80}, size=10})
        screen.draw_text(string.format("%.0f%%", lo), 656, content_y + 64, {color={120, 190, 255}, size=10})
    end

    if #sim.mem_history >= 2 then
        screen.draw_sparkline(sim.mem_history, 24, content_y + 20, 628, 54, {color=mem_color, baseline_color=theme.bg_lighter})
    end

    content_y = content_y + 88

    -- Memory breakdown cards
    -- Simulate memory categories as fractions of used memory
    local app_pct = sim.mem_current * 0.55
    local cache_pct = sim.mem_current * 0.25
    local buffers_pct = sim.mem_current * 0.12
    local shared_pct = sim.mem_current * 0.08

    local segments = {
        {label="Applications", pct=app_pct, mb=math.floor(app_pct / 100 * sim.mem_total_mb), color={220, 100, 255}},
        {label="Cache", pct=cache_pct, mb=math.floor(cache_pct / 100 * sim.mem_total_mb), color={100, 180, 255}},
        {label="Buffers", pct=buffers_pct, mb=math.floor(buffers_pct / 100 * sim.mem_total_mb), color={100, 220, 180}},
        {label="Shared", pct=shared_pct, mb=math.floor(shared_pct / 100 * sim.mem_total_mb), color={255, 200, 80}},
    }

    screen.draw_card(10, content_y, 700, 130, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})
    screen.draw_text("BREAKDOWN", 24, content_y + 8, {color=theme.text_dim, size=10})

    local row_y = content_y + 26
    for i, seg in ipairs(segments) do
        local sy = row_y + (i - 1) * 24

        -- Label
        screen.draw_rect(24, sy + 3, 8, 8, {color=seg.color, filled=true, radius=2})
        screen.draw_text(seg.label, 38, sy, {color=theme.text, size=12})

        -- Value
        local mb_str = string.format("%d MB", seg.mb)
        screen.draw_text(mb_str, 220, sy, {color=theme.text_dim, size=12})

        -- Progress bar
        screen.draw_progress_bar(310, sy + 2, 380, 8, seg.pct / 100, {
            fill_color=seg.color, bg_color=theme.bg_lighter, radius=3,
        })
    end

    content_y = content_y + 138

    -- Lua VM memory card
    screen.draw_card(10, content_y, 344, 50, {bg=theme.card_bg, border=theme.border, radius=6, shadow=true})
    screen.draw_text("LUA VM HEAP", 24, content_y + 6, {color={180, 220, 100}, size=10, bold=true})
    screen.draw_text(string.format("%.1f KB", sim.lua_mem_kb), 24, content_y + 24, {color=theme.text, size=16, bold=true})

    -- Free memory card
    screen.draw_card(366, content_y, 344, 50, {bg=theme.card_bg, border=theme.border, radius=6, shadow=true})
    screen.draw_text("FREE RAM", 380, content_y + 6, {color={80, 220, 160}, size=10, bold=true})
    screen.draw_text(string.format("%d MB", free_mb), 380, content_y + 24, {color=theme.text, size=16, bold=true})

    draw_footer({
        {"B", "Back", theme.btn_b},
        {"L1/R1", "Tab", theme.btn_l},
    })
end

-- ── System Info Screen ───────────────────────────────────────────────────────

local function draw_system_info_screen()
    local uptime_secs = os.time() - sim.uptime_start
    draw_header("System Info", format_uptime(uptime_secs), theme.text_dim)
    draw_tab_bar(state.tab_labels, 4, 40)

    local content_y = 72

    -- Disk usage card
    local disk_color = METRIC_COLORS[METRIC_DISK]
    local disk_used_gb = sim.disk_current / 100 * sim.disk_total_gb
    local disk_free_gb = sim.disk_total_gb - disk_used_gb
    local disk_sc = status_color(sim.disk_current)

    screen.draw_card(10, content_y, 700, 60, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})
    screen.draw_text("DISK USAGE", 24, content_y + 6, {color=disk_color, size=11, bold=true})
    screen.draw_circle(24 + screen.get_text_width("DISK USAGE", 11, true) + 8, content_y + 12, 3, disk_sc[1], disk_sc[2], disk_sc[3])

    local disk_pct_str = string.format("%.1f%%", sim.disk_current)
    local dpw = screen.get_text_width(disk_pct_str, 20, true)
    screen.draw_text(disk_pct_str, 696 - dpw, content_y + 4, {color=theme.text, size=20, bold=true})

    screen.draw_progress_bar(24, content_y + 28, 672, 10, sim.disk_current / 100, {
        fill_color=disk_color, bg_color=theme.bg_lighter, radius=4,
    })
    screen.draw_text(string.format("%.1f GB / %d GB  (%.1f GB free)", disk_used_gb, sim.disk_total_gb, disk_free_gb), 24, content_y + 44, {color=theme.text_dim, size=11})

    content_y = content_y + 68

    -- Network card
    local net_color = METRIC_COLORS[METRIC_NET]
    screen.draw_card(10, content_y, 700, 80, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})
    screen.draw_text("NETWORK ACTIVITY", 24, content_y + 6, {color=net_color, size=11, bold=true})

    if #sim.net_history >= 2 then
        screen.draw_sparkline(sim.net_history, 24, content_y + 22, 480, 50, {color=net_color, baseline_color=theme.bg_lighter})
    end

    -- Network stats (right side)
    screen.draw_text(string.format("%.0f KB/s", sim.net_current), 530, content_y + 22, {color=theme.text, size=16, bold=true})
    screen.draw_text(string.format("RX: %.0f KB/s", sim.net_rx_rate), 530, content_y + 44, {color={80, 200, 255}, size=12})
    screen.draw_text(string.format("TX: %.0f KB/s", sim.net_tx_rate), 530, content_y + 60, {color={255, 160, 80}, size=12})

    content_y = content_y + 88

    -- System information table
    screen.draw_card(10, content_y, 700, 170, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})
    screen.draw_text("SYSTEM", 24, content_y + 8, {color=theme.text_dim, size=10, bold=true})

    local info_rows = {
        {"Hostname", "cartridge-r36s"},
        {"Platform", "Linux ARM64"},
        {"Kernel", "6.1.43-sun50iw9"},
        {"CPU", "Allwinner H700 (4 cores)"},
        {"Display", "720x720 IPS"},
        {"Uptime", format_uptime(uptime_secs)},
        {"Lua VM", string.format("%.1f KB allocated", sim.lua_mem_kb)},
        {"Tick", string.format("#%d (%.0f samples)", sim.tick, sim.tick * UPDATE_INTERVAL)},
    }

    local row_y = content_y + 26
    for i, row in ipairs(info_rows) do
        local ry = row_y + (i - 1) * 18

        -- Alternating row background
        if i % 2 == 0 then
            screen.draw_rect(16, ry - 1, 688, 18, {color=theme.bg_lighter, filled=true, radius=2})
        end

        -- Key
        screen.draw_text(row[1], 24, ry, {color=theme.text_dim, size=12})
        -- Value
        screen.draw_text(row[2], 200, ry, {color=theme.text, size=12})
    end

    -- Network totals at bottom
    content_y = content_y + 178

    screen.draw_card(10, content_y, 344, 42, {bg=theme.card_bg, border=theme.border, radius=6, shadow=true})
    screen.draw_text("TOTAL RX", 24, content_y + 4, {color={80, 200, 255}, size=10, bold=true})
    screen.draw_text(format_bytes(sim.net_rx_total_kb), 24, content_y + 20, {color=theme.text, size=14, bold=true})

    screen.draw_card(366, content_y, 344, 42, {bg=theme.card_bg, border=theme.border, radius=6, shadow=true})
    screen.draw_text("TOTAL TX", 380, content_y + 4, {color={255, 160, 80}, size=10, bold=true})
    screen.draw_text(format_bytes(sim.net_tx_total_kb), 380, content_y + 20, {color=theme.text, size=14, bold=true})

    draw_footer({
        {"B", "Back", theme.btn_b},
        {"L1/R1", "Tab", theme.btn_l},
    })
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    init_simulation()
    sim.lua_mem_kb = collectgarbage("count")
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local current = state.screen_stack[#state.screen_stack]

    -- Tab switching with L1/R1 (works on all screens)
    if button == "l1" and action == "press" then
        state.active_tab = state.active_tab > 1 and (state.active_tab - 1) or #state.tab_labels
        -- Navigate to tab's screen
        if state.active_tab == 1 then
            state.screen_stack = {"dashboard"}
        elseif state.active_tab == 2 then
            state.screen_stack = {"cpu_detail"}
        elseif state.active_tab == 3 then
            state.screen_stack = {"memory_detail"}
        elseif state.active_tab == 4 then
            state.screen_stack = {"system_info"}
        end
        return
    elseif button == "r1" and action == "press" then
        state.active_tab = state.active_tab < #state.tab_labels and (state.active_tab + 1) or 1
        if state.active_tab == 1 then
            state.screen_stack = {"dashboard"}
        elseif state.active_tab == 2 then
            state.screen_stack = {"cpu_detail"}
        elseif state.active_tab == 3 then
            state.screen_stack = {"memory_detail"}
        elseif state.active_tab == 4 then
            state.screen_stack = {"system_info"}
        end
        return
    end

    if current == "dashboard" then
        if button == "dpad_up" then
            -- Move up in 2x2 grid
            if state.selected_card > 2 then
                state.selected_card = state.selected_card - 2
            end
        elseif button == "dpad_down" then
            if state.selected_card <= 2 then
                state.selected_card = state.selected_card + 2
            end
        elseif button == "dpad_left" then
            if state.selected_card % 2 == 0 then
                state.selected_card = state.selected_card - 1
            end
        elseif button == "dpad_right" then
            if state.selected_card % 2 == 1 then
                state.selected_card = state.selected_card + 1
            end
        elseif button == "a" then
            -- Drill into the selected metric
            local metric = METRICS_ORDER[state.selected_card]
            if metric == METRIC_CPU then
                state.active_tab = 2
                state.screen_stack[#state.screen_stack + 1] = "cpu_detail"
            elseif metric == METRIC_MEM then
                state.active_tab = 3
                state.screen_stack[#state.screen_stack + 1] = "memory_detail"
            elseif metric == METRIC_DISK or metric == METRIC_NET then
                state.active_tab = 4
                state.screen_stack[#state.screen_stack + 1] = "system_info"
            end
        end

    elseif current == "cpu_detail" or current == "memory_detail" or current == "system_info" then
        if button == "b" then
            if #state.screen_stack > 1 then
                state.screen_stack[#state.screen_stack] = nil
                state.active_tab = 1
            end
        end
    end
end

function on_update(dt)
    update_simulation(dt)

    -- Pulse animation
    state.pulse_t = state.pulse_t + dt

    -- Blink timer for LIVE indicator
    state.blink_timer = state.blink_timer + dt
    if state.blink_timer >= 0.8 then
        state.blink_timer = state.blink_timer - 0.8
        state.blink_on = not state.blink_on
    end
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    local current = state.screen_stack[#state.screen_stack]
    if current == "dashboard" then
        draw_dashboard_screen()
    elseif current == "cpu_detail" then
        draw_cpu_detail_screen()
    elseif current == "memory_detail" then
        draw_memory_detail_screen()
    elseif current == "system_info" then
        draw_system_info_screen()
    end
end
