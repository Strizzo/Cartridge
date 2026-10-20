-- VibeBoy Cartridge — Remote control for VibeBoy daemon
-- Manages tmux sessions and Claude Code via HTTP API

local U = _G.ui.public
local C = U.C
local api = require("api")
local ui = require("ui")
local connect_screen = require("screens.connect")
local dashboard_screen = require("screens.dashboard")
local session_screen = require("screens.session")

-- ── State ──────────────────────────────────────────────────────────────────

local state = {
    screen = "connect",  -- "connect", "dashboard", "session"

    -- Connection
    host = "192.168.1.100",
    port = 8766,
    connected = false,
    connecting = false,
    connect_error = "",
    edit_field = nil,  -- nil, "host", "port", "ssh_user"
    servers = {},      -- list of configured servers (hostnames or IPs)
    host_edit_ip = false, -- true = editing IP octets, false = cycling servers
    host_octet = 1,

    -- SSH tunnel
    ssh_enabled = false,
    ssh_user = "",  -- empty = auto (let SSH config decide)
    ssh_active = false,
    ssh_request_id = nil,
    ssh_timer = 0,
    connect_phase = "",
    local_port = nil,

    -- Polling
    poll_timer = 0,
    poll_interval = 1.0,
    fail_count = 0,
    max_failures = 3,
    poll_in_flight = false,
    poll_request_id = nil,
    action_request_id = nil,
    last_etag = nil,

    -- Dashboard
    sessions = {},           -- sorted list of session data
    session_ids = {},        -- ordered list of session IDs
    dashboard_cursor = 1,

    -- Session view
    current_session = nil,   -- current session data table
    session_index = 0,       -- 1-based index into session_ids
    option_index = 1,
    terminal_scroll = 0,

    -- Deferred actions
    pending_action = nil,    -- action to send on next update
    force_poll = false,      -- re-poll immediately after action
    loading = false,

    -- Deferred initial connect
    needs_initial_connect = false,
    ready_to_connect = false,

    -- First-run help
    show_help = false,
}

-- ── Persistence ────────────────────────────────────────────────────────────

-- SSH keys are looked up in this directory on the SD card.
-- Users drop their private key here from their computer.
local SSH_KEY_DIR = "/roms/Cartridge/ssh"

local function save_settings()
    storage.save("vibeboy_settings", {
        host = state.host, port = state.port,
        ssh_enabled = state.ssh_enabled,
        ssh_user = state.ssh_user,
        servers = state.servers,
    })
end

local function load_settings()
    local data = storage.load("vibeboy_settings")
    if data then
        state.host = data.host or state.host
        state.port = data.port or state.port
        if data.ssh_enabled ~= nil then state.ssh_enabled = data.ssh_enabled end
        state.ssh_user = data.ssh_user or state.ssh_user
        if data.servers and #data.servers > 0 then
            state.servers = data.servers
        end
    end

    -- Ensure server list is populated
    if #state.servers == 0 then
        state.servers = {state.host, "localhost"}
    end

    -- Add current host to list if not present
    local found = false
    for _, s in ipairs(state.servers) do
        if s == state.host then found = true; break end
    end
    if not found then
        table.insert(state.servers, 1, state.host)
    end
end

-- ── Session Management ─────────────────────────────────────────────────────

local function update_sessions(data)
    local sessions_map = data.sessions or {}
    local list = {}
    local ids = {}

    for sid, sdata in pairs(sessions_map) do
        sdata.session_id = sid
        -- Normalize fields from daemon API to what the UI expects
        sdata.session_name = sdata.session_name or sdata.name or sid
        sdata.status = sdata.status or (sdata.attached and "attached" or "idle")
        sdata.pane_command = sdata.pane_command or ""
        sdata.session_type = sdata.session_type or "idle_shell"
        sdata.response_options = sdata.response_options or {}
        -- Convert terminal string to lines array for the session screen
        if type(sdata.terminal) == "string" and sdata.terminal ~= "" then
            local lines = {}
            for line in (sdata.terminal .. "\n"):gmatch("(.-)\n") do
                lines[#lines + 1] = line
            end
            sdata.screen_content = lines
        else
            sdata.screen_content = sdata.screen_content or {}
        end
        list[#list + 1] = sdata
        ids[#ids + 1] = sid
    end

    dashboard_screen.sort_sessions(list)

    -- Rebuild ordered IDs from sorted list
    ids = {}
    for _, s in ipairs(list) do
        ids[#ids + 1] = s.session_id
    end

    state.sessions = list
    state.session_ids = ids

    state.dashboard_cursor = math.max(1, math.min(state.dashboard_cursor, #list))
    -- Keep the chosen session identity stable when waiting sessions reorder.
    if state.screen == "session" then
        local sid = state.current_session and state.current_session.session_id
        state.current_session = nil
        for index, session in ipairs(list) do
            if session.session_id == sid then
                state.current_session = session
                state.session_index = index
                break
            end
        end
    end
end

local function open_session(index)
    if index < 1 or index > #state.session_ids then return end
    state.session_index = index
    state.option_index = 1
    state.terminal_scroll = 0

    local sid = state.session_ids[index]
    for _, s in ipairs(state.sessions) do
        if s.session_id == sid then
            state.current_session = s
            break
        end
    end

    state.screen = "session"
end

local function switch_session(delta)
    local n = #state.session_ids
    if n == 0 then return end
    local new_idx = state.session_index + delta
    if new_idx < 1 then new_idx = n end
    if new_idx > n then new_idx = 1 end
    open_session(new_idx)
end

-- ── Polling ────────────────────────────────────────────────────────────────

local function api_host()
    if state.ssh_active and state.local_port then
        return "127.0.0.1"
    end
    return state.host
end

local function api_port()
    if state.ssh_active and state.local_port then
        return state.local_port
    end
    return state.port
end

local function close_tunnel()
    -- SDK close cancels a pending worker and closes any active tunnel.
    ssh.close()
    state.ssh_request_id = nil
    state.ssh_active = false
    state.local_port = nil
end

local function cancel_connection()
    close_tunnel()
    state.connected = false
    state.connecting = false
    state.connect_phase = ""
    state.connect_error = ""
    state.needs_initial_connect = false
    state.ready_to_connect = false
    -- Results from cancelled SSH/HTTP requests cannot reopen the old endpoint.
    state.poll_request_id = nil
    state.poll_in_flight = false
    state.action_request_id = nil
    state.pending_action = nil
    state.last_etag = nil
    state.force_poll = false
end

local function poll_success(data)
    state.connected = true
    state.fail_count = 0
    if data then update_sessions(data) end
    if state.connecting then
        state.connecting = false
        state.connect_phase = ""
        save_settings()
        state.screen = "dashboard"
        state.dashboard_cursor = 1
    end
end

local function poll_error(err)
    if state.connecting then
        state.connecting = false
        state.connected = false
        state.connect_error = err or "Connection failed"
        close_tunnel()
        return
    end
    state.fail_count = state.fail_count + 1
    if state.fail_count >= state.max_failures then
        state.connected = false
        state.connect_error = err or "Connection lost"
        close_tunnel()
        state.screen = "connect"
        state.fail_count = 0
    end
end

local function do_poll_async()
    if state.poll_in_flight then return end
    if not http.get_async then
        poll_error("Async HTTP unavailable")
        return
    end
    local ok, id = pcall(api.poll_state_async, api_host(), api_port(), state.last_etag)
    if not ok or not id then poll_error("Could not start state request"); return end
    state.poll_request_id = id
    state.poll_in_flight = true
end

local function process_async_responses()
    if not http.poll then return false end
    local changed = false
    for _, resp in ipairs(http.poll() or {}) do
        if state.poll_request_id and resp.id == state.poll_request_id then
            changed = true
            state.poll_in_flight = false
            state.poll_request_id = nil
            if resp.status == 304 then
                poll_success()
                if resp.etag then state.last_etag = resp.etag end
            else
                local value, err = api.parse_state(resp)
                if value then
                    poll_success(value)
                    if resp.etag then state.last_etag = resp.etag end
                else poll_error(err) end
            end
        elseif state.action_request_id and resp.id == state.action_request_id then
            changed = true
            state.action_request_id = nil
            if resp.ok then state.connect_error = ""
            else state.connect_error = "Action failed: HTTP " .. tostring(resp.status or "error") end
            state.force_poll = true
        end
    end
    return changed
end

local function process_ssh_responses()
    if not ssh.poll then return false end
    local ok, responses = pcall(ssh.poll)
    if not ok then
        if state.ssh_request_id then
            cancel_connection()
            state.connect_error = "Could not read SSH result"
            return true
        end
        return false
    end
    local changed = false
    for _, result in ipairs(responses or {}) do
        if state.ssh_request_id and result.id == state.ssh_request_id then
            state.ssh_request_id = nil
            changed = true
            local port = tonumber(result.local_port)
            if result.ok and port and port >= 1 and port <= 65535 and port == math.floor(port) then
                state.ssh_active = true
                state.local_port = port
                state.connect_phase = "Contacting daemon..."
                do_poll_async()
            else
                cancel_connection()
                state.connect_error = result.error or "SSH tunnel failed"
            end
        end
    end
    return changed
end

local function do_connect()
    if state.connecting or state.poll_in_flight then return end
    state.connecting = true
    state.connect_error = ""
    state.last_etag = nil
    state.ssh_timer = 0
    if state.ssh_enabled then
        if not ssh.tunnel_async or not ssh.poll then
            state.connecting = false
            state.connect_error = "Async SSH requires runtime 0.7.0"
            return
        end
        state.connect_phase = "Opening SSH tunnel..."
        local ok, id = pcall(ssh.tunnel_async, {
            host = state.host,
            user = state.ssh_user,
            key_dir = SSH_KEY_DIR,
            remote_port = state.port,
        })
        if not ok or not id then
            cancel_connection()
            state.connect_error = "Could not start SSH tunnel"
            return
        end
        state.ssh_request_id = id
        return
    end
    state.connect_phase = "Contacting daemon..."
    do_poll_async()
end

-- ── Lifecycle ──────────────────────────────────────────────────────────────

function on_init()
    load_settings()
    -- Show help on first launch (no saved settings yet)
    local saved = storage.load("vibeboy_settings")
    if not saved then
        state.show_help = true
    end
end

function on_update(dt)
    -- Deferred connect (after first frame renders)
    if state.ready_to_connect then
        state.ready_to_connect = false
        do_connect()
        return true
    end

    local dirty = process_ssh_responses()
    if state.ssh_request_id then
        state.ssh_timer = state.ssh_timer + dt
        if state.ssh_timer >= 30 then
            cancel_connection()
            state.connect_error = "SSH connection timed out"
            dirty = true
        end
    end

    -- Send pending action
    if state.pending_action then
        local act = state.pending_action
        state.pending_action = nil
        local ok, err, id = api.send_action(api_host(), api_port(), act.action, act.session_id, act.payload)
        if not ok then state.connect_error = err or "Action failed" end
        state.action_request_id = id
        state.force_poll = not id
        dirty = true
    end

    -- Drain any completed async HTTP responses (cheap, non-blocking)
    dirty = process_async_responses() or dirty

    -- Polling (only when connected and on dashboard/session)
    if state.connected and (state.screen == "dashboard" or state.screen == "session") then
        if state.force_poll then
            state.force_poll = false
            state.poll_timer = 0
            do_poll_async()
        else
            state.poll_timer = state.poll_timer + dt
            if state.poll_timer >= state.poll_interval then
                state.poll_timer = 0
                do_poll_async()
            end
        end
    end
    return dirty
end

function on_input(button, action)
    -- Dismiss help overlay
    if state.show_help then
        if action == "press" then
            state.show_help = false
        end
        return
    end

    if state.screen == "connect" then
        if action == "press" or action == "repeat" then
            local pending = state.connecting or state.needs_initial_connect or state.ready_to_connect
            if pending and button == "b" then cancel_connection(); return end
            if pending and (button == "start" or button == "y" or state.edit_field) then
                cancel_connection()
            end
        end
        local result = connect_screen.on_input(state, button, action)
        if result == "connect" and not state.connecting then
            state.needs_initial_connect = true
        end

    elseif state.screen == "dashboard" then
        local result = dashboard_screen.on_input(state, button, action)
        if result == "open_session" then
            open_session(state.dashboard_cursor)
        elseif result == "refresh" then
            state.force_poll = true
        elseif result == "disconnect" then
            cancel_connection()
            state.screen = "connect"
        end

    elseif state.screen == "session" then
        local result = session_screen.on_input(state, button, action)
        if result == "dashboard" then
            state.screen = "dashboard"
        elseif result == "prev_session" then
            switch_session(-1)
        elseif result == "next_session" then
            switch_session(1)
        elseif type(result) == "table" and result.action then
            if not state.action_request_id and not state.pending_action then
                state.connect_error = ""
                state.pending_action = result
            end
        end
    end
end

local function draw_help()
    U.header("VibeBoy", "REMOTE PANEL / SETUP", "First launch", C.muted)
    U.display("Your server, in hand.", 24, 126, 36, C.paper, 672)
    ui.lines("Manage tmux sessions and terminal responses through your VibeBoy daemon.", 24, 181, 672, 18, 2, C.muted)
    local steps = {
        {"01 / RUN THE DAEMON", "Install vibeboy-daemon on your server. Default port: 8766.", "github.com/Strizzo/vibeboy-daemon"},
        {"02 / COPY YOUR SSH KEY", "Place your private key on the SD card:", "Cartridge/ssh/id_ed25519"},
        {"03 / SET YOUR CONNECTION", "START edits fields. X enters an IP. Y enables SSH.", "B finishes editing. A connects to your server."},
    }
    for i, step in ipairs(steps) do
        local y = 254 + (i - 1) * 124
        U.panel(24, y, 672, 112, C.panel, i == 1 and C.red or C.line)
        U.text(step[1], 40, y + 12, 14, C.muted, 632, true)
        U.text(step[2], 40, y + 40, 16, C.paper, 632)
        U.text(step[3], 40, y + 72, 16, C.muted, 632)
    end
    U.footer({{"ANY", "Continue"}})
end

function on_render()
    -- Deferred connect: trigger on first render frame
    if state.needs_initial_connect then
        state.needs_initial_connect = false
        state.ready_to_connect = true
    end

    if state.show_help then
        draw_help()
    elseif state.screen == "connect" then
        connect_screen.draw(state)
    elseif state.screen == "dashboard" then
        dashboard_screen.draw(state)
    elseif state.screen == "session" then
        session_screen.draw(state)
    end
end

function on_destroy()
    close_tunnel()
end
