-- Pomodoro Timer for Cartridge OS (Lua)
-- Quiet work/break instrument with compatible daily statistics.

local U = ui.public
local C = U.C
local PHASE_WORK = "work"
local PHASE_SHORT_BREAK = "short_break"
local PHASE_LONG_BREAK = "long_break"
local PHASE_LABELS = {[PHASE_WORK] = "Focus", [PHASE_SHORT_BREAK] = "Short rest", [PHASE_LONG_BREAK] = "Long rest"}
local PHASE_COLORS = {[PHASE_WORK] = C.red, [PHASE_SHORT_BREAK] = C.teal, [PHASE_LONG_BREAK] = C.gold}
local SETTINGS = {
    {label = "Focus duration", phase = PHASE_WORK, presets = {15, 20, 25, 30, 45}},
    {label = "Short rest", phase = PHASE_SHORT_BREAK, presets = {3, 5, 10}},
    {label = "Long rest", phase = PHASE_LONG_BREAK, presets = {10, 15, 20, 30}},
}
local VISIBLE_SESSIONS = 5
local state = {
    screen_stack = {"timer"},
    phase = PHASE_WORK,
    running = false,
    work_count = 0,
    total_completed = 0,
    durations = {[PHASE_WORK] = 25 * 60, [PHASE_SHORT_BREAK] = 5 * 60, [PHASE_LONG_BREAK] = 15 * 60},
    phase_duration = 25 * 60, -- A running phase retains the duration it started with.
    remaining = 25 * 60,
    total_focus_seconds = 0,
    sessions = {},
    session_start = nil,
    session_elapsed = 0,
    stats_scroll = 0, -- Rows, newest first.
    settings_row = 1,
    settings_indices = {3, 2, 2},
}

local function current_screen()
    return state.screen_stack[#state.screen_stack]
end

local function format_time()
    local total = math.max(0, math.ceil(state.remaining))
    return string.format("%02d:%02d", math.floor(total / 60), total % 60)
end

local function focus_time()
    local minutes = math.floor(state.total_focus_seconds / 60)
    if minutes >= 60 then return math.floor(minutes / 60) .. "h " .. (minutes % 60) .. "m" end
    return minutes .. "m"
end

local function timer_status()
    if state.running then return "RUNNING" end
    return state.remaining < state.phase_duration and "PAUSED" or "READY"
end

local function save_stats()
    storage.save("stats", {
        today = os.date("%Y-%m-%d"),
        completed = state.total_completed,
        total_focus_seconds = state.total_focus_seconds,
        sessions = state.sessions,
        work_count = state.work_count,
    })
end

local function advance_phase(completed)
    if state.phase == PHASE_WORK then
        if completed then
            state.work_count = state.work_count + 1
            state.total_completed = state.total_completed + 1
            state.total_focus_seconds = state.total_focus_seconds + math.floor(state.session_elapsed)
        end
        if state.session_start then
            state.sessions[#state.sessions + 1] = {
                start = state.session_start,
                duration_min = math.floor(state.session_elapsed / 60 * 10) / 10,
                completed = completed,
            }
        end
        state.session_start, state.session_elapsed = nil, 0
        if state.work_count >= 4 then
            state.phase, state.work_count = PHASE_LONG_BREAK, 0
        else
            state.phase = PHASE_SHORT_BREAK
        end
    else
        state.phase = PHASE_WORK
    end
    state.phase_duration = state.durations[state.phase]
    state.remaining, state.running = state.phase_duration, false
    save_stats()
    if audio then
        local frequency = state.phase == PHASE_WORK and 880 or 660
        audio.beep(frequency, 200)
        audio.beep(frequency, 200)
        audio.beep(frequency, 400)
    end
end

local function timer_toggle()
    if not state.running and state.phase == PHASE_WORK and not state.session_start then
        state.session_start, state.session_elapsed = os.date("%H:%M"), 0
    end
    state.running = not state.running
end

local function timer_reset()
    state.phase_duration = state.durations[state.phase]
    state.remaining, state.running = state.phase_duration, false
    if state.phase == PHASE_WORK then state.session_start, state.session_elapsed = nil, 0 end
end

local function set_duration(phase, minutes)
    local duration = minutes * 60
    if state.durations[phase] == duration then return end
    state.durations[phase] = duration
    if state.phase == phase and not state.running then timer_reset() end
end

local function sync_settings_from_engine()
    for row, setting in ipairs(SETTINGS) do
        local minutes = math.floor(state.durations[setting.phase] / 60)
        local closest, difference = 1, math.huge
        for i, preset in ipairs(setting.presets) do
            if math.abs(preset - minutes) < difference then closest, difference = i, math.abs(preset - minutes) end
        end
        state.settings_indices[row] = closest
    end
end

local function draw_timer_screen()
    U.header("Pomodoro", "TIME / FOCUS INSTRUMENT", timer_status(), state.running and C.red or C.muted)
    U.tabs({"Timer", "Statistics", "Settings"}, 1)
    local color = PHASE_COLORS[state.phase]
    U.panel(24, 164, 672, 300, C.paper, color)
    U.display(PHASE_LABELS[state.phase]:upper(), 44, 178, 32, C.black, 420)
    U.text(math.floor(state.phase_duration / 60) .. " MINUTE PHASE", 44, 216, 14, C.black, 420, true)
    U.text(timer_status(), 552, 190, 14, C.black, 124, true)
    U.display(format_time(), 44, 236, 132, C.black, 632)
    local action = state.running and "A pauses the timer. Stay with the task."
        or (timer_status() == "PAUSED" and "A resumes this phase. X starts it over."
        or "A starts this phase. Each phase waits for you.")
    U.text(action, 44, 418, 18, C.black, 632)
    U.rect(44, 449, 632, 6, C.quiet)
    local progress = math.max(0, math.min(1, (state.phase_duration - state.remaining) / state.phase_duration))
    U.rect(44, 449, math.floor(632 * progress), 6, color)

    U.text("ROUND / FOUR FOCUS SESSIONS", 24, 482, 14, C.muted, 444, true)
    U.text("LONG REST AFTER 4", 504, 482, 14, C.muted, 192)
    for i = 1, 4 do
        local completed = state.phase == PHASE_LONG_BREAK or i <= state.work_count
        local current = state.phase == PHASE_WORK and i == state.work_count + 1
        local x = 24 + (i - 1) * 172
        local fg, secondary = U.row(x, 508, 156, 68, current)
        U.display(string.format("%02d", i), x + 14, 515, 28, fg, 60)
        U.text(completed and "DONE" or (current and "FOCUS" or "NEXT"), x + 14, 550, 14,
            completed and C.teal or secondary, 128, true)
        if completed then U.rect(x, 572, 156, 4, C.teal) end
    end
    U.panel(24, 594, 328, 56, C.panel)
    U.text("COMPLETED TODAY", 40, 613, 14, C.muted, 188, true)
    U.display(tostring(state.total_completed), 244, 601, 32, C.paper, 92)
    U.panel(368, 594, 328, 56, C.panel)
    U.text("COMPLETED FOCUS", 384, 613, 14, C.muted, 160, true)
    U.display(focus_time(), 554, 601, 32, C.paper, 126)
    U.footer({{"A", state.running and "Pause" or "Start"}, {"X", "Reset"}, {"Y", "Skip"}, {"R1", "Stats"}, {"L1", "Settings"}})
end

local function draw_stats_screen()
    U.header("Pomodoro", "TIME / DAILY RECORD", "TODAY", C.muted)
    U.tabs({"Timer", "Statistics", "Settings"}, 2)
    local values = {tostring(state.total_completed), focus_time(), state.work_count .. " / 4"}
    local labels = {"COMPLETED", "COMPLETED FOCUS", "CURRENT ROUND"}
    local colors = {C.red, C.teal, C.gold}
    for i = 1, 3 do
        local x = 24 + (i - 1) * 228
        U.panel(x, 164, 216, 104, C.panel, colors[i])
        U.text(labels[i], x + 16, 180, 14, C.muted, 184, true)
        U.display(values[i], x + 16, 204, 40, C.paper, 184)
    end
    U.text("SESSION REGISTER / NEWEST FIRST", 24, 286, 14, C.muted, 460, true)
    U.text(#state.sessions .. " RECORDED", 528, 286, 14, C.muted, 168)
    if #state.sessions == 0 then
        U.empty("No focus sessions yet", "B returns to the timer. Press A to begin. Completed and skipped sessions appear here.", 326)
    else
        state.stats_scroll = math.min(state.stats_scroll, math.max(0, #state.sessions - VISIBLE_SESSIONS))
        for row = 0, math.min(VISIBLE_SESSIONS, #state.sessions - state.stats_scroll) - 1 do
            local session = state.sessions[#state.sessions - state.stats_scroll - row]
            local y = 310 + row * 64
            U.panel(24, y, 672, 58, C.panel, session.completed and C.teal or C.quiet)
            U.text(session.start or "--:--", 44, y + 18, 18, C.paper, 94, true)
            local duration = session.duration_min or 0
            local minutes = duration == math.floor(duration) and string.format("%.0f min", duration) or string.format("%.1f min", duration)
            U.text(minutes, 176, y + 18, 18, C.paper, 320)
            U.text(session.completed and "COMPLETED" or "SKIPPED", 544, y + 21, 14,
                session.completed and C.teal or C.muted, 132, true)
        end
        U.text((state.stats_scroll + 1) .. "-" .. math.min(#state.sessions, state.stats_scroll + VISIBLE_SESSIONS)
            .. " / " .. #state.sessions .. " sessions", 24, 634, 14, C.muted, 672)
    end
    U.footer({{"B", "Back"}, {"Y", "Clear stats"}, {"D-PAD", "Scroll"}})
end

local function draw_settings_screen()
    U.header("Pomodoro", "TIME / CONFIGURE THE ROUND", state.running and "TIMER RUNNING" or "DURATIONS", C.muted)
    U.tabs({"Timer", "Statistics", "Settings"}, 3)
    U.text("Set a pace you can keep.", 24, 160, 18, C.paper, 672, true)
    U.text("Changes reset a paused phase; a running phase keeps its current duration.", 24, 190, 14, C.muted, 672)
    for row, setting in ipairs(SETTINGS) do
        local y = 216 + (row - 1) * 112
        local selected = row == state.settings_row
        local fg, secondary = U.row(24, y, 672, 96, selected)
        local color = PHASE_COLORS[setting.phase]
        U.text(setting.label, 44, y + 12, 18, fg, 420, true)
        U.text(math.floor(state.durations[setting.phase] / 60) .. " MIN", 552, y + 14, 16, secondary, 124, true)
        local count = #setting.presets
        local width = (632 - (count - 1) * 8) / count
        for i, preset in ipairs(setting.presets) do
            local x = 44 + (i - 1) * (width + 8)
            local active = i == state.settings_indices[row]
            U.rect(x, y + 48, width, 32, active and color or C.panel2)
            U.text(preset .. " min", x + 12, y + 55, 14, active and C.black or C.paper, width - 24, true)
        end
    end
    local work = math.floor(state.durations[PHASE_WORK] / 60)
    local short = math.floor(state.durations[PHASE_SHORT_BREAK] / 60)
    local long = math.floor(state.durations[PHASE_LONG_BREAK] / 60)
    U.text("FULL ROUND / FOUR FOCUS PHASES", 24, 556, 14, C.muted, 444, true)
    U.text((4 * work + 3 * short + long) .. " MIN TOTAL", 520, 556, 14, C.muted, 176)
    for i = 1, 8 do
        local phase = i % 2 == 1 and PHASE_WORK or (i == 8 and PHASE_LONG_BREAK or PHASE_SHORT_BREAK)
        local x = 24 + (i - 1) * 85
        local minutes = phase == PHASE_WORK and work or (phase == PHASE_LONG_BREAK and long or short)
        U.panel(x, 582, 77, 68, C.panel)
        U.text(phase == PHASE_WORK and "FOCUS" or (phase == PHASE_LONG_BREAK and "LONG" or "REST"), x + 8, 593, 14, C.paper, 61, true)
        U.text(minutes .. "m", x + 8, 623, 14, C.muted, 61)
        U.rect(x, 647, 77, 3, PHASE_COLORS[phase])
    end
    U.footer({{"B", "Back"}, {"A", "Apply"}, {"D-PAD", "Adjust"}, {"L1/R1", "Preset"}})
end

function on_init()
    local data = storage.load("stats")
    if data and data.today == os.date("%Y-%m-%d") then
        state.total_completed = data.completed or 0
        state.total_focus_seconds = data.total_focus_seconds or 0
        state.sessions = data.sessions or {}
        state.work_count = data.work_count or 0
    end
end

local function handle_input(button, action)
    if action ~= "press" then return end
    local current = current_screen()
    if current == "timer" then
        if button == "a" then timer_toggle()
        elseif button == "x" then timer_reset()
        elseif button == "y" then advance_phase(false)
        elseif button == "r1" then state.screen_stack[#state.screen_stack + 1] = "stats"
        elseif button == "l1" then
            state.screen_stack[#state.screen_stack + 1] = "settings"
            sync_settings_from_engine()
        end
    elseif current == "stats" then
        if button == "b" then state.screen_stack[#state.screen_stack] = nil
        elseif button == "y" then
            state.total_completed, state.total_focus_seconds, state.work_count = 0, 0, 0
            state.sessions, state.stats_scroll = {}, 0
            save_stats()
        elseif button == "dpad_down" then state.stats_scroll = math.min(state.stats_scroll + 1, math.max(0, #state.sessions - VISIBLE_SESSIONS))
        elseif button == "dpad_up" then state.stats_scroll = math.max(state.stats_scroll - 1, 0) end
    elseif current == "settings" then
        if button == "b" then state.screen_stack[#state.screen_stack] = nil
        elseif button == "dpad_down" then state.settings_row = math.min(state.settings_row + 1, #SETTINGS)
        elseif button == "dpad_up" then state.settings_row = math.max(state.settings_row - 1, 1)
        elseif button == "dpad_right" or button == "r1" or button == "dpad_left" or button == "l1" then
            local setting = SETTINGS[state.settings_row]
            local delta = (button == "dpad_right" or button == "r1") and 1 or -1
            local index = (state.settings_indices[state.settings_row] - 1 + delta) % #setting.presets + 1
            state.settings_indices[state.settings_row] = index
            set_duration(setting.phase, setting.presets[index])
        elseif button == "a" then
            local setting = SETTINGS[state.settings_row]
            set_duration(setting.phase, setting.presets[state.settings_indices[state.settings_row]])
        end
    end
end

local function input_snapshot()
    return table.concat({current_screen(), state.phase, tostring(state.running), state.remaining,
        state.total_completed, state.total_focus_seconds, state.work_count, #state.sessions,
        state.stats_scroll, state.settings_row, state.durations[PHASE_WORK],
        state.durations[PHASE_SHORT_BREAK], state.durations[PHASE_LONG_BREAK]}, "\0")
end

function on_input(button, action)
    local before = input_snapshot()
    handle_input(button, action)
    if before ~= input_snapshot() then app.request_redraw() end
end

function on_update(dt)
    if not state.running then return false end
    local before = math.ceil(state.remaining)
    local elapsed = math.min(state.remaining, math.max(0, dt))
    state.remaining = math.max(0, state.remaining - elapsed)
    if state.phase == PHASE_WORK then state.session_elapsed = state.session_elapsed + elapsed end
    if state.remaining <= 0 then
        -- Accumulating fractional deltas can finish a few floating-point bits
        -- below the full duration. Completed work always accounts for the phase.
        if state.phase == PHASE_WORK then state.session_elapsed = state.phase_duration end
        advance_phase(true)
        return true
    end
    -- Static statistics/settings do not redraw for hidden timer ticks.
    return current_screen() == "timer" and math.ceil(state.remaining) ~= before
end

function on_render()
    local current = current_screen()
    if current == "timer" then draw_timer_screen()
    elseif current == "stats" then draw_stats_screen()
    elseif current == "settings" then draw_settings_screen() end
end
