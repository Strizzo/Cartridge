-- api.lua — HTTP client wrapper for VibeBoy daemon

local M = {}

--- Poll full state from daemon (synchronous — blocks UI; use poll_state_async).
-- @param host string  e.g. "192.168.1.100"
-- @param port number  e.g. 8766
-- @return table|nil   parsed state table, or nil on error
-- @return string|nil  error message on failure
function M.poll_state(host, port)
    local url = "http://" .. host .. ":" .. tostring(port) .. "/api/state"
    local ok, resp = pcall(http.get, url)
    if not ok then
        return nil, "Connection failed"
    end
    if not resp.ok then
        return nil, "HTTP " .. tostring(resp.status or "error")
    end
    local dok, data = pcall(json.decode, resp.body)
    if not dok or type(data) ~= "table" or type(data.sessions) ~= "table" then
        return nil, "Invalid JSON"
    end
    return data, nil
end

--- Start an async poll. Returns a request id that should be matched against
-- http.poll() results. If `etag` is provided, sends If-None-Match so the
-- daemon can return 304 when nothing changed (saves JSON decode + network).
function M.poll_state_async(host, port, etag)
    local url = "http://" .. host .. ":" .. tostring(port) .. "/api/state"
    if etag then
        return http.get_async(url, etag)
    end
    return http.get_async(url)
end

--- Drain pending async responses; returns parsed state for any state poll
-- request whose id matches `state_request_id`. Returns nil if not ready.
-- For other request types, results are dropped — callers should track ids.
function M.drain_responses()
    if not http.poll then return {} end
    local raw = http.poll()
    return raw or {}
end

--- Parse a poll response body into a state table.
-- @return table|nil, string|nil  state, error
function M.parse_state(resp)
    if not resp.ok then
        return nil, "HTTP " .. tostring(resp.status or "error")
    end
    local dok, data = pcall(json.decode, resp.body)
    if not dok or type(data) ~= "table" or type(data.sessions) ~= "table" then
        return nil, "Invalid JSON"
    end
    return data, nil
end

--- Send an action to the daemon (synchronous; small request, returns quickly).
-- @param host    string
-- @param port    number
-- @param action  string   e.g. "send_response", "interrupt"
-- @param session_id string
-- @param payload table|nil  optional payload
-- @return boolean success
-- @return string|nil error message
function M.send_action(host, port, action, session_id, payload)
    local url = "http://" .. host .. ":" .. tostring(port) .. "/api/action"
    local body = json.encode({
        action = action,
        session_id = session_id,
        payload = payload or {},
    })
    -- Prefer async post if available, fall back to sync
    if http.post_async then
        local ok, id = pcall(http.post_async, url, body)
        if not ok or not id then return false, "Could not start action request" end
        return true, nil, id
    end
    local ok, resp = pcall(http.post, url, body)
    if not ok then
        return false, "Connection failed"
    end
    if not resp.ok then
        return false, "HTTP " .. tostring(resp.status or "error")
    end
    return true, nil
end

return M
