-- VibeBoy Client for Cartridge OS (Lua)
-- Monitor and control tmux sessions with AI-powered response suggestions

-- ── Constants ──────────────────────────────────────────────────────────────

local DEFAULT_HOST = "127.0.0.1"
local DEFAULT_PORT = 8766

local TYPE_ICONS = {
    claude_code = "CC", interactive_prompt = "??", idle_shell = "$$",
    running_process = ">>", stale = "zz", unknown = "..",
}
local STATUS_ICONS = {
    waiting = "WAIT", thinking = "WORK", idle = "IDLE",
    running = "RUN", stale = "DEAD", error = "ERR!",
}
local CATEGORY_LABELS = {
    approve = "ok", approve_plus = "o+", redirect = "<>", clarify = "??",
    extend = "++", pivot = ">>", abort = "XX", custom = "..",
}
local PERM_DISPLAY = { bypass = "BYPASS", plan = "PLAN", normal = "PERMS" }

-- Layout (640x480)
local HEADER_H = 32
local STATUS_H = 22
local FOOTER_Y = 444
local FOOTER_H = 36
local OPTIONS_H = 108
local TERMINAL_TOP = HEADER_H + STATUS_H  -- 54
local OPTIONS_TOP = FOOTER_Y - OPTIONS_H  -- 336
local TERMINAL_H = OPTIONS_TOP - TERMINAL_TOP  -- 282
local VISIBLE_OPTIONS = 3
local SCROLL_STEP = 3
local MONO_SIZE = 13

-- ── State ──────────────────────────────────────────────────────────────────

local state = {
    screen_stack = {"connect"},
    connected = false,
    sessions = {},
    session_order = {},
    current_session = nil,
    current_index = 0,
    option_cursor = 0,
    terminal_scroll = 0,
    dashboard_cursor = 1,
    poll_timer = 0,
    poll_interval = 1.5,
    elapsed = 0,
    daemon_host = DEFAULT_HOST,
    daemon_port = DEFAULT_PORT,
}

-- ── Utility Functions ──────────────────────────────────────────────────────

local function strip_ansi(text)
    if not text then return "" end
    return text:gsub("\27%[[\48-\57;]*[\64-\126]", ""):gsub("[\0-\8\11\12\14-\31]", "")
end

local function truncate_text(text, max_w, size, bold)
    if not text or text == "" then return "" end
    local tw = screen.get_text_width(text, size, bold or false)
    if tw <= max_w then return text end
    while #text > 1 and screen.get_text_width(text .. "..", size, bold or false) > max_w do
        text = text:sub(1, -2)
    end
    return text .. ".."
end

local function category_color(cat)
    if cat == "approve" or cat == "approve_plus" then
        return theme.positive
    elseif cat == "redirect" or cat == "pivot" then
        return theme.accent
    elseif cat == "clarify" then
        return {255, 180, 60}
    elseif cat == "extend" then
        return theme.text_accent
    elseif cat == "abort" then
        return theme.negative
    elseif cat == "custom" then
        return theme.text_dim
    else
        return theme.text_dim
    end
end

local function status_color(status)
    if status == "waiting" then
        return {255, 180, 60}
    elseif status == "thinking" then
        return theme.accent
    elseif status == "idle" then
        return theme.text_dim
    elseif status == "running" then
        return theme.positive
    elseif status == "error" then
        return theme.negative
    elseif status == "stale" then
        return theme.negative
    else
        return theme.text_dim
    end
end

local function sort_sessions(sessions)
    local order = {}
    for id, _ in pairs(sessions) do
        order[#order + 1] = id
    end
    table.sort(order, function(a, b)
        local sa = sessions[a]
        local sb = sessions[b]
        local a_waiting = (sa.status == "waiting") and 1 or 0
        local b_waiting = (sb.status == "waiting") and 1 or 0
        if a_waiting ~= b_waiting then
            return a_waiting > b_waiting
        end
        return a < b
    end)
    return order
end

local function get_api_url()
    return "http://" .. state.daemon_host .. ":" .. state.daemon_port
end

local function send_action(action, payload)
    local body = {
        action = action,
        session_id = state.current_session or "",
        payload = payload or {},
    }
    local encoded = json.encode(body)
    http.post(get_api_url() .. "/api/action", encoded)
end

-- ── Drawing Helpers ────────────────────────────────────────────────────────

local function draw_header(title, right_text)
    screen.draw_gradient_rect(0, 0, 640, HEADER_H,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 640, 0, {color=theme.accent})
    screen.draw_text(title, 12, 7, {color=theme.text, size=18, bold=true})
    if right_text then
        local rw = screen.get_text_width(right_text, 12, false)
        screen.draw_text(right_text, 624 - rw, 11, {color=theme.text_dim, size=12})
    end
end

local function draw_footer(hints)
    screen.draw_rect(0, FOOTER_Y, 640, FOOTER_H, {color=theme.bg_header, filled=true})
    screen.draw_line(0, FOOTER_Y, 640, FOOTER_Y, {color=theme.border})
    local x = 10
    for _, h in ipairs(hints) do
        local w = screen.draw_button_hint(h[1], h[2], x, FOOTER_Y + 10, {color=h[3], size=12})
        x = x + w + 14
    end
end

local function draw_scroll_indicator(y_start, height, cursor, total, visible)
    if total <= visible then return end
    local ind_x = 635
    local bar_top = y_start + 4
    local bar_h = height - 8
    screen.draw_line(ind_x, bar_top, ind_x, bar_top + bar_h, {color=theme.border})
    local thumb_h = math.max(8, math.floor(bar_h * visible / total))
    local progress = (cursor - 1) / math.max(1, total - 1)
    local thumb_y = bar_top + math.floor((bar_h - thumb_h) * progress)
    screen.draw_rect(ind_x - 1, thumb_y, 3, thumb_h, {color=theme.text_dim, filled=true, radius=1})
end

-- ── Screen 1: Connect Screen ──────────────────────────────────────────────

local function draw_connect_screen()
    -- Centered layout
    local dots_count = math.floor(state.elapsed * 2) % 4
    local dots = string.rep(".", dots_count)

    -- Title
    local title = "VIBEBOY"
    local tw = screen.get_text_width(title, 36, true)
    screen.draw_text(title, (640 - tw) / 2, 160, {color=theme.accent, size=36, bold=true})

    -- Subtitle with animated dots
    local sub = "Connecting to daemon" .. dots
    local sw = screen.get_text_width(sub, 16, false)
    screen.draw_text(sub, (640 - sw) / 2, 210, {color=theme.text_dim, size=16})

    -- Animated pulse bar
    local pulse_w = 200
    local pulse_x = (640 - pulse_w) / 2
    local pulse_y = 250
    local fill = (math.sin(state.elapsed * 3) + 1) / 2
    screen.draw_rect(pulse_x, pulse_y, pulse_w, 4, {color=theme.bg_lighter, filled=true, radius=2})
    local bar_w = math.floor(pulse_w * fill)
    if bar_w > 0 then
        screen.draw_rect(pulse_x, pulse_y, bar_w, 4, {color=theme.accent, filled=true, radius=2})
    end

    -- Host info at bottom
    local host_str = state.daemon_host .. ":" .. state.daemon_port
    local hw = screen.get_text_width(host_str, 12, false)
    screen.draw_text(host_str, (640 - hw) / 2, 400, {color=theme.text_dim, size=12})
end

-- ── Screen 2: Dashboard Screen ────────────────────────────────────────────

local function draw_dashboard_screen()
    local n = #state.session_order

    draw_header("VIBEBOY", "[" .. n .. " sessions]")

    local content_y = HEADER_H + 2
    local content_h = FOOTER_Y - content_y
    local row_h = 52

    if n == 0 then
        local msg = "No active sessions"
        local mw = screen.get_text_width(msg, 16, false)
        screen.draw_text(msg, (640 - mw) / 2, content_y + content_h / 2 - 8, {color=theme.text_dim, size=16})
    else
        local visible = math.max(1, math.floor(content_h / row_h))
        state.dashboard_cursor = math.max(1, math.min(state.dashboard_cursor, n))

        local start_idx
        if n <= visible then
            start_idx = 1
        else
            start_idx = math.max(1, math.min(state.dashboard_cursor - 1, n - visible + 1))
        end
        local end_idx = math.min(start_idx + visible - 1, n)

        local y = content_y
        for idx = start_idx, end_idx do
            local session_id = state.session_order[idx]
            local session = state.sessions[session_id]
            local is_selected = (idx == state.dashboard_cursor)

            -- Row background
            if is_selected then
                screen.draw_rect(0, y, 630, row_h, {color=theme.bg_selected, filled=true})
            end

            -- Cursor indicator
            if is_selected then
                screen.draw_text(">", 6, y + 8, {color=theme.accent, size=16, bold=true})
            end

            -- Type icon
            local session_type = session.session_type or session.type or "unknown"
            local type_icon = TYPE_ICONS[session_type] or TYPE_ICONS.unknown
            screen.draw_text(type_icon, 24, y + 8, {color=theme.text_dim, size=14, bold=true})

            -- Session name (bold)
            local name = session_id
            local name_trunc = truncate_text(name, 380, 14, true)
            screen.draw_text(name_trunc, 58, y + 6, {color=theme.text, size=14, bold=true})

            -- Status badge right-aligned
            local sess_status = session.status or "unknown"
            local badge_text = STATUS_ICONS[sess_status] or sess_status
            local badge_color = status_color(sess_status)
            local bw = screen.get_text_width(badge_text, 11, true)
            screen.draw_rect(620 - bw - 12, y + 6, bw + 10, 18, {color=badge_color, filled=true, radius=4})
            screen.draw_text(badge_text, 620 - bw - 7, y + 8, {color={20, 20, 30}, size=11, bold=true})

            -- Pane command on second line (dim)
            local pane_cmd = session.pane_command or session.command or ""
            if pane_cmd ~= "" then
                local cmd_trunc = truncate_text(strip_ansi(pane_cmd), 500, 11, false)
                screen.draw_text(cmd_trunc, 58, y + 28, {color=theme.text_dim, size=11})
            end

            -- Separator line
            screen.draw_line(10, y + row_h - 1, 630, y + row_h - 1, {color=theme.border})

            y = y + row_h
        end

        draw_scroll_indicator(content_y, content_h, state.dashboard_cursor, n, visible)
    end

    draw_footer({
        {"UD", "Move", theme.btn_l},
        {"A", "Select", theme.btn_a},
        {"B", "Back", theme.btn_b},
    })
end

-- ── Screen 3: Session Screen ──────────────────────────────────────────────

local function get_current_session()
    if not state.current_session then return nil end
    return state.sessions[state.current_session]
end

local function draw_status_bar(session)
    local y = HEADER_H
    screen.draw_rect(0, y, 640, STATUS_H, {color=theme.bg_lighter, filled=true})
    screen.draw_line(0, y + STATUS_H, 640, y + STATUS_H, {color=theme.border})

    local x = 8

    -- Type icon
    local session_type = session.session_type or session.type or "unknown"
    local type_icon = TYPE_ICONS[session_type] or TYPE_ICONS.unknown
    screen.draw_text(type_icon, x, y + 4, {color=theme.text_dim, size=12, bold=true})
    x = x + screen.get_text_width(type_icon, 12, true) + 6

    -- Status badge
    local sess_status = session.status or "unknown"
    local badge_text = STATUS_ICONS[sess_status] or sess_status
    local badge_color = status_color(sess_status)
    local badge_w = screen.get_text_width(badge_text, 10, true)
    screen.draw_rect(x, y + 3, badge_w + 8, 16, {color=badge_color, filled=true, radius=3})
    screen.draw_text(badge_text, x + 4, y + 4, {color={20, 20, 30}, size=10, bold=true})
    x = x + badge_w + 14

    -- Separator
    screen.draw_text("|", x, y + 4, {color=theme.border, size=12})
    x = x + 10

    -- Session name
    local name = state.current_session or ""
    local name_max_w = 340
    local name_trunc = truncate_text(name, name_max_w, 12, false)
    screen.draw_text(name_trunc, x, y + 5, {color=theme.text, size=12})

    -- Permission mode badge (right side)
    local perm_mode = session.permission_mode
    if perm_mode and PERM_DISPLAY[perm_mode] then
        local perm_text = PERM_DISPLAY[perm_mode]
        local pw = screen.get_text_width(perm_text, 10, true)
        local perm_color
        if perm_mode == "bypass" then
            perm_color = theme.negative
        elseif perm_mode == "plan" then
            perm_color = theme.accent
        else
            perm_color = {80, 140, 255}
        end
        screen.draw_rect(624 - pw - 8, y + 3, pw + 8, 16, {color=perm_color, filled=true, radius=3})
        screen.draw_text(perm_text, 624 - pw - 4, y + 4, {color={20, 20, 30}, size=10, bold=true})
    end
end

local function draw_terminal_view(session)
    local raw = session.screen_content or {}
    local lines = {}

    -- screen_content is an array of strings from the daemon
    if type(raw) == "table" then
        for i = 1, #raw do
            lines[#lines + 1] = strip_ansi(tostring(raw[i] or ""))
        end
    elseif type(raw) == "string" and raw ~= "" then
        for line in (raw .. "\n"):gmatch("([^\n]*)\n") do
            lines[#lines + 1] = strip_ansi(line)
        end
    end

    local lh = screen.get_line_height(12, false)
    local visible_lines = math.max(1, math.floor(TERMINAL_H / lh))
    local total = #lines

    -- Clamp terminal scroll
    local max_scroll = math.max(0, total - visible_lines)
    if state.terminal_scroll > max_scroll then
        state.terminal_scroll = max_scroll
    end

    -- Scroll from bottom
    local start = total - visible_lines - state.terminal_scroll
    if start < 0 then start = 0 end

    -- Background for terminal area
    screen.draw_rect(0, TERMINAL_TOP, 640, TERMINAL_H, {color=theme.bg, filled=true})

    local y = TERMINAL_TOP + 2
    local max_text_w = 628
    for i = start + 1, math.min(start + visible_lines, total) do
        local line_text = lines[i] or ""
        local display = truncate_text(line_text, max_text_w, 12, false)
        screen.draw_text(display, 6, y, {color=theme.text, size=12})
        y = y + lh
    end

    -- Scrollbar on right edge
    if total > visible_lines then
        local ind_x = 636
        local bar_top = TERMINAL_TOP + 4
        local bar_h = TERMINAL_H - 8
        screen.draw_line(ind_x, bar_top, ind_x, bar_top + bar_h, {color=theme.border})
        local thumb_h = math.max(8, math.floor(bar_h * visible_lines / total))
        -- Progress: 0 at bottom (scroll=0), 1 at top (scroll=max)
        local progress = max_scroll > 0 and (state.terminal_scroll / max_scroll) or 0
        local thumb_y = bar_top + math.floor((bar_h - thumb_h) * (1 - progress))
        screen.draw_rect(ind_x - 1, thumb_y, 3, thumb_h, {color=theme.text_dim, filled=true, radius=1})
    end

    -- Separator line at bottom of terminal
    screen.draw_line(0, OPTIONS_TOP - 1, 640, OPTIONS_TOP - 1, {color=theme.border})
end

local function draw_options_panel(session)
    local options = session.response_options or {}
    local detected = session.detected_choices or {}
    local session_type = session.session_type or session.type or "unknown"

    -- Use detected_choices for interactive prompts if no response_options
    local display_options = options
    if #options == 0 and #detected > 0 then
        display_options = detected
    end

    local total = #display_options
    local option_row_h = math.floor(OPTIONS_H / VISIBLE_OPTIONS)

    -- Background
    screen.draw_rect(0, OPTIONS_TOP, 640, OPTIONS_H, {color=theme.bg_header, filled=true})

    if total == 0 then
        local msg = "No suggestions"
        local mw = screen.get_text_width(msg, 14, false)
        screen.draw_text(msg, (640 - mw) / 2, OPTIONS_TOP + OPTIONS_H / 2 - 8, {color=theme.text_dim, size=14})
        return
    end

    -- Clamp option cursor
    if state.option_cursor >= total then
        state.option_cursor = math.max(0, total - 1)
    end

    -- Sliding window
    local window_start = math.max(0, math.min(state.option_cursor - 1, total - VISIBLE_OPTIONS))
    if window_start < 0 then window_start = 0 end
    local window_end = math.min(window_start + VISIBLE_OPTIONS - 1, total - 1)

    local y = OPTIONS_TOP
    for i = window_start, window_end do
        local opt = display_options[i + 1]
        local is_selected = (i == state.option_cursor)

        -- Selected background
        if is_selected then
            screen.draw_rect(0, y, 630, option_row_h, {color=theme.bg_selected, filled=true})
        end

        -- Cursor indicator
        local cx = 6
        if is_selected then
            screen.draw_text(">", cx, y + (option_row_h - 16) / 2, {color=theme.accent, size=14, bold=true})
        end
        cx = cx + 16

        -- Category badge
        local cat = opt.category or "custom"
        local cat_label = CATEGORY_LABELS[cat] or ".."
        local cat_color = category_color(cat)
        local cat_w = screen.get_text_width(cat_label, 11, true)
        screen.draw_rect(cx, y + (option_row_h - 18) / 2, cat_w + 8, 18, {color=cat_color, filled=true, radius=3})
        screen.draw_text(cat_label, cx + 4, y + (option_row_h - 14) / 2, {color={20, 20, 30}, size=11, bold=true})
        cx = cx + cat_w + 14

        -- Option text
        local opt_text = opt.text or opt.label or opt.value or ""
        local opt_max_w = 620 - cx
        local opt_trunc = truncate_text(opt_text, opt_max_w, MONO_SIZE, false)
        screen.draw_text(opt_trunc, cx, y + (option_row_h - 14) / 2, {color=theme.text, size=MONO_SIZE})

        y = y + option_row_h
    end

    -- Scroll indicator for options if more than visible
    if total > VISIBLE_OPTIONS then
        local ind_x = 635
        local bar_top = OPTIONS_TOP + 4
        local bar_h = OPTIONS_H - 8
        screen.draw_line(ind_x, bar_top, ind_x, bar_top + bar_h, {color=theme.border})
        local thumb_h = math.max(6, math.floor(bar_h * VISIBLE_OPTIONS / total))
        local progress = (total > 1) and (state.option_cursor / (total - 1)) or 0
        local thumb_y = bar_top + math.floor((bar_h - thumb_h) * progress)
        screen.draw_rect(ind_x - 1, thumb_y, 3, thumb_h, {color=theme.text_dim, filled=true, radius=1})
    end
end

local function get_session_footer_hints(session)
    local session_type = session.session_type or session.type or "unknown"
    local sess_status = session.status or "unknown"
    local hints = {}

    if session_type == "claude_code" then
        if sess_status == "waiting" then
            hints[#hints + 1] = {"A", "Send", theme.btn_a}
            hints[#hints + 1] = {"Y", "Ghost", theme.btn_x}
            hints[#hints + 1] = {"B", "Esc", theme.btn_b}
            hints[#hints + 1] = {"SEL", "Dash", theme.btn_l}
            if session.permission_mode then
                hints[#hints + 1] = {"STA", "Perm", theme.btn_l}
            end
        elseif sess_status == "thinking" then
            hints[#hints + 1] = {"B", "Ctrl+C", theme.btn_b}
            hints[#hints + 1] = {"SEL", "Dash", theme.btn_l}
        else
            hints[#hints + 1] = {"B", "Ctrl+C", theme.btn_b}
            hints[#hints + 1] = {"SEL", "Dash", theme.btn_l}
        end
    elseif session_type == "interactive_prompt" then
        hints[#hints + 1] = {"A", "Enter", theme.btn_a}
        hints[#hints + 1] = {"B", "Ctrl+C", theme.btn_b}
        hints[#hints + 1] = {"Y", "y", theme.btn_x}
        hints[#hints + 1] = {"X", "n", theme.btn_x}
    elseif session_type == "running_process" then
        hints[#hints + 1] = {"B", "Ctrl+C", theme.btn_b}
        hints[#hints + 1] = {"Y", "Ctrl+Z", theme.btn_x}
        hints[#hints + 1] = {"SEL", "Dash", theme.btn_l}
    elseif session_type == "idle_shell" then
        hints[#hints + 1] = {"A", "Enter", theme.btn_a}
        hints[#hints + 1] = {"UD", "History", theme.btn_l}
        hints[#hints + 1] = {"SEL", "Dash", theme.btn_l}
    else
        hints[#hints + 1] = {"SEL", "Dash", theme.btn_l}
    end

    return hints
end

local function draw_session_screen()
    local session = get_current_session()
    if not session then
        draw_header("VIBEBOY", "No Session")
        local msg = "No session selected"
        local mw = screen.get_text_width(msg, 16, false)
        screen.draw_text(msg, (640 - mw) / 2, 220, {color=theme.text_dim, size=16})
        draw_footer({{"SEL", "Dash", theme.btn_l}})
        return
    end

    -- Header: session index / total
    local idx_str = ""
    if state.current_index > 0 and #state.session_order > 0 then
        idx_str = state.current_index .. "/" .. #state.session_order
    end
    draw_header("VIBEBOY", idx_str)

    -- Status bar
    draw_status_bar(session)

    -- Terminal view
    draw_terminal_view(session)

    -- Options panel
    draw_options_panel(session)

    -- Footer
    local hints = get_session_footer_hints(session)
    draw_footer(hints)
end

-- ── Input Handling ─────────────────────────────────────────────────────────

local function navigate_to_session(index)
    if index < 1 or index > #state.session_order then return end
    state.current_index = index
    state.current_session = state.session_order[index]
    state.option_cursor = 0
    state.terminal_scroll = 0
end

local function push_screen(name)
    state.screen_stack[#state.screen_stack + 1] = name
end

local function pop_screen()
    if #state.screen_stack > 1 then
        state.screen_stack[#state.screen_stack] = nil
    end
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local current = state.screen_stack[#state.screen_stack]

    if current == "connect" then
        -- No input handling on connect screen, auto-polls
        return
    end

    if current == "dashboard" then
        local n = #state.session_order
        if button == "dpad_up" then
            state.dashboard_cursor = math.max(1, state.dashboard_cursor - 1)
        elseif button == "dpad_down" then
            state.dashboard_cursor = math.min(math.max(1, n), state.dashboard_cursor + 1)
        elseif button == "a" and n > 0 and state.dashboard_cursor <= n then
            local session_id = state.session_order[state.dashboard_cursor]
            state.current_session = session_id
            state.current_index = state.dashboard_cursor
            state.option_cursor = 0
            state.terminal_scroll = 0
            push_screen("session")
        elseif button == "b" or button == "select" then
            pop_screen()
        end
        return
    end

    if current == "session" then
        local session = get_current_session()
        if not session then return end

        local session_type = session.session_type or session.type or "unknown"
        local sess_status = session.status or "unknown"
        local options = session.response_options or {}
        local detected = session.detected_choices or {}
        local display_options = options
        if #options == 0 and #detected > 0 then
            display_options = detected
        end
        local n_opts = #display_options

        -- Universal controls (all session types, all statuses)
        if button == "l1" then
            -- Previous session
            if state.current_index > 1 then
                navigate_to_session(state.current_index - 1)
            elseif #state.session_order > 0 then
                navigate_to_session(#state.session_order)
            end
            return
        elseif button == "r1" then
            -- Next session
            if state.current_index < #state.session_order then
                navigate_to_session(state.current_index + 1)
            elseif #state.session_order > 0 then
                navigate_to_session(1)
            end
            return
        elseif button == "l2" then
            -- Scroll terminal up
            state.terminal_scroll = state.terminal_scroll + SCROLL_STEP
            return
        elseif button == "r2" then
            -- Scroll terminal down
            state.terminal_scroll = math.max(0, state.terminal_scroll - SCROLL_STEP)
            return
        elseif button == "select" then
            push_screen("dashboard")
            return
        elseif button == "start" then
            -- Toggle Claude permission mode
            send_action("send_keys", {keys = "BTab"})
            return
        end

        -- Session-type-specific controls
        if session_type == "claude_code" then
            if sess_status == "waiting" then
                if button == "dpad_up" then
                    state.option_cursor = math.max(0, state.option_cursor - 1)
                elseif button == "dpad_down" then
                    state.option_cursor = math.min(math.max(0, n_opts - 1), state.option_cursor + 1)
                elseif button == "a" and n_opts > 0 then
                    local selected = display_options[state.option_cursor + 1]
                    if selected then
                        local text = selected.text or selected.label or selected.value or ""
                        send_action("send_response", {text = text})
                    end
                elseif button == "y" then
                    send_action("accept_ghost", {})
                elseif button == "b" then
                    send_action("escape", {})
                end
            elseif sess_status == "thinking" then
                if button == "b" then
                    send_action("interrupt", {})
                end
            end

        elseif session_type == "interactive_prompt" then
            if button == "a" then
                send_action("send_keys", {keys = "Enter"})
            elseif button == "b" then
                send_action("interrupt", {})
            elseif button == "y" then
                send_action("send_keys", {keys = "y Enter"})
            elseif button == "x" then
                send_action("send_keys", {keys = "n Enter"})
            elseif button == "dpad_up" then
                send_action("send_keys", {keys = "Up"})
            elseif button == "dpad_down" then
                send_action("send_keys", {keys = "Down"})
            end

        elseif session_type == "running_process" then
            if button == "b" then
                send_action("interrupt", {})
            elseif button == "y" then
                send_action("send_keys", {keys = "C-z"})
            end

        elseif session_type == "idle_shell" then
            if button == "a" then
                send_action("send_keys", {keys = "Enter"})
            elseif button == "dpad_up" then
                send_action("send_keys", {keys = "Up"})
            elseif button == "dpad_down" then
                send_action("send_keys", {keys = "Down"})
            end
        end

        return
    end
end

-- ── Polling (on_update) ───────────────────────────────────────────────────

function on_update(dt)
    state.elapsed = state.elapsed + dt
    state.poll_timer = state.poll_timer + dt
    if state.poll_timer < state.poll_interval then return end
    state.poll_timer = 0

    local resp = http.get(get_api_url() .. "/api/state")
    if resp and resp.ok then
        local data = json.decode(resp.body)
        if data and data.sessions then
            state.sessions = data.sessions
            state.session_order = sort_sessions(data.sessions)
            state.connected = true
            -- Auto-select first session if none
            if not state.current_session and #state.session_order > 0 then
                state.current_session = state.session_order[1]
                state.current_index = 1
            end
            -- Validate current session still exists
            if state.current_session and not state.sessions[state.current_session] then
                if #state.session_order > 0 then
                    state.current_session = state.session_order[1]
                    state.current_index = 1
                else
                    state.current_session = nil
                end
            end
            -- Update current_index if session_order changed
            if state.current_session then
                for i, id in ipairs(state.session_order) do
                    if id == state.current_session then
                        state.current_index = i
                        break
                    end
                end
            end
            -- Clamp option cursor
            local session = state.current_session and state.sessions[state.current_session]
            if session then
                local opts = session.response_options or {}
                local detected = session.detected_choices or {}
                local display = opts
                if #opts == 0 and #detected > 0 then
                    display = detected
                end
                local n = #display
                if state.option_cursor >= n then
                    state.option_cursor = math.max(0, n - 1)
                end
            end
            -- Leave connect screen once daemon is reachable
            if state.screen_stack[#state.screen_stack] == "connect" then
                if #state.session_order > 0 then
                    state.screen_stack = {"session"}
                else
                    state.screen_stack = {"dashboard"}
                end
            end
        end
    else
        if state.connected then
            state.connected = false
            state.screen_stack = {"connect"}
        end
    end
end

-- ── Rendering ─────────────────────────────────────────────────────────────

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    local current = state.screen_stack[#state.screen_stack]
    if current == "connect" then
        draw_connect_screen()
    elseif current == "dashboard" then
        draw_dashboard_screen()
    elseif current == "session" then
        draw_session_screen()
    end
end

-- ── Initialization ────────────────────────────────────────────────────────

function on_init()
    local config = storage.load("config")
    if config then
        state.daemon_host = config.host or DEFAULT_HOST
        state.daemon_port = config.port or DEFAULT_PORT
    end
end
