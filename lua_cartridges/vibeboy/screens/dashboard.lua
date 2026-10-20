-- screens/dashboard.lua — Session list overview

local U = _G.ui.public
local C = U.C
local ui = require("ui")

local M = {}

local ROW_HEIGHT = 72
local VISIBLE = 5

--- Sort sessions: waiting first, then alphabetical by name.
local function sort_sessions(sessions)
    table.sort(sessions, function(a, b)
        local a_waiting = a.status == "waiting" and 0 or 1
        local b_waiting = b.status == "waiting" and 0 or 1
        if a_waiting ~= b_waiting then
            return a_waiting < b_waiting
        end
        return (a.session_name or "") < (b.session_name or "")
    end)
end

--- Draw the remote session dashboard.
function M.draw(state)
    local sessions = state.sessions or {}
    local n, waiting = #sessions, 0
    for _, session in ipairs(sessions) do
        if session.status == "waiting" then waiting = waiting + 1 end
    end
    U.header("VibeBoy", "REMOTE PANEL / SESSIONS", state.poll_in_flight and "Updating" or "Connected", C.green)
    U.metric(24, 112, 214, 108, "Sessions", n, "total", C.teal)
    U.metric(250, 112, 214, 108, "Waiting", waiting, "input", waiting > 0 and C.gold or C.muted)
    U.panel(476, 112, 220, 108, C.panel)
    U.text("CONNECTION", 490, 124, 14, C.muted, 192, true)
    U.text(state.ssh_active and "SSH tunnel" or "Direct HTTP", 490, 155, 18, C.paper, 192, true)
    U.text(state.host, 490, 192, 14, C.muted, 192)
    U.text("Remote sessions", 24, 236, 18, C.paper, 500, true)
    U.text(n > 0 and string.format("%d / %d", math.min(state.dashboard_cursor, n), n) or "0", 580, 238, 14, C.muted, 116)

    if n == 0 then
        U.empty("No sessions", "The daemon returned an empty session list. Start a tmux session on your server, then press X to refresh.", 290)
    else
        state.dashboard_cursor = math.max(1, math.min(state.dashboard_cursor, n))
        local first = math.max(1, math.min(state.dashboard_cursor - VISIBLE + 1, n - VISIBLE + 1))
        for index = first, math.min(n, first + VISIBLE - 1) do
            local session, y = sessions[index], 270 + (index - first) * ROW_HEIGHT
            local selected = index == state.dashboard_cursor
            local fg, secondary = U.row(24, y, 672, ROW_HEIGHT - 6, selected)
            U.text(session.session_name or session.session_id, 40, y + 10, 18, fg, 476, true)
            U.text(ui.type_label(session.session_type) .. " / " .. (session.pane_command or ""),
                40, y + 39, 14, secondary, 488)
            U.text(session.status or "idle", 546, y + 24, 14,
                selected and fg or ui.status_color(session.status), 134, true)
        end
        ui.scroll(270, VISIBLE * ROW_HEIGHT - 6, first, n, VISIBLE)
    end
    if state.connect_error and state.connect_error ~= "" then
        U.text(state.connect_error, 24, 648, 14, C.red, 672)
    end
    U.footer({{"D-PAD", "Select"}, {"A", "Open"}, {"X", "Refresh"}, {"L2/R2", "Page"}, {"B", "Close"}})
end

--- Handle input on dashboard screen.
-- @return string|nil  "open_session", "refresh", or nil
function M.on_input(state, button, action)
    if action ~= "press" and action ~= "repeat" then return nil end
    local sessions = state.sessions or {}
    local n = #sessions

    if button == "dpad_up" then
        state.dashboard_cursor = math.max(1, state.dashboard_cursor - 1)
    elseif button == "dpad_down" then
        state.dashboard_cursor = math.min(math.max(1, n), state.dashboard_cursor + 1)
    elseif button == "a" and n > 0 then
        return "open_session"
    elseif button == "x" then
        return "refresh"
    elseif button == "b" then
        return "disconnect"
    elseif button == "l2" then
        local visible = VISIBLE
        state.dashboard_cursor = math.max(1, state.dashboard_cursor - visible)
    elseif button == "r2" then
        local visible = VISIBLE
        state.dashboard_cursor = math.min(math.max(1, n), state.dashboard_cursor + visible)
    end

    return nil
end

--- Sort the sessions list in state.
function M.sort_sessions(sessions)
    sort_sessions(sessions)
end

return M
