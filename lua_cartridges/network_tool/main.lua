-- Net Tool: asynchronous network diagnostics for CartridgeOS.
local U = ui.public
local C = U.C
local HEADER_VISIBLE = 18

local state = {
    tabs = {"Overview", "Headers", "DNS", "Probe"},
    active_tab = 1,
    public_ip = nil,
    geo = nil,
    loading = false,
    error_msg = "",
    headers_lines = {},
    headers_scroll = 0,
    headers_max_scroll = 0,
    headers_loading = false,
    headers_error = false,
    dns_targets = {"google.com", "cloudflare.com", "github.com", "wikipedia.org", "amazon.com"},
    dns_results = {},
    dns_cursor = 0,
    dns_loading = false,
    probe_targets = {
        {name="Google", url="https://www.google.com"},
        {name="Cloudflare", url="https://1.1.1.1"},
        {name="GitHub", url="https://github.com"},
        {name="Wikipedia", url="https://en.wikipedia.org"},
        {name="HN", url="https://news.ycombinator.com"},
        {name="Reddit", url="https://www.reddit.com"},
    },
    probe_results = {},
    probe_loading = false,
    probe_cursor = 0,
}

-- Each operation keeps its own pending results; tabs never block requests.
local pending = {}
local function request(url, handler)
    local ok, id = pcall(http.get_async, url)
    if ok and id then
        pending[id] = handler
    else
        handler({ok=false, status=0, body="Request could not be started", elapsed_ms=0})
    end
end

local function poll_requests()
    local dirty = false
    for _, response in ipairs(http.poll()) do
        local handler = pending[response.id]
        if handler then
            pending[response.id] = nil
            handler(response)
            dirty = true
        end
    end
    return dirty
end

local function load_overview()
    if state.loading then return false end
    state.loading = true
    state.error_msg = ""
    request("https://ipinfo.io/json", function(resp)
        local ok, data = pcall(json.decode, resp.body)
        if resp.ok and ok and type(data) == "table" and data.ip then
            state.public_ip = tostring(data.ip)
            state.geo = {city=data.city or "", region=data.region or "", country=data.country or "",
                org=data.org or "", timezone=data.timezone or "", loc=data.loc or ""}
        else
            state.error_msg = "Could not fetch IP info. X to retry."
        end
        state.loading = false
    end)
    return true
end

local function load_headers()
    if state.headers_loading then return false end
    state.headers_loading = true
    state.headers_error = false
    state.headers_lines = {}
    state.headers_scroll = 0
    state.headers_max_scroll = 0
    request("https://httpbin.org/headers", function(resp)
        local ok, data = pcall(json.decode, resp.body)
        if resp.ok and ok and type(data) == "table" and type(data.headers) == "table" then
            for k, v in pairs(data.headers) do
                state.headers_lines[#state.headers_lines + 1] = {key=tostring(k), value=tostring(v)}
            end
            table.sort(state.headers_lines, function(a, b) return a.key < b.key end)
        else
            state.headers_error = true
        end
        state.headers_loading = false
    end)
    return true
end

local function load_dns()
    if state.dns_loading then return false end
    state.dns_loading = true
    state.dns_results = {}
    state.dns_cursor = 0
    local remaining = #state.dns_targets
    for i, target in ipairs(state.dns_targets) do
        local result = {name=target, status="waiting", ips={}}
        state.dns_results[i] = result
        request("https://dns.google/resolve?name=" .. target .. "&type=A", function(resp)
            local ok, data = pcall(json.decode, resp.body)
            result.status = "error"
            if resp.ok and ok and type(data) == "table" then
                result.status = data.Status == 0 and "ok" or ("code:" .. tostring(data.Status))
                for _, answer in ipairs(type(data.Answer) == "table" and data.Answer or {}) do
                    if answer.type == 1 then result.ips[#result.ips + 1] = tostring(answer.data) end
                end
            end
            remaining = remaining - 1
            state.dns_loading = remaining > 0
        end)
    end
    return true
end

local function load_probes()
    if state.probe_loading then return false end
    state.probe_loading = true
    state.probe_results = {}
    state.probe_cursor = 0
    local remaining = #state.probe_targets
    for i, target in ipairs(state.probe_targets) do
        local result = {name=target.name, url=target.url, status="waiting", code=0, time_ms=nil}
        state.probe_results[i] = result
        request(target.url, function(resp)
            result.code = resp.status or 0
            result.status = resp.ok and "up" or "down"
            -- A failed launch has no measured response time.
            result.time_ms = type(resp.elapsed_ms) == "number" and resp.elapsed_ms > 0
                and math.floor(resp.elapsed_ms) or nil
            remaining = remaining - 1
            state.probe_loading = remaining > 0
        end)
    end
    return true
end

local function progress(results)
    local done, failed = 0, 0
    for _, r in ipairs(results) do
        if r.status ~= "waiting" then
            done = done + 1
            if r.status ~= "ok" and r.status ~= "up" then failed = failed + 1 end
        end
    end
    return done, failed
end

local function draw_header()
    local status, color = "READY", C.muted
    if state.active_tab == 1 then
        if state.loading then status, color = "FETCHING", C.gold
        elseif state.error_msg ~= "" then status, color = "RETRY X", C.red
        elseif state.public_ip then status = "LOOKUP COMPLETE" end
    elseif state.active_tab == 2 then
        if state.headers_loading then status, color = "FETCHING", C.gold
        elseif state.headers_error then status, color = "RETRY X", C.red
        elseif #state.headers_lines > 0 then status = "RECEIVED" end
    else
        local results = state.active_tab == 3 and state.dns_results or state.probe_results
        local done, failed = progress(results)
        if #results > 0 then
            status = done .. " / " .. #results .. " COMPLETE"
            color = done < #results and C.gold or (failed > 0 and C.red or C.green)
        end
    end
    U.header("NET TOOL", "NETWORK / DIAGNOSTIC WORKBENCH", status, color)
    U.tabs(state.tabs, state.active_tab)
end

local function draw_value_panel(x, y, w, h, label, value)
    U.panel(x, y, w, h, C.panel)
    U.text(label, x + 16, y + 14, 14, C.muted, w - 32, true)
    for i, line in ipairs(U.wrap(value ~= "" and value or "Not supplied", w - 32, 18, 2)) do
        U.text(line, x + 16, y + 42 + (i - 1) * 24, 18, C.paper, w - 32)
    end
end

local function draw_overview()
    U.display("PUBLIC CONNECTION", 24, 160, 28, C.paper, 420)
    U.text("SOURCE / IPINFO", 514, 168, 14, C.muted, 182)
    if not state.public_ip then
        if state.loading then
            U.empty("LOOKING UP PUBLIC IP", "Requesting IP and approximate location from ipinfo.io. Other tools remain available while this request runs.")
        elseif state.error_msg ~= "" then
            U.empty("IP LOOKUP FAILED", state.error_msg)
        else
            U.empty("NO IP INFORMATION", "Press A to look up your public IP and approximate location.")
        end
        return
    end

    U.panel(24, 202, 672, 142, C.panel2, C.red)
    U.text(state.error_msg ~= "" and "LAST RESULT / REFRESH FAILED" or "PUBLIC IP ADDRESS", 40, 218, 14,
        state.error_msg ~= "" and C.gold or C.muted, 640, true)
    U.display(state.public_ip, 40, 252, #state.public_ip > 18 and 34 or 64, C.paper, 640)
    local g = state.geo
    if g then
        local parts = {}
        for _, value in ipairs({g.city, g.region, g.country}) do
            if value ~= "" then parts[#parts + 1] = value end
        end
        draw_value_panel(24, 360, 328, 104, "APPROXIMATE LOCATION", table.concat(parts, ", "))
        draw_value_panel(368, 360, 328, 104, "TIMEZONE", g.timezone)
        draw_value_panel(24, 480, 672, 86, "NETWORK / ORGANIZATION", g.org)
        U.panel(24, 582, 672, 68, C.panel)
        U.text("COORDINATES", 40, 596, 14, C.muted, 150, true)
        U.text(g.loc ~= "" and g.loc or "Not supplied", 224, 596, 18, C.paper, 450)
        U.text("Location follows the public connection, including any active VPN.", 40, 625, 14, C.muted, 640)
    end
end

local function draw_headers_tab()
    U.display("REQUEST HEADERS", 24, 160, 28, C.paper, 420)
    U.text("SOURCE / HTTPBIN", 488, 168, 14, C.muted, 208)
    if state.headers_loading then
        U.empty("FETCHING HEADERS", "Waiting for httpbin.org to echo this tool's HTTP request headers.")
        return
    elseif state.headers_error then
        U.empty("HEADERS UNAVAILABLE", "Could not fetch headers. X to retry.")
        return
    elseif #state.headers_lines == 0 then
        U.empty("INSPECT A REQUEST", "Press A to send a request to httpbin.org and inspect the headers received by that service.")
        return
    end

    -- Wrap the complete values; scrolling traverses lines, including long values.
    local lines = {}
    for _, h in ipairs(state.headers_lines) do
        lines[#lines + 1] = {text=h.key .. ":", key=true}
        for _, line in ipairs(U.wrap(h.value, 632, 16, 1024)) do
            lines[#lines + 1] = {text=line}
        end
        lines[#lines + 1] = {text=""}
    end
    state.headers_max_scroll = math.max(0, #lines - HEADER_VISIBLE)
    local start = math.min(state.headers_scroll, state.headers_max_scroll) + 1
    U.panel(24, 202, 672, 410, C.panel)
    for i = 0, HEADER_VISIBLE - 1 do
        local line = lines[start + i]
        if not line then break end
        U.text(line.text, 40, 214 + i * 22, line.key and 14 or 16,
            line.key and C.teal or C.paper, 632, line.key)
    end
    U.text(#state.headers_lines .. " HEADERS / LINES " .. start .. "–" .. math.min(#lines, start + HEADER_VISIBLE - 1)
        .. " OF " .. #lines, 24, 628, 14, C.muted, 672)
end

local function draw_dns_tab()
    U.display("DNS / A RECORDS", 24, 160, 28, C.paper, 420)
    U.text("RESOLVER / GOOGLE", 480, 168, 14, C.muted, 216)
    if #state.dns_results == 0 then
        U.empty("RESOLVE FIVE DOMAINS", "Press A to query Google DNS for IPv4 records. Results arrive independently; use Up/Down to select a domain.")
        return
    end
    for i, r in ipairs(state.dns_results) do
        local y = 206 + (i - 1) * 86
        local selected = i - 1 == state.dns_cursor
        local fg, secondary = U.row(24, y, 672, 78, selected)
        local label = r.status == "waiting" and "PENDING" or (r.status == "ok" and "RESOLVED" or r.status:upper())
        U.text(r.name, 40, y + 10, 18, fg, 460, true)
        U.text(label, 552, y + 12, 14, selected and fg or
            (r.status == "waiting" and C.gold or (r.status == "ok" and C.green or C.red)), 128, true)
        local detail
        if r.status == "waiting" then detail = "Waiting for the resolver"
        elseif #r.ips > 0 then detail = table.concat(r.ips, ", ")
        elseif r.status == "ok" then detail = "No A records returned"
        else detail = "Lookup failed / X to retry the batch" end
        for j, line in ipairs(U.wrap(detail, 632, 16, 2)) do
            U.text(line, 40, y + 38 + (j - 1) * 20, 16, secondary, 632)
        end
    end
end

local function draw_probe_tab()
    U.display("HTTP / ENDPOINT PROBES", 24, 160, 28, C.paper, 448)
    U.text("SIX HTTPS TARGETS", 506, 168, 14, C.muted, 190)
    if #state.probe_results == 0 then
        U.empty("CHECK SIX ENDPOINTS", "Press A to run asynchronous HTTP probes. Each result shows the response code and measured request time; Up/Down selects an endpoint.")
        return
    end
    for i, r in ipairs(state.probe_results) do
        local y = 206 + (i - 1) * 72
        local selected = i - 1 == state.probe_cursor
        local fg, secondary = U.row(24, y, 672, 66, selected)
        U.text(r.name, 40, y + 10, 18, fg, 256, true)
        local label = r.status == "waiting" and "PENDING" or (r.status == "up" and "REPLIED" or "FAILED")
        local color = selected and fg or (r.status == "waiting" and C.gold or (r.status == "up" and C.green or C.red))
        U.text(label, 328, y + 13, 14, color, 108, true)
        U.text(r.status == "waiting" and "--" or (r.code > 0 and "HTTP " .. r.code or "NO HTTP"),
            446, y + 13, 14, secondary, 108)
        U.text(r.time_ms and r.time_ms .. "ms" or "--ms", 574, y + 11, 16, fg, 106, true)
        U.text(r.url, 40, y + 40, 14, secondary, 640)
    end
end

function on_init()
    load_overview()
end

function on_update(dt)
    local dirty = poll_requests()
    if dirty then app.request_redraw() end
    return dirty
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end
    local old_tab = state.active_tab
    if button == "l1" then
        state.active_tab = state.active_tab > 1 and state.active_tab - 1 or #state.tabs
        app.request_redraw()
        return
    elseif button == "r1" then
        state.active_tab = state.active_tab < #state.tabs and state.active_tab + 1 or 1
        app.request_redraw()
        return
    end

    local dirty = false
    if old_tab == 1 then
        if button == "a" or button == "x" then dirty = load_overview() end
    elseif old_tab == 2 then
        if button == "a" and #state.headers_lines == 0 or button == "x" then
            dirty = load_headers()
        elseif button == "dpad_up" or button == "dpad_down" then
            local old = state.headers_scroll
            state.headers_scroll = math.max(0, math.min(state.headers_max_scroll,
                old + (button == "dpad_down" and 1 or -1)))
            dirty = old ~= state.headers_scroll
        end
    elseif old_tab == 3 then
        if button == "a" and #state.dns_results == 0 or button == "x" then
            dirty = load_dns()
        elseif button == "dpad_up" or button == "dpad_down" then
            local old = state.dns_cursor
            state.dns_cursor = math.max(0, math.min(math.max(0, #state.dns_results - 1),
                old + (button == "dpad_down" and 1 or -1)))
            dirty = old ~= state.dns_cursor
        end
    elseif old_tab == 4 then
        if button == "a" and #state.probe_results == 0 or button == "x" then
            dirty = load_probes()
        elseif button == "dpad_up" or button == "dpad_down" then
            local old = state.probe_cursor
            state.probe_cursor = math.max(0, math.min(math.max(0, #state.probe_results - 1),
                old + (button == "dpad_down" and 1 or -1)))
            dirty = old ~= state.probe_cursor
        end
    end
    if dirty then app.request_redraw() end
end

function on_render()
    draw_header()
    if state.active_tab == 1 then draw_overview()
    elseif state.active_tab == 2 then draw_headers_tab()
    elseif state.active_tab == 3 then draw_dns_tab()
    elseif state.active_tab == 4 then draw_probe_tab() end
    local hints = {{"L1/R1", "Tab"}}
    if state.active_tab == 1 then
        hints[#hints + 1] = {"A", "Lookup"}
        hints[#hints + 1] = {"X", "Refresh"}
    elseif state.active_tab == 2 then
        if #state.headers_lines == 0 then hints[#hints + 1] = {"A", "Run"} end
        hints[#hints + 1] = {"Up/Down", "Scroll"}
        hints[#hints + 1] = {"X", "Retry"}
    else
        local results = state.active_tab == 3 and state.dns_results or state.probe_results
        if #results == 0 then hints[#hints + 1] = {"A", "Run"} end
        hints[#hints + 1] = {"Up/Down", "Select"}
        hints[#hints + 1] = {"X", "Rerun"}
    end
    U.footer(hints)
end
