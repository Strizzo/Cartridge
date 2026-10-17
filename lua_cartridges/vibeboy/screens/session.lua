-- Session terminal and response controls.
local U = _G.ui.public
local C = U.C
local ui = require("ui")
local M = {}
local SCROLL_STEP = 3
local TERMINAL_LINES = 11

local function build_options(session)
    local options = session.response_options or {}
    local choices = session.detected_choices or {}
    local all_opts = {}
    for _, o in ipairs(options) do
        all_opts[#all_opts + 1] = {
            text = o.text or "?",
            category = o.category or "custom",
            kind = o.keys and "keys" or "response",
            key_sequence = o.keys,
        }
    end
    for _, c in ipairs(choices) do
        all_opts[#all_opts + 1] = {
            text = c.label or c.text or "?",
            category = c.is_default and "approve" or "custom",
            kind = "choice",
            key_sequence = c.key_sequence or c.keys,
        }
    end
    if #all_opts == 0 then
        all_opts[1] = {
            text = "Interrupt (Ctrl+C)",
            category = "danger",
            kind = "keys",
            key_sequence = "C-c",
        }
    end
    return all_opts
end

local function get_selected_option(state, session)
    local options = build_options(session)
    state.option_index = math.max(1, math.min(state.option_index, #options))
    return options[state.option_index]
end

function M.draw(state)
    local session = state.current_session
    if not session then
        U.header("VibeBoy", "REMOTE PANEL / TERMINAL", "Unavailable", C.muted)
        U.empty("Session ended", "This session is no longer in the daemon's response. Return to the dashboard to choose another session.")
        U.footer({{"B", "Back"}, {"SEL", "Sessions"}})
        return
    end
    U.header("VibeBoy", "REMOTE PANEL / TERMINAL",
        string.format("%d / %d sessions", state.session_index, #state.sessions), C.muted)
    U.text(session.session_name or "Session", 24, 112, 24, C.paper, 500, true)
    U.text(session.status or "idle", 548, 119, 14, ui.status_color(session.status), 148, true)
    local context = ui.type_label(session.session_type) .. " / " .. (session.pane_command or "")
    if session.permission_mode then context = context .. " / permissions: " .. session.permission_mode end
    U.text(context, 24, 148, 14, C.muted, 672)

    U.panel(24, 176, 672, 264, C.panel)
    local lines = session.screen_content or {}
    local n = #lines
    local max_scroll = math.max(0, n - TERMINAL_LINES)
    state.terminal_scroll = math.max(0, math.min(state.terminal_scroll, max_scroll))
    local first = math.max(1, n - TERMINAL_LINES - state.terminal_scroll + 1)
    U.text(state.terminal_scroll == 0 and "TERMINAL / LATEST" or "TERMINAL / SCROLLBACK", 40, 187, 14, C.quiet, 530, true)
    if n == 0 then
        U.text("No terminal output received.", 40, 252, 16, C.muted, 632)
    else
        for index = first, math.min(n, first + TERMINAL_LINES - 1) do
            U.text(lines[index], 40, 215 + (index - first) * 20, 16, C.paper, 632)
        end
        ui.scroll(215, 216, first, n, TERMINAL_LINES)
    end

    local options = build_options(session)
    local opt = get_selected_option(state, session)
    U.panel(24, 454, 672, 174, C.panel2, opt.category == "danger" and C.red or C.teal)
    U.text("RESPONSE / " .. string.upper(opt.category), 40, 466, 14, C.muted, 490, true)
    U.text(string.format("%d / %d", state.option_index, #options), 604, 466, 14, C.muted, 76, true)
    ui.lines(opt.text, 40, 497, 632, 18, 4, C.paper, true)
    U.text(state.action_request_id and "Sending action..." or "A sends this response. Y interrupts the session.",
        40, 603, 14, C.muted, 632)
    U.text(state.connect_error ~= "" and state.connect_error or "D-pad scrolls output. L1/R1 switches session. X denies a waiting prompt.",
        24, 644, 14, state.connect_error ~= "" and C.red or C.muted, 672)
    U.footer({{"A", "Send"}, {"Y", "Stop"}, {"L2/R2", "Option"}, {"B", "Back"}, {"L1/R1", "Session"}})
end

function M.on_input(state, button, action)
    if action ~= "press" and action ~= "repeat" then return nil end
    local session = state.current_session
    if not session then
        if button == "select" or button == "b" then return "dashboard" end
        return nil
    end

    local sid = session.session_id

    -- Universal controls
    if button == "b" then return "dashboard" end
    if button == "l1" then return "prev_session" end
    if button == "r1" then return "next_session" end
    if button == "dpad_up" then
        state.terminal_scroll = state.terminal_scroll + SCROLL_STEP
        return nil
    end
    if button == "dpad_down" then
        state.terminal_scroll = math.max(0, state.terminal_scroll - SCROLL_STEP)
        return nil
    end
    if button == "select" then return "dashboard" end

    -- Mode-specific controls
    local status = session.status

    -- All session types use the same option selection logic
    -- L2/R2: cycle options, A: execute selected, Y: interrupt
    if button == "l2" then
        state.option_index = math.max(1, state.option_index - 1)
        return nil
    elseif button == "r2" then
        local options = session.response_options or {}
        local choices = session.detected_choices or {}
        local n = math.max(1, #options + #choices)
        state.option_index = math.min(n, state.option_index + 1)
        return nil
    elseif button == "a" then
        local opt = get_selected_option(state, session)
        if opt then
            if opt.key_sequence then
                return {action = "send_keys", session_id = sid, payload = {keys = opt.key_sequence}}
            else
                return {action = "send_response", session_id = sid, payload = {text = opt.text}}
            end
        end
    elseif button == "y" then
        return {action = "interrupt", session_id = sid}
    elseif button == "x" then
        -- Quick deny/no for prompts
        if status == "waiting" then
            return {action = "send_keys", session_id = sid, payload = {keys = "C-c"}}
        end
    end
    return nil
end

return M
