-- screens/connect.lua — Connection settings screen

local U = _G.ui.public
local C = U.C
local ui = require("ui")

local M = {}

-- Common SSH users for cycling. "" means auto (let SSH config decide).
local ssh_users = {"", "root", "pi", "admin", "ubuntu", "deck", "ark"}

local function cycle_value(list, current, delta)
    local idx = 1
    for i, v in ipairs(list) do
        if v == current then idx = i; break end
    end
    idx = idx + delta
    if idx < 1 then idx = #list end
    if idx > #list then idx = 1 end
    return list[idx]
end

local function parse_ip(host)
    local parts = {}
    for p in host:gmatch("[^.]+") do
        parts[#parts + 1] = tonumber(p) or 0
    end
    while #parts < 4 do parts[#parts + 1] = 0 end
    return parts
end

local function build_ip(parts)
    return parts[1] .. "." .. parts[2] .. "." .. parts[3] .. "." .. parts[4]
end

--- Draw the connect screen.
function M.draw(state)
    local status = state.connecting and "Connecting" or state.connected and "Connected" or "Disconnected"
    U.header("VibeBoy", "REMOTE PANEL / CONNECTION", status,
        state.connected and C.green or state.connecting and C.gold or C.muted)
    U.text("Connection settings", 24, 114, 18, C.paper, 672, true)

    local fg, secondary = U.row(24, 148, 672, 116, state.edit_field == "host")
    U.text("SERVER", 40, 160, 14, secondary, 620, true)
    if state.edit_field == "host" and state.host_edit_ip then
        local parts, x = parse_ip(state.host), 40
        for i = 1, 4 do
            local value = tostring(parts[i])
            local width = screen.get_text_width(value, 24, true)
            U.text(value, x, 187, 24, fg, width, true)
            if i == state.host_octet then U.rule(x, 218, width, C.red) end
            x = x + width
            if i < 4 then U.text(".", x, 187, 24, secondary, 16, true); x = x + 12 end
        end
        U.text("UP/DOWN octet  /  LEFT/RIGHT value  /  A +10  /  X save", 40, 236, 14, secondary, 632)
    else
        U.text(state.host, 40, 187, 24, fg, 632, true)
        U.text(state.edit_field == "host" and "D-pad chooses a saved server. X enters a new IP." or
            "Choose a saved server or enter an IPv4 address.", 40, 236, 14, secondary, 632)
    end

    fg, secondary = U.row(24, 276, 672, 92, state.edit_field == "port")
    U.text("DAEMON PORT", 40, 288, 14, secondary, 280, true)
    U.display(tostring(state.port), 40, 312, 36, fg, 280)
    U.text(state.edit_field == "port" and "LEFT/RIGHT 1  /  UP/DOWN 100" or "Default port 8766",
        326, 326, 14, secondary, 350)

    fg, secondary = U.row(24, 380, 672, 116, state.edit_field == "ssh_user")
    U.text("SSH TUNNEL", 40, 392, 14, secondary, 400, true)
    U.text(state.ssh_enabled and "ENABLED" or "OFF", 536, 392, 14,
        state.edit_field == "ssh_user" and fg or state.ssh_enabled and C.green or C.muted, 140, true)
    U.text(state.ssh_enabled and ("User: " .. (state.ssh_user ~= "" and state.ssh_user or "auto (SSH config)"))
        or "Direct HTTP connection", 40, 420, 18, fg, 632, true)
    U.text(state.edit_field == "ssh_user" and "D-pad cycles users. START moves to the next field." or
        state.ssh_enabled and "Keys: /roms/Cartridge/ssh or ~/.ssh/" or "Y enables SSH tunneling.",
        40, 464, 14, secondary, 632)

    U.panel(24, 512, 672, 58, state.connecting and C.panel2 or C.red)
    U.text(state.connecting and state.connect_phase or state.edit_field and "B  Finish editing, then A to connect"
        or "A  Connect to server", 40, 531, 18, state.connecting and C.paper or C.black, 632, true)
    if state.connect_error and state.connect_error ~= "" then
        ui.lines(state.connect_error, 24, 586, 672, 16, 3, C.red)
    else
        U.text("tmux sessions and terminal controls on your handheld.", 24, 590, 16, C.muted, 672)
    end

    if state.connecting then
        U.footer({{"B", "Cancel"}, {"START", "Edit"}, {"Y", "SSH"}})
    elseif state.edit_field then
        U.footer({{"D-PAD", "Value"}, {"X", "IP"}, {"Y", "SSH"}, {"B", "Done"}, {"START", "Field"}})
    else
        U.footer({{"A", "Connect"}, {"Y", "SSH"}, {"START", "Edit"}})
    end
end

--- Handle input on connect screen.
function M.on_input(state, button, action)
    if action ~= "press" and action ~= "repeat" then return nil end

    -- Y toggles SSH
    if button == "y" and action == "press" then
        state.ssh_enabled = not state.ssh_enabled
        return nil
    end

    if state.edit_field then
        -- Editing mode
        if button == "b" then
            state.edit_field = nil
            state.host_edit_ip = false
            return nil
        end

        if state.edit_field == "host" then
            if state.host_edit_ip then
                -- IP octet editing mode
                local parts = parse_ip(state.host)
                local octet = state.host_octet or 1
                if button == "dpad_right" then
                    parts[octet] = math.min(255, parts[octet] + 1)
                elseif button == "dpad_left" then
                    parts[octet] = math.max(0, parts[octet] - 1)
                elseif button == "dpad_up" then
                    state.host_octet = math.max(1, octet - 1)
                elseif button == "dpad_down" then
                    state.host_octet = math.min(4, octet + 1)
                elseif button == "a" then
                    parts[octet] = math.min(255, parts[octet] + 10)
                elseif button == "x" then
                    -- Confirm new IP and add to server list
                    state.host = build_ip(parts)
                    -- Add to server list if not already there
                    local found = false
                    for _, s in ipairs(state.servers) do
                        if s == state.host then found = true; break end
                    end
                    if not found then
                        table.insert(state.servers, 1, state.host)
                    end
                    state.host_edit_ip = false
                    return nil
                end
                state.host = build_ip(parts)
            else
                -- Server list cycling mode
                if button == "dpad_right" or button == "dpad_down" then
                    state.host = cycle_value(state.servers, state.host, 1)
                elseif button == "dpad_left" or button == "dpad_up" then
                    state.host = cycle_value(state.servers, state.host, -1)
                elseif button == "x" then
                    -- Enter IP editing mode to add a new server
                    state.host_edit_ip = true
                    state.host_octet = 1
                    -- Start from 0.0.0.0 for a fresh IP
                    state.host = "0.0.0.0"
                end
            end
        elseif state.edit_field == "port" then
            if button == "dpad_right" then
                state.port = math.min(65535, state.port + 1)
            elseif button == "dpad_left" then
                state.port = math.max(1, state.port - 1)
            elseif button == "dpad_up" then
                state.port = math.min(65535, state.port + 100)
            elseif button == "dpad_down" then
                state.port = math.max(1, state.port - 100)
            end
        elseif state.edit_field == "ssh_user" then
            if button == "dpad_right" or button == "dpad_up" then
                state.ssh_user = cycle_value(ssh_users, state.ssh_user, 1)
            elseif button == "dpad_left" or button == "dpad_down" then
                state.ssh_user = cycle_value(ssh_users, state.ssh_user, -1)
            end
        end

        if button == "start" then
            if state.host_edit_ip then
                -- If in IP edit mode, exit it first
                state.host_edit_ip = false
            end
            -- Cycle through editable fields
            local fields = {"host", "port"}
            if state.ssh_enabled then
                fields[#fields + 1] = "ssh_user"
            end
            local idx = 1
            for i, f in ipairs(fields) do
                if f == state.edit_field then idx = i; break end
            end
            idx = idx + 1
            if idx > #fields then idx = 1 end
            state.edit_field = fields[idx]
        end
        return nil
    end

    -- Normal mode
    if button == "a" then
        return "connect"
    elseif button == "start" then
        state.edit_field = "host"
        state.host_edit_ip = false
    end
    return nil
end

return M
