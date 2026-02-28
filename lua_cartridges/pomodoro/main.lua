-- Pomodoro Timer for Cartridge OS (Lua)
-- Work/break cycles with statistics and settings

-- ── Constants ────────────────────────────────────────────────────────────────

local PHASE_WORK = "work"
local PHASE_SHORT_BREAK = "short_break"
local PHASE_LONG_BREAK = "long_break"

local PHASE_LABELS = {
    [PHASE_WORK] = "Work",
    [PHASE_SHORT_BREAK] = "Short Break",
    [PHASE_LONG_BREAK] = "Long Break",
}

local PHASE_COLORS = {
    [PHASE_WORK] = {220, 80, 80},
    [PHASE_SHORT_BREAK] = {80, 200, 120},
    [PHASE_LONG_BREAK] = {80, 140, 240},
}

local PHASE_COLORS_DIM = {
    [PHASE_WORK] = {110, 40, 40},
    [PHASE_SHORT_BREAK] = {40, 100, 60},
    [PHASE_LONG_BREAK] = {40, 70, 120},
}

local WORK_PRESETS = {15, 20, 25, 30, 45}
local SHORT_BREAK_PRESETS = {3, 5, 10}
local LONG_BREAK_PRESETS = {10, 15, 20, 30}

local SETTINGS = {
    {label="Work Duration", phase=PHASE_WORK, presets=WORK_PRESETS},
    {label="Short Break", phase=PHASE_SHORT_BREAK, presets=SHORT_BREAK_PRESETS},
    {label="Long Break", phase=PHASE_LONG_BREAK, presets=LONG_BREAK_PRESETS},
}

-- ── State ────────────────────────────────────────────────────────────────────

local state = {
    screen_stack = {"timer"},
    -- Timer engine
    phase = PHASE_WORK,
    running = false,
    work_count = 0,       -- 0-3 toward long break
    total_completed = 0,
    durations = {
        [PHASE_WORK] = 25 * 60,
        [PHASE_SHORT_BREAK] = 5 * 60,
        [PHASE_LONG_BREAK] = 15 * 60,
    },
    remaining = 25 * 60,
    total_focus_seconds = 0,
    sessions = {},
    session_start = nil,
    session_elapsed = 0,
    -- Animation
    pulse_t = 0,
    -- Stats screen
    stats_scroll = 0,
    stats_max_scroll = 0,
    -- Settings screen
    settings_row = 1,
    settings_indices = {3, 2, 1},  -- default: 25min work, 5min short, 10min long
}

-- ── Drawing Helpers ──────────────────────────────────────────────────────────

local function draw_header(title)
    screen.draw_gradient_rect(0, 0, 640, 40,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 640, 0, {color=theme.accent})
    screen.draw_text(title, 12, 10, {color=theme.text, size=20, bold=true})
end

local function draw_footer(hints)
    screen.draw_rect(0, 444, 640, 36, {color=theme.bg_header, filled=true})
    screen.draw_line(0, 444, 640, 444, {color=theme.border})
    local x = 10
    for _, h in ipairs(hints) do
        local w = screen.draw_button_hint(h[1], h[2], x, 452, {color=h[3], size=12})
        x = x + w + 14
    end
end

-- ── Timer Engine ─────────────────────────────────────────────────────────────

local function format_time()
    local total = math.max(0, math.floor(state.remaining))
    local minutes = math.floor(total / 60)
    local seconds = total % 60
    return string.format("%02d:%02d", minutes, seconds)
end

local function get_duration()
    return state.durations[state.phase]
end

local function get_progress()
    local d = get_duration()
    if d <= 0 then return 1.0 end
    local elapsed = d - state.remaining
    return math.max(0, math.min(1, elapsed / d))
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

local function advance_phase()
    if state.phase == PHASE_WORK then
        state.work_count = state.work_count + 1
        state.total_completed = state.total_completed + 1
        state.total_focus_seconds = state.total_focus_seconds + math.floor(state.session_elapsed)

        if state.session_start then
            state.sessions[#state.sessions + 1] = {
                start = state.session_start,
                duration_min = math.floor(state.durations[PHASE_WORK] / 60 * 10) / 10,
                completed = true,
            }
        end
        state.session_start = nil
        state.session_elapsed = 0

        if state.work_count >= 4 then
            state.phase = PHASE_LONG_BREAK
            state.work_count = 0
        else
            state.phase = PHASE_SHORT_BREAK
        end
    else
        state.phase = PHASE_WORK
    end

    state.remaining = state.durations[state.phase]
    state.running = false
    save_stats()
end

local function timer_toggle()
    if not state.running then
        if state.phase == PHASE_WORK and not state.session_start then
            state.session_start = os.date("%H:%M")
            state.session_elapsed = 0
        end
        state.running = true
    else
        state.running = false
    end
end

local function timer_reset()
    state.remaining = state.durations[state.phase]
    state.running = false
    if state.phase == PHASE_WORK then
        state.session_start = nil
        state.session_elapsed = 0
    end
end

local function timer_skip()
    if state.phase == PHASE_WORK and state.session_start then
        state.sessions[#state.sessions + 1] = {
            start = state.session_start,
            duration_min = math.floor(state.session_elapsed / 60 * 10) / 10,
            completed = false,
        }
        state.session_start = nil
        state.session_elapsed = 0
    end
    advance_phase()
end

local function set_duration(phase, minutes)
    state.durations[phase] = minutes * 60
    if state.phase == phase and not state.running then
        state.remaining = state.durations[phase]
    end
end

-- ── Timer Screen ─────────────────────────────────────────────────────────────

local function draw_timer_screen()
    draw_header("Pomodoro")

    local content_top = 40
    local content_bottom = 444
    local cx = 320
    local cy = (content_top + content_bottom) / 2 - 30

    local phase_color = PHASE_COLORS[state.phase]
    local phase_color_dim = PHASE_COLORS_DIM[state.phase]
    local track_color = {50, 50, 65}
    local progress = get_progress()

    -- Draw circular ring
    local radius = 130
    local ring_width = 12

    -- Track circle
    screen.draw_circle(cx, cy, radius, track_color[1], track_color[2], track_color[3])

    -- Inner circle to create ring effect
    screen.draw_circle(cx, cy, radius - ring_width, theme.bg.r, theme.bg.g, theme.bg.b)

    -- Progress arc (approximate with filled segments)
    if progress > 0 then
        local segments = math.floor(progress * 60)
        for s = 0, segments do
            local angle = -math.pi / 2 + (s / 60) * 2 * math.pi
            local px = cx + math.floor(radius * math.cos(angle))
            local py = cy + math.floor(radius * math.sin(angle))
            screen.draw_circle(px, py, ring_width / 2, phase_color[1], phase_color[2], phase_color[3])
        end
    end

    -- Inner glow
    screen.draw_circle(cx, cy, radius - ring_width - 2, phase_color_dim[1] / 2, phase_color_dim[2] / 2, phase_color_dim[3] / 2)
    screen.draw_circle(cx, cy, radius - ring_width - 3, theme.bg.r, theme.bg.g, theme.bg.b)

    -- Time display inside ring
    local time_str = format_time()
    local tw = screen.get_text_width(time_str, 52, true)
    screen.draw_text(time_str, cx - tw / 2, cy - 18, {color=theme.text, size=52, bold=true})

    -- Phase label
    local label = PHASE_LABELS[state.phase]
    local lw = screen.get_text_width(label, 18, false)
    screen.draw_text(label, cx - lw / 2, cy + 32, {color=phase_color, size=18})

    -- Running indicator
    if state.running then
        local pulse = (math.sin(state.pulse_t * 4) + 1) / 2
        local dot_radius = math.floor(4 + 2 * pulse)
        screen.draw_circle(cx, cy + 56, dot_radius, phase_color[1], phase_color[2], phase_color[3])
    else
        if state.remaining < get_duration() then
            local pw = screen.get_text_width("PAUSED", 13, false)
            screen.draw_text("PAUSED", cx - pw / 2, cy + 50, {color=theme.text_dim, size=13})
        end
    end

    -- Session dots (4 circles)
    local dot_y = cy + radius + 40
    local dot_radius_s = 8
    local spacing = 30
    local total_width = 3 * spacing
    local start_x = cx - total_width / 2
    for i = 0, 3 do
        local dx = start_x + i * spacing
        if i < state.work_count then
            screen.draw_circle(dx, dot_y, dot_radius_s, phase_color[1], phase_color[2], phase_color[3])
        else
            screen.draw_circle(dx, dot_y, dot_radius_s, theme.border.r, theme.border.g, theme.border.b)
            screen.draw_circle(dx, dot_y, dot_radius_s - 2, theme.bg.r, theme.bg.g, theme.bg.b)
        end
    end

    -- Completed count
    local count_text = state.total_completed .. " pomodoro" .. (state.total_completed ~= 1 and "s" or "") .. " today"
    local cw = screen.get_text_width(count_text, 14, false)
    screen.draw_text(count_text, cx - cw / 2, dot_y + 24, {color=theme.text_dim, size=14})

    -- Footer
    local hints = {}
    if state.running then
        hints[#hints + 1] = {"A", "Pause", theme.btn_a}
    else
        hints[#hints + 1] = {"A", "Start", theme.btn_a}
    end
    hints[#hints + 1] = {"X", "Reset", theme.btn_x}
    hints[#hints + 1] = {"Y", "Skip", theme.btn_y}
    draw_footer(hints)

    -- Right-side nav hints
    screen.draw_text("R1:Stats", 560, 454, {color=theme.text_dim, size=11})
    screen.draw_text("L1:Set", 500, 454, {color=theme.text_dim, size=11})
end

-- ── Stats Screen ─────────────────────────────────────────────────────────────

local function draw_stats_screen()
    draw_header("Statistics")

    local y = 52 - state.stats_scroll

    -- Summary cards
    local card_h = 80
    local card_gap = 12
    local card_w = 194
    local cards_x = 16

    -- Card 1: Completed
    screen.draw_card(cards_x, y, card_w, card_h, {bg=theme.card_bg, border=theme.border, radius=10, shadow=true})
    screen.draw_rect(cards_x + 10, y + 6, card_w - 20, 3, {color={220, 80, 80}, filled=true, radius=2})
    local val_str = tostring(state.total_completed)
    local vw = screen.get_text_width(val_str, 28, true)
    screen.draw_text(val_str, cards_x + card_w / 2 - vw / 2, y + 26, {color=theme.text, size=28, bold=true})
    local lbl = "Completed"
    local lw = screen.get_text_width(lbl, 13, false)
    screen.draw_text(lbl, cards_x + card_w / 2 - lw / 2, y + 58, {color=theme.text_dim, size=13})

    -- Card 2: Focus Time
    local focus_min = math.floor(state.total_focus_seconds / 60)
    local focus_str
    if focus_min >= 60 then
        focus_str = math.floor(focus_min / 60) .. "h " .. (focus_min % 60) .. "m"
    else
        focus_str = focus_min .. "m"
    end
    screen.draw_card(cards_x + card_w + card_gap, y, card_w, card_h, {bg=theme.card_bg, border=theme.border, radius=10, shadow=true})
    screen.draw_rect(cards_x + card_w + card_gap + 10, y + 6, card_w - 20, 3, {color={80, 200, 120}, filled=true, radius=2})
    vw = screen.get_text_width(focus_str, 28, true)
    screen.draw_text(focus_str, cards_x + card_w + card_gap + card_w / 2 - vw / 2, y + 26, {color=theme.text, size=28, bold=true})
    lbl = "Focus Time"
    lw = screen.get_text_width(lbl, 13, false)
    screen.draw_text(lbl, cards_x + card_w + card_gap + card_w / 2 - lw / 2, y + 58, {color=theme.text_dim, size=13})

    -- Card 3: Streak
    local streak_str = state.work_count .. "/4"
    screen.draw_card(cards_x + 2 * (card_w + card_gap), y, card_w, card_h, {bg=theme.card_bg, border=theme.border, radius=10, shadow=true})
    screen.draw_rect(cards_x + 2 * (card_w + card_gap) + 10, y + 6, card_w - 20, 3, {color={80, 140, 240}, filled=true, radius=2})
    vw = screen.get_text_width(streak_str, 28, true)
    screen.draw_text(streak_str, cards_x + 2 * (card_w + card_gap) + card_w / 2 - vw / 2, y + 26, {color=theme.text, size=28, bold=true})
    lbl = "Streak"
    lw = screen.get_text_width(lbl, 13, false)
    screen.draw_text(lbl, cards_x + 2 * (card_w + card_gap) + card_w / 2 - lw / 2, y + 58, {color=theme.text_dim, size=13})

    y = y + card_h + 20

    -- Session history header
    screen.draw_text("Session History", 20, y, {color=theme.text, size=16, bold=true})
    y = y + 28

    if #state.sessions == 0 then
        screen.draw_text("No sessions yet. Start a pomodoro!", 20, y, {color=theme.text_dim, size=14})
        y = y + 30
    else
        for i = #state.sessions, 1, -1 do
            if y > 440 then break end
            if y + 44 > 40 then
                local session = state.sessions[i]
                local idx = #state.sessions - i
                local h = 42

                -- Row background
                local bg = idx % 2 == 0 and theme.card_bg or theme.bg_lighter
                screen.draw_rounded_rect(16, y, 608, h, bg.r, bg.g, bg.b, 6, false)

                -- Status dot
                local completed = session.completed
                local dot_color = completed and {80, 200, 120} or {240, 80, 90}
                screen.draw_circle(16 + 18, y + h / 2, 5, dot_color[1], dot_color[2], dot_color[3])

                -- Start time
                screen.draw_text(session.start or "--:--", 50, y + 12, {color=theme.text, size=15, bold=true})

                -- Duration
                local dur = session.duration_min or 0
                local dur_str = dur == math.floor(dur) and string.format("%.0f min", dur) or string.format("%.1f min", dur)
                screen.draw_text(dur_str, 136, y + 13, {color=theme.text_dim, size=14})

                -- Status text
                local status = completed and "Completed" or "Skipped"
                local status_color = completed and {80, 200, 120} or theme.text_dim
                local sw = screen.get_text_width(status, 13, false)
                screen.draw_text(status, 608 - sw, y + 14, {color=status_color, size=13})
            end
            y = y + 50
        end
    end

    -- Calculate max scroll
    local total_content_height = (y + state.stats_scroll) - 52
    local visible_height = 444 - 52
    state.stats_max_scroll = math.max(0, total_content_height - visible_height)

    draw_footer({
        {"B", "Back", theme.btn_b},
        {"Y", "Reset", theme.btn_y},
    })

    if state.stats_max_scroll > 0 then
        screen.draw_text("D-Pad: Scroll", 540, 454, {color=theme.text_dim, size=12})
    end
end

-- ── Settings Screen ──────────────────────────────────────────────────────────

local function sync_settings_from_engine()
    for row_idx, setting in ipairs(SETTINGS) do
        local current_min = math.floor(state.durations[setting.phase] / 60)
        local closest = 1
        local closest_diff = math.abs(setting.presets[1] - current_min)
        for i, preset in ipairs(setting.presets) do
            local diff = math.abs(preset - current_min)
            if diff < closest_diff then
                closest = i
                closest_diff = diff
            end
        end
        state.settings_indices[row_idx] = closest
    end
end

local function draw_settings_screen()
    draw_header("Settings")

    local y = 60

    screen.draw_text("Timer Durations", 20, y, {color=theme.text, size=18, bold=true})
    y = y + 36

    screen.draw_text("Use LEFT/RIGHT to change, A to confirm", 20, y, {color=theme.text_dim, size=13})
    y = y + 32

    for row_idx, setting in ipairs(SETTINGS) do
        local selected = (row_idx == state.settings_row)
        local preset_idx = state.settings_indices[row_idx]
        local h = 64

        -- Row card
        local border_color = selected and theme.accent or theme.border
        local bg = selected and theme.card_highlight or theme.card_bg
        screen.draw_card(20, y, 600, h, {bg=bg, border=border_color, radius=10, shadow=true})

        -- Selection indicator
        if selected then
            screen.draw_rect(24, y + 14, 3, h - 28, {color=theme.accent, filled=true, radius=2})
        end

        -- Label
        screen.draw_text(setting.label, 38, y + 10, {color=theme.text, size=16, bold=true})

        -- Preset pills
        local pill_y = y + 36
        local pill_x = 38
        local phase_color = PHASE_COLORS[setting.phase]

        for i, preset in ipairs(setting.presets) do
            local is_active = (i == preset_idx)
            local text = preset .. " min"

            local pill_bg, pill_text_color
            if is_active then
                pill_bg = phase_color
                pill_text_color = {18, 18, 24}
            else
                pill_bg = theme.bg_lighter
                pill_text_color = theme.text_dim
            end

            local pw = screen.draw_pill(text, pill_x, pill_y,
                pill_bg[1] or pill_bg.r, pill_bg[2] or pill_bg.g, pill_bg[3] or pill_bg.b,
                {text_color=pill_text_color, size=13})
            pill_x = pill_x + pw + 8
        end

        -- Left/right arrows for selected row
        if selected then
            screen.draw_text("<", 570, y + 20, {color=theme.accent, size=20, bold=true})
            screen.draw_text(">", 598, y + 20, {color=theme.accent, size=20, bold=true})
        end

        y = y + 80
    end

    -- Cycle preview
    y = y + 20
    screen.draw_text("Cycle preview:", 20, y, {color=theme.text_dim, size=13})
    y = y + 22

    local work_min = math.floor(state.durations[PHASE_WORK] / 60)
    local short_min = math.floor(state.durations[PHASE_SHORT_BREAK] / 60)
    local long_min = math.floor(state.durations[PHASE_LONG_BREAK] / 60)

    local cycle = {
        {"W " .. work_min .. "m", PHASE_COLORS[PHASE_WORK]},
        {"S " .. short_min .. "m", PHASE_COLORS[PHASE_SHORT_BREAK]},
        {"W " .. work_min .. "m", PHASE_COLORS[PHASE_WORK]},
        {"S " .. short_min .. "m", PHASE_COLORS[PHASE_SHORT_BREAK]},
        {"W " .. work_min .. "m", PHASE_COLORS[PHASE_WORK]},
        {"S " .. short_min .. "m", PHASE_COLORS[PHASE_SHORT_BREAK]},
        {"W " .. work_min .. "m", PHASE_COLORS[PHASE_WORK]},
        {"L " .. long_min .. "m", PHASE_COLORS[PHASE_LONG_BREAK]},
    }

    local px = 20
    for _, item in ipairs(cycle) do
        local pw = screen.draw_pill(item[1], px, y,
            item[2][1], item[2][2], item[2][3], {text_color={18, 18, 24}, size=12})
        px = px + pw + 4
        if px > 600 then break end
    end

    draw_footer({
        {"B", "Back", theme.btn_b},
        {"A", "Select", theme.btn_a},
    })
    screen.draw_text("L1/R1 or LEFT/RIGHT: Change", 430, 454, {color=theme.text_dim, size=12})
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    -- Load saved stats
    local data = storage.load("stats")
    if data then
        local today = os.date("%Y-%m-%d")
        if data.today == today then
            state.total_completed = data.completed or 0
            state.total_focus_seconds = data.total_focus_seconds or 0
            state.sessions = data.sessions or {}
            state.work_count = data.work_count or 0
        end
    end
end

function on_input(button, action)
    if action ~= "press" then return end

    local current = state.screen_stack[#state.screen_stack]

    if current == "timer" then
        if button == "a" then
            timer_toggle()
        elseif button == "x" then
            timer_reset()
        elseif button == "y" then
            timer_skip()
        elseif button == "r1" then
            state.screen_stack[#state.screen_stack + 1] = "stats"
        elseif button == "l1" then
            state.screen_stack[#state.screen_stack + 1] = "settings"
            sync_settings_from_engine()
        end

    elseif current == "stats" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
        elseif button == "y" then
            -- Reset stats
            state.total_completed = 0
            state.total_focus_seconds = 0
            state.sessions = {}
            state.work_count = 0
            state.stats_scroll = 0
            save_stats()
        elseif button == "dpad_down" then
            state.stats_scroll = math.min(state.stats_scroll + 40, state.stats_max_scroll)
        elseif button == "dpad_up" then
            state.stats_scroll = math.max(state.stats_scroll - 40, 0)
        end

    elseif current == "settings" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
        elseif button == "dpad_down" then
            state.settings_row = math.min(state.settings_row + 1, #SETTINGS)
        elseif button == "dpad_up" then
            state.settings_row = math.max(state.settings_row - 1, 1)
        elseif button == "dpad_right" or button == "r1" then
            local setting = SETTINGS[state.settings_row]
            local idx = state.settings_indices[state.settings_row]
            idx = idx % #setting.presets + 1
            state.settings_indices[state.settings_row] = idx
            set_duration(setting.phase, setting.presets[idx])
        elseif button == "dpad_left" or button == "l1" then
            local setting = SETTINGS[state.settings_row]
            local idx = state.settings_indices[state.settings_row]
            idx = (idx - 2) % #setting.presets + 1
            state.settings_indices[state.settings_row] = idx
            set_duration(setting.phase, setting.presets[idx])
        elseif button == "a" then
            local setting = SETTINGS[state.settings_row]
            local idx = state.settings_indices[state.settings_row]
            set_duration(setting.phase, setting.presets[idx])
        end
    end
end

function on_update(dt)
    -- Timer ticks every frame
    if state.running then
        state.remaining = state.remaining - dt
        if state.phase == PHASE_WORK then
            state.session_elapsed = state.session_elapsed + dt
        end
        if state.remaining <= 0 then
            state.remaining = 0
            advance_phase()
        end
    end

    -- Pulse animation
    state.pulse_t = state.pulse_t + dt
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    local current = state.screen_stack[#state.screen_stack]
    if current == "timer" then
        draw_timer_screen()
    elseif current == "stats" then
        draw_stats_screen()
    elseif current == "settings" then
        draw_settings_screen()
    end
end
