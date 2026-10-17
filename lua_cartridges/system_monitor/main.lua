-- System Monitor for Cartridge OS (Lua)
-- Sampled CPU, memory, disk, and network monitoring.

local U = ui.public
local C = U.C
local USE_REAL_SYSINFO = (system ~= nil)
local SIMULATED = not USE_REAL_SYSINFO or system.is_simulated == true
local SOURCE_KNOWN = not USE_REAL_SYSINFO or type(system.is_simulated) == "boolean"

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
    [METRIC_CPU] = C.red,
    [METRIC_MEM] = C.teal,
    [METRIC_DISK] = C.gold,
    [METRIC_NET] = C.paper,
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
    hostname = "Not supplied",
    device_uptime = nil,
    mem_used_mb = 0,
    disk_used_gb = 0,
    session_seconds = 0,
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

    -- Per-core data exists only in the explicitly labelled local fallback.
    if not USE_REAL_SYSINFO then
        for c = 1, CPU_CORES do
            local base = 20 + prng_float() * 40
            sim.core_targets[c] = base
            sim.core_currents[c] = base
            sim.core_histories[c] = {}
        end
    end

    -- Histories start empty. A chart contains only samples taken this session;
    -- the system API can itself be simulated, so its values are never mixed
    -- with this app's local fallback generator.
end

local function update_simulation(dt, initial)
    sim.sample_timer = sim.sample_timer + dt
    if sim.sample_timer < UPDATE_INTERVAL then return false end
    sim.sample_timer = sim.sample_timer - UPDATE_INTERVAL
    sim.tick = sim.tick + 1
    local elapsed = initial and 0 or UPDATE_INTERVAL

    if USE_REAL_SYSINFO then
        -- Pull real values from the system API. The launcher's background
        -- poller updates these every ~2 seconds.
        sim.cpu_current = system.cpu_percent()
        push_history(sim.cpu_history, sim.cpu_current)
        -- The API exposes aggregate CPU only; no per-core values are invented.

        sim.mem_current = system.mem_percent()
        sim.mem_total_mb = system.mem_total_mb()
        push_history(sim.mem_history, sim.mem_current)

        local disk_total = system.disk_total_gb()
        local disk_used = system.disk_used_gb()
        if disk_total > 0 then
            sim.disk_current = (disk_used / disk_total) * 100
        else
            sim.disk_current = 0
        end
        sim.disk_total_gb = disk_total
        sim.disk_used_gb = disk_used
        push_history(sim.disk_history, sim.disk_current)

        sim.net_rx_rate = system.net_rx_rate()
        sim.net_tx_rate = system.net_tx_rate()
        sim.net_current = sim.net_rx_rate + sim.net_tx_rate
        push_history(sim.net_history, sim.net_current)
        sim.net_rx_total_kb = sim.net_rx_total_kb + sim.net_rx_rate * elapsed
        sim.net_tx_total_kb = sim.net_tx_total_kb + sim.net_tx_rate * elapsed
    else
        prng_next()
        local spike = prng_float()

        local cpu_vol = 4
        if spike < 0.03 then cpu_vol = 25 end
        sim.cpu_target = drift_target(sim.cpu_target, 5, 95, cpu_vol)
        sim.cpu_current = smooth(sim.cpu_current, sim.cpu_target, SMOOTH_FACTOR)
        push_history(sim.cpu_history, sim.cpu_current)

        for c = 1, CPU_CORES do
            local core_vol = 6
            if prng_float() < 0.05 then core_vol = 30 end
            sim.core_targets[c] = drift_target(sim.core_targets[c], 2, 98, core_vol)
            sim.core_currents[c] = smooth(sim.core_currents[c], sim.core_targets[c], SMOOTH_FACTOR * 1.2)
            push_history(sim.core_histories[c], sim.core_currents[c])
        end

        sim.mem_target = drift_target(sim.mem_target, 40, 85, 1.0)
        sim.mem_current = smooth(sim.mem_current, sim.mem_target, SMOOTH_FACTOR * 0.5)
        push_history(sim.mem_history, sim.mem_current)

        sim.disk_target = drift_target(sim.disk_target, 30, 80, 0.2)
        sim.disk_current = smooth(sim.disk_current, sim.disk_target, SMOOTH_FACTOR * 0.3)
        push_history(sim.disk_history, sim.disk_current)

        local net_vol = 20
        if prng_float() < 0.1 then
            sim.net_target = drift_target(sim.net_target, 200, 800, 100)
        elseif prng_float() < 0.15 then
            sim.net_target = drift_target(sim.net_target, 0, 50, 30)
        else
            sim.net_target = drift_target(sim.net_target, 10, 500, net_vol)
        end
        sim.net_current = smooth(sim.net_current, sim.net_target, SMOOTH_FACTOR)
        sim.net_current = math.max(0, sim.net_current)
        push_history(sim.net_history, sim.net_current)

        sim.net_rx_rate = sim.net_current * 0.65
        sim.net_tx_rate = sim.net_current * 0.35
        sim.net_rx_total_kb = sim.net_rx_total_kb + sim.net_rx_rate * elapsed
        sim.net_tx_total_kb = sim.net_tx_total_kb + sim.net_tx_rate * elapsed
    end

    sim.mem_used_mb = sim.mem_current / 100 * sim.mem_total_mb
    if USE_REAL_SYSINFO then
        if type(system.mem_used_mb) == "function" then sim.mem_used_mb = system.mem_used_mb() end
        if type(system.hostname) == "function" then
            local host = system.hostname()
            sim.hostname = host and host ~= "" and host or "Not supplied"
        end
        if type(system.uptime_secs) == "function" then sim.device_uptime = system.uptime_secs() end
    else
        sim.disk_used_gb = sim.disk_current / 100 * sim.disk_total_gb
    end
    -- All telemetry is cached in update, including the real Lua VM heap.
    sim.lua_mem_kb = collectgarbage("count")
    sim.session_seconds = os.time() - sim.uptime_start
    return true
end

-- ── State ────────────────────────────────────────────────────────────────────

local state = {
    screen_stack = {"dashboard"},
    -- Dashboard
    selected_card = 1,     -- 1-4 for the four metric cards
    -- Tab navigation (dashboard tabs via L1/R1)
    active_tab = 1,        -- 1=Dashboard, 2=CPU, 3=Memory, 4=System
    tab_labels = {"Dashboard", "CPU", "Memory", "System"},
}

-- ── Utility Functions ────────────────────────────────────────────────────────

local function status_color(pct)
    return pct < 50 and C.teal or (pct < 75 and C.gold or C.red)
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

-- ── Public layouts ───────────────────────────────────────────────────────────

local function draw_header(title)
    local source = SIMULATED and "SIMULATED DATA" or (SOURCE_KNOWN and "SYSTEM SNAPSHOT" or "UNVERIFIED FEED")
    U.header(title, "SYSTEM / RESOURCE MONITOR", source, SIMULATED and C.gold or C.muted)
    U.tabs(state.tab_labels, state.active_tab)
    local detail
    if not USE_REAL_SYSINFO then detail = "LOCAL SIMULATION / NOT DEVICE TELEMETRY"
    elseif SIMULATED then detail = "SYSTEM API / SIMULATED VALUES"
    elseif not SOURCE_KNOWN then detail = "SYSTEM API / SIMULATION STATUS UNKNOWN"
    else detail = "DEVICE TELEMETRY / CACHED SYSTEM API" end
    U.text(detail, 24, 166, 14, SIMULATED and C.gold or C.muted, 460)
    U.text("0.5S / " .. sim.tick .. " SAMPLES", 500, 166, 14, C.muted, 196)
end

local function bar(x, y, w, pct, color)
    U.rect(x, y, w, 6, C.line)
    U.rect(x, y, w * clamp(pct / 100, 0, 1), 6, color)
end

local function chart(history, x, y, w, h, color)
    if #history >= 2 then
        U.sparkline(history, x, y, w, h, color, C.line)
    else
        U.text("Collecting samples…", x, y + 10, 16, C.muted, w)
    end
end

-- A fixed percentage scale keeps flat/quiet samples at their true level.
-- Stepped segments use only measured samples and never insert scale anchors.
local function percent_chart(history, x, y, w, h, color, baseline)
    if #history < 2 then
        U.text("Collecting samples…", x, y + 10, 16, C.muted, w)
        return
    end
    U.rule(x, y + h, w, baseline or C.line)
    local step = w / (#history - 1)
    for i = 2, #history do
        local previous = y + h * (1 - clamp(history[i - 1], 0, 100) / 100)
        local current = y + h * (1 - clamp(history[i], 0, 100) / 100)
        local left = x + (i - 2) * step
        U.rule(left, previous, step, color)
        U.rect(left + step - 1, math.min(previous, current), 1, math.abs(current - previous) + 1, color)
    end
end

local function history_panel(history, y, h, label, color)
    U.panel(24, y, 672, h, C.panel)
    U.text(label, 40, y + 14, 14, C.muted, 320, true)
    if #history > 0 then
        local lo, hi = history[1], history[1]
        for _, value in ipairs(history) do lo = math.min(lo, value); hi = math.max(hi, value) end
        U.text(string.format("%.1f%% MIN / %.1f%% MAX", lo, hi), 436, y + 14, 14, C.paper, 244)
    end
    percent_chart(history, 40, y + 46, 640, h - 76, color)
    U.text("0–100% SCALE / LAST 40 SAMPLES", 40, y + h - 24, 14, C.muted, 640)
end

local function draw_metric_card(x, y, metric, selected)
    local value, history, detail
    if metric == METRIC_CPU then
        value, history, detail = sim.cpu_current, sim.cpu_history, "Aggregate / all processors"
    elseif metric == METRIC_MEM then
        value, history = sim.mem_current, sim.mem_history
        detail = string.format("%.0f / %.0f MB used", sim.mem_used_mb, sim.mem_total_mb)
    elseif metric == METRIC_DISK then
        value, history = sim.disk_current, sim.disk_history
        detail = string.format("%.1f / %.1f GB used", sim.disk_used_gb, sim.disk_total_gb)
    else
        value, history = sim.net_current, sim.net_history
        detail = string.format("RX %.0f / TX %.0f KB/s", sim.net_rx_rate, sim.net_tx_rate)
    end
    local fg, secondary = U.row(x, y, 328, 168, selected)
    local accent = METRIC_COLORS[metric]
    U.text(METRIC_LABELS[metric], x + 16, y + 14, 14, secondary, 230, true)
    U.display(string.format(metric == METRIC_NET and "%.0f" or "%.1f", value), x + 16, y + 36, 58, fg, 230)
    U.text(METRIC_UNITS[metric], x + 256, y + 66, 16, secondary, 56)
    U.text(detail, x + 16, y + 104, 14, secondary, 296)
    if #history >= 2 then
        if metric == METRIC_NET then
            U.sparkline(history, x + 16, y + 130, 296, 24, selected and C.red or accent, selected and C.muted or C.line)
        else
            percent_chart(history, x + 16, y + 130, 296, 24, selected and C.red or accent, selected and C.muted or C.line)
        end
    else
        U.rule(x + 16, y + 148, 296, selected and C.muted or C.line)
    end
end

local function draw_dashboard_screen()
    draw_header("SYSTEM MONITOR")
    for i, metric in ipairs(METRICS_ORDER) do
        local x = 24 + ((i - 1) % 2) * 344
        local y = 194 + math.floor((i - 1) / 2) * 184
        draw_metric_card(x, y, metric, state.selected_card == i)
    end
    U.panel(24, 562, 672, 88, C.panel)
    U.text("FREE MEMORY", 40, 576, 14, C.muted, 192, true)
    U.text(string.format("%.0f MB", sim.mem_total_mb - sim.mem_used_mb), 40, 602, 18, C.paper, 192)
    U.text("LUA VM HEAP", 264, 576, 14, C.muted, 192, true)
    U.text(format_bytes(sim.lua_mem_kb), 264, 602, 18, C.paper, 192)
    U.text("MONITOR SESSION", 488, 576, 14, C.muted, 192, true)
    U.text(format_uptime(sim.session_seconds), 488, 602, 18, C.paper, 192)
    U.footer({{"D-pad", "Metric"}, {"A", "Detail"}, {"L1/R1", "Tab"}})
end

local function draw_cpu_detail_screen()
    draw_header("CPU MONITOR")
    U.panel(24, 194, 672, 132, C.panel2, C.red)
    U.text("AGGREGATE CPU", 40, 208, 14, C.muted, 324, true)
    U.display(string.format("%.1f", sim.cpu_current), 40, 232, 64, C.paper, 260)
    U.text("%", 304, 260, 18, C.muted, 54)
    local load = sim.cpu_current < 20 and "LOW LOAD" or (sim.cpu_current < 50 and "NORMAL LOAD"
        or (sim.cpu_current < 75 and "MODERATE LOAD" or "HIGH LOAD"))
    U.text(load, 410, 214, 18, status_color(sim.cpu_current), 270, true)
    U.text("All processors combined", 410, 248, 16, C.paper, 270)
    U.text("Sampled every 0.5 seconds", 410, 276, 14, C.muted, 270)
    bar(40, 308, 640, sim.cpu_current, C.red)
    history_panel(sim.cpu_history, 342, 146, "CPU / SAMPLED HISTORY", C.red)
    U.text(USE_REAL_SYSINFO and "PER-CORE / NOT EXPOSED BY SYSTEM API" or "PER-CORE / LOCAL SIMULATION ONLY",
        24, 502, 14, C.muted, 672, true)
    if USE_REAL_SYSINFO then
        U.panel(24, 526, 672, 124, C.panel)
        U.display("AGGREGATE CPU ONLY", 40, 542, 32, C.paper, 640)
        U.text("Per-core measurements are unavailable from the system API.", 40, 588, 16, C.muted, 640)
        U.text("The chart above contains sampled aggregate CPU values only.", 40, 618, 14, C.muted, 640)
    else
        for c = 1, CPU_CORES do
            local x = 24 + (c - 1) * 172
            U.panel(x, 526, 156, 124, C.panel)
            U.text("CORE " .. (c - 1), x + 14, 540, 14, C.muted, 128, true)
            U.display(string.format("%.0f%%", sim.core_currents[c]), x + 14, 566, 32, C.paper, 128)
            percent_chart(sim.core_histories[c], x + 14, 606, 128, 28, C.red)
        end
    end
    U.footer({{"B", "Back"}, {"L1/R1", "Tab"}})
end

local function draw_memory_detail_screen()
    draw_header("MEMORY MONITOR")
    local free = sim.mem_total_mb - sim.mem_used_mb
    U.panel(24, 194, 672, 124, C.panel2, C.teal)
    U.text("PHYSICAL MEMORY", 40, 208, 14, C.muted, 324, true)
    U.display(string.format("%.1f", sim.mem_current), 40, 232, 56, C.paper, 260)
    U.text("%", 304, 258, 18, C.muted, 54)
    U.text(string.format("%.0f / %.0f MB used", sim.mem_used_mb, sim.mem_total_mb), 410, 224, 18, C.paper, 270, true)
    U.text(string.format("%.0f MB available", free), 410, 258, 16, C.muted, 270)
    bar(40, 298, 640, sim.mem_current, C.teal)
    history_panel(sim.mem_history, 334, 132, "MEMORY / SAMPLED HISTORY", C.teal)

    U.panel(24, 482, 440, 168, C.panel)
    U.text(USE_REAL_SYSINFO and "CATEGORIES / UNAVAILABLE" or "CATEGORIES / LOCAL SIMULATION", 40, 496, 14, C.muted, 408, true)
    local categories = {{"Applications", 0.55}, {"Cache", 0.25}, {"Buffers", 0.12}, {"Shared", 0.08}}
    for i, category in ipairs(categories) do
        local y = 526 + (i - 1) * 24
        U.text(category[1], 40, y, 16, C.paper, 226)
        U.text(USE_REAL_SYSINFO and "Not supplied" or string.format("%.0f MB", sim.mem_used_mb * category[2]),
            294, y, 16, C.muted, 154)
    end
    U.text(USE_REAL_SYSINFO and "The API reports aggregate memory only." or "Modeled categories / not device measurements.",
        40, 630, 14, C.muted, 408)
    U.panel(480, 482, 216, 78, C.panel)
    U.text("LUA VM / MEASURED", 496, 496, 14, C.muted, 184, true)
    U.display(string.format("%.0f", sim.lua_mem_kb), 496, 520, 28, C.paper, 128)
    U.text("KB", 640, 534, 14, C.muted, 40)
    U.panel(480, 576, 216, 74, C.panel)
    U.text("FREE RAM", 496, 590, 14, C.muted, 184, true)
    U.text(string.format("%.0f MB", free), 496, 616, 18, C.paper, 184, true)
    U.footer({{"B", "Back"}, {"L1/R1", "Tab"}})
end

local function draw_system_info_screen()
    draw_header("SYSTEM DETAILS")
    U.panel(24, 194, 672, 124, C.panel2, C.gold)
    U.text("STORAGE / USED", 40, 208, 14, C.muted, 310, true)
    U.display(string.format("%.1f%%", sim.disk_current), 40, 232, 56, C.paper, 320)
    U.text(string.format("%.1f / %.1f GB", sim.disk_used_gb, sim.disk_total_gb), 410, 224, 18, C.paper, 270, true)
    U.text(string.format("%.1f GB free", sim.disk_total_gb - sim.disk_used_gb), 410, 258, 16, C.muted, 270)
    bar(40, 298, 640, sim.disk_current, C.gold)

    U.panel(24, 334, 672, 144, C.panel)
    U.text("NETWORK / SESSION TRAFFIC", 40, 348, 14, C.muted, 640, true)
    chart(sim.net_history, 40, 386, 368, 72, C.teal)
    U.text(string.format("RX %.0f KB/s", sim.net_rx_rate), 440, 382, 18, C.paper, 240, true)
    U.text(string.format("TX %.0f KB/s", sim.net_tx_rate), 440, 412, 18, C.paper, 240, true)
    U.text("RECEIVED / " .. format_bytes(sim.net_rx_total_kb), 440, 440, 14, C.muted, 240)
    U.text("SENT / " .. format_bytes(sim.net_tx_total_kb), 440, 460, 14, C.muted, 240)

    U.panel(24, 494, 672, 156, C.panel)
    U.text("SOURCE INFORMATION", 40, 506, 14, C.muted, 640, true)
    local info = {
        {"Host", USE_REAL_SYSINFO and sim.hostname or "Local simulation fallback"},
        {"Device uptime", sim.device_uptime and format_uptime(math.floor(sim.device_uptime)) or "Not supplied"},
        {"Monitor session", format_uptime(sim.session_seconds)},
        {"Display", "720 x 720"},
        {"Samples", sim.tick .. " / every 0.5 seconds"},
    }
    for i, row in ipairs(info) do
        local y = 532 + (i - 1) * 24
        U.text(row[1], 40, y, 14, C.muted, 192)
        U.text(row[2], 264, y, 16, C.paper, 416)
    end
    U.footer({{"B", "Back"}, {"L1/R1", "Tab"}})
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    init_simulation()
    update_simulation(UPDATE_INTERVAL, true)
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local current = state.screen_stack[#state.screen_stack]
    local old_card = state.selected_card

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
        app.request_redraw()
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
        app.request_redraw()
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
    if old_card ~= state.selected_card or current ~= state.screen_stack[#state.screen_stack] then
        app.request_redraw()
    end
end

function on_update(dt)
    local dirty = update_simulation(dt)
    if dirty then app.request_redraw() end
    return dirty
end

function on_render()
    local current = state.screen_stack[#state.screen_stack]
    if current == "dashboard" then draw_dashboard_screen()
    elseif current == "cpu_detail" then draw_cpu_detail_screen()
    elseif current == "memory_detail" then draw_memory_detail_screen()
    elseif current == "system_info" then draw_system_info_screen() end
end
