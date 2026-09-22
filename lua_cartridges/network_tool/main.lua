-- Net Tool for CartridgeOS (Lua)
-- Network diagnostics: public IP, headers, DNS lookup, HTTP probe

local state = {
    tabs = {"Overview", "Headers", "DNS", "Probe"},
    active_tab = 1,
    -- Overview
    public_ip = nil,
    geo = nil,
    loading = false,
    error_msg = "",
    -- Headers
    headers_lines = {},
    headers_scroll = 0,
    -- DNS
    dns_targets = {"google.com", "cloudflare.com", "github.com", "wikipedia.org", "amazon.com"},
    dns_results = {},
    dns_cursor = 0,
    dns_loading = false,
    -- HTTP Probe
    probe_targets = {
        {name = "Google", url = "https://www.google.com"},
        {name = "Cloudflare", url = "https://1.1.1.1"},
        {name = "GitHub", url = "https://github.com"},
        {name = "Wikipedia", url = "https://en.wikipedia.org"},
        {name = "HN", url = "https://news.ycombinator.com"},
        {name = "Reddit", url = "https://www.reddit.com"},
    },
    probe_results = {},
    probe_loading = false,
    probe_cursor = 0,
    headers_loading = false,
}

-- ── Drawing Helpers ──────────────────────────────────────────────────────────

local function draw_header()
    ui.header("NET TOOL")
end

local function draw_tab_bar()
    local y = 40
    ui.rect(0, y, 720, 30, {color=theme.bg_header, filled=true})
    local tx = 10
    for i, label in ipairs(state.tabs) do
        local is_active = (i == state.active_tab)
        local tw = screen.get_text_width(label, 12, is_active)
        local tab_w = tw + 16
        if is_active then
            ui.rect(tx, y + 4, tab_w, 22, {color=theme.accent, filled=true, radius=4})
            screen.draw_text(label, tx + 8, y + 7, {color={20, 20, 30}, size=12, bold=true})
        else
            ui.rect(tx, y + 4, tab_w, 22, {color=theme.card_bg, filled=true, radius=4})
            screen.draw_text(label, tx + 8, y + 7, {color=theme.text_dim, size=12})
        end
        tx = tx + tab_w + 6
    end
    screen.draw_line(0, y + 30, 720, y + 30, {color=theme.border})
end

local function draw_footer(hints)
    ui.footer(hints)
end

local function draw_card(x, y, w, h, label, value, value_color)
    ui.card(x, y, w, h, {bg=theme.card_bg, border=theme.card_border, radius=6})
    screen.draw_text(label, x + 12, y + 8, {color=theme.text_dim, size=12, bold=true})
    screen.draw_text(value or "—", x + 12, y + 28, {color=value_color or theme.text, size=14})
end

local function draw_loading(msg)
    local text = msg or "Loading..."
    local tw = screen.get_text_width(text, 16, false)
    screen.draw_text(text, (720 - tw) / 2, 360, {color=theme.text_dim, size=16})
end

local pending = {}
local function request(url, handler)
    local ok, id = pcall(http.get_async, url)
    if ok then pending[id] = handler
    else handler({ok=false, status=0, body=tostring(id), elapsed_ms=0}) end
end

local function poll_requests()
    for _, response in ipairs(http.poll()) do
        local handler = pending[response.id]
        if handler then
            pending[response.id] = nil
            handler(response)
        end
    end
end

-- ── Data Loading ─────────────────────────────────────────────────────────────

local function load_overview()
    if state.loading then return end
    state.loading = true
    state.error_msg = ""
    request("https://ipinfo.io/json", function(resp)
        local ok, data = pcall(json.decode, resp.body)
        if resp.ok and ok and type(data) == "table" and data.ip then
            state.public_ip = data.ip
            state.geo = {city=data.city or "", region=data.region or "", country=data.country or "",
                         org=data.org or "", timezone=data.timezone or "", loc=data.loc or ""}
        else state.error_msg = "Could not fetch IP info. X to retry." end
        state.loading = false
    end)
end

local function load_headers()
    if state.headers_loading then return end
    state.headers_loading = true
    state.headers_lines = {}
    state.headers_scroll = 0
    request("https://httpbin.org/headers", function(resp)
        local ok, data = pcall(json.decode, resp.body)
        if resp.ok and ok and type(data) == "table" and type(data.headers) == "table" then
            for k,v in pairs(data.headers) do
                state.headers_lines[#state.headers_lines+1] = {key=tostring(k), value=tostring(v)}
            end
            table.sort(state.headers_lines, function(a,b) return a.key < b.key end)
        end
        if #state.headers_lines == 0 then
            state.headers_lines = {{key="Unavailable", value="Could not fetch headers. X to retry."}}
        end
        state.headers_loading = false
    end)
end

local function load_dns()
    if state.dns_loading then return end
    state.dns_loading = true
    state.dns_results = {}
    state.dns_cursor = 0
    local remaining = #state.dns_targets
    for i,target in ipairs(state.dns_targets) do
        local result = {name=target, status="waiting", ips={}}
        state.dns_results[i] = result
        request("https://dns.google/resolve?name="..target.."&type=A", function(resp)
            local ok, data = pcall(json.decode, resp.body)
            result.status = "error"
            if resp.ok and ok and type(data) == "table" then
                result.status = data.Status == 0 and "ok" or ("code:"..tostring(data.Status))
                for _,answer in ipairs(type(data.Answer)=="table" and data.Answer or {}) do
                    if answer.type == 1 then result.ips[#result.ips+1] = tostring(answer.data) end
                end
            end
            remaining = remaining - 1
            state.dns_loading = remaining > 0
        end)
    end
end

local function load_probes()
    if state.probe_loading then return end
    state.probe_loading = true
    state.probe_results = {}
    state.probe_cursor = 0
    local remaining = #state.probe_targets
    for i,target in ipairs(state.probe_targets) do
        local result = {name=target.name, url=target.url, status="waiting", code=0, time_ms=nil}
        state.probe_results[i] = result
        request(target.url, function(resp)
            result.code = resp.status or 0
            result.status = resp.ok and "up" or "down"
            result.time_ms = math.floor(resp.elapsed_ms or 0)
            remaining = remaining - 1
            state.probe_loading = remaining > 0
        end)
    end
end

-- ── Screens ──────────────────────────────────────────────────────────────────

local function draw_overview()
    local y = 82
    if state.loading then
        draw_loading("Fetching IP info...")
        return
    end
    if state.error_msg ~= "" then
        screen.draw_text(state.error_msg, 20, y + 40, {color=theme.negative, size=14})
        return
    end

    -- Public IP (big)
    draw_card(12, y, 696, 54, "PUBLIC IP", state.public_ip, theme.accent)
    y = y + 62

    if state.geo then
        local g = state.geo
        -- Two columns
        local col_w = 340
        draw_card(12, y, col_w, 54, "LOCATION", (g.city ~= "" and (g.city .. ", ") or "") .. g.region .. " " .. g.country, theme.text)
        draw_card(12 + col_w + 16, y, col_w, 54, "TIMEZONE", g.timezone, theme.text)
        y = y + 62

        draw_card(12, y, 696, 54, "ISP / ORG", g.org, theme.text)
        y = y + 62

        draw_card(12, y, 696, 54, "COORDINATES", g.loc, theme.text_dim)
        y = y + 62
    end

    -- Visual separator
    y = y + 10
    screen.draw_text("IP lookup complete", 20, y, {color=theme.positive, size=13})
end

local function draw_headers_tab()
    local y = 82
    local lh = 22
    local visible = math.floor(580 / lh)

    if #state.headers_lines == 0 then
        draw_loading("Fetching request headers...")
        return
    end

    screen.draw_text("Your HTTP Request Headers", 12, y, {color=theme.text_dim, size=13, bold=true})
    y = y + 24

    local start = state.headers_scroll + 1
    local stop = math.min(start + visible - 1, #state.headers_lines)
    for i = start, stop do
        local h = state.headers_lines[i]
        screen.draw_text(h.key .. ":", 20, y, {color=theme.accent, size=13, bold=true})
        local kw = screen.get_text_width(h.key .. ": ", 13, true)
        screen.draw_text(h.value, 20 + kw, y, {color=theme.text, size=13, max_width=680 - kw})
        y = y + lh
    end
end

local function draw_dns_tab()
    local y = 82

    if state.dns_loading and #state.dns_results == 0 then
        draw_loading("Resolving DNS...")
        return
    end

    if #state.dns_results == 0 then
        draw_loading("Press A to run DNS lookup")
        return
    end

    screen.draw_text("DNS Resolution (via Google DNS)", 12, y, {color=theme.text_dim, size=13, bold=true})
    y = y + 28

    for i, r in ipairs(state.dns_results) do
        local is_sel = (i - 1 == state.dns_cursor)
        local card_h = 50
        local bg = is_sel and theme.card_highlight or theme.card_bg
        local border = is_sel and theme.accent or theme.card_border
        ui.card(12, y, 696, card_h, {bg=bg, border=border, radius=6})

        -- Domain name
        screen.draw_text(r.name, 24, y + 6, {color=theme.text, size=14, bold=true})

        -- Status
        local status_color = r.status == "ok" and theme.positive or theme.negative
        ui.pill(r.status:upper(), 620, y + 6, status_color.r, status_color.g, status_color.b, {text_color={20,20,30}, size=10})

        -- IPs
        local ip_str = #r.ips > 0 and table.concat(r.ips, ", ") or "No A records"
        screen.draw_text(ip_str, 24, y + 28, {color=theme.text_dim, size=12})

        y = y + card_h + 6
    end
end

local function draw_probe_tab()
    local y = 82

    if state.probe_loading and #state.probe_results == 0 then
        draw_loading("Probing endpoints...")
        return
    end

    if #state.probe_results == 0 then
        draw_loading("Press A to run HTTP probes")
        return
    end

    screen.draw_text("HTTP Endpoint Probes", 12, y, {color=theme.text_dim, size=13, bold=true})
    y = y + 28

    for i, r in ipairs(state.probe_results) do
        local is_sel = (i - 1 == state.probe_cursor)
        local card_h = 50
        local bg = is_sel and theme.card_highlight or theme.card_bg
        local border = is_sel and theme.accent or theme.card_border
        ui.card(12, y, 696, card_h, {bg=bg, border=border, radius=6})

        -- Name
        screen.draw_text(r.name, 24, y + 6, {color=theme.text, size=14, bold=true})

        -- Status pill
        local status_color = r.status == "up" and theme.positive or theme.negative
        ui.pill(r.status:upper(), 580, y + 6, status_color.r, status_color.g, status_color.b, {text_color={20,20,30}, size=10})

        -- Response time
        local time_str = r.time_ms and (r.time_ms .. "ms") or "--"
        local time_color = not r.time_ms and theme.text_dim or r.time_ms < 500 and theme.positive or (r.time_ms < 2000 and theme.text_warning or theme.negative)
        ui.pill(time_str, 630, y + 6, time_color.r, time_color.g, time_color.b, {text_color={20,20,30}, size=10})

        -- URL + HTTP code
        local meta = r.url .. "  [" .. r.code .. "]"
        screen.draw_text(meta, 24, y + 28, {color=theme.text_dim, size=12, max_width=660})

        y = y + card_h + 6
    end
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    load_overview()
end

function on_update(dt)
    poll_requests()
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    -- Tab switching
    if button == "l1" then
        state.active_tab = state.active_tab > 1 and (state.active_tab - 1) or #state.tabs
        return
    elseif button == "r1" then
        state.active_tab = state.active_tab < #state.tabs and (state.active_tab + 1) or 1
        return
    end

    local tab = state.active_tab

    if tab == 1 then
        -- Overview: A=refresh
        if button == "a" or button == "x" then
            load_overview()
        end
    elseif tab == 2 then
        -- Headers
        if button == "a" and #state.headers_lines == 0 then
            load_headers()
        elseif button == "x" then
            load_headers()
        elseif button == "dpad_up" then
            state.headers_scroll = math.max(0, state.headers_scroll - 1)
        elseif button == "dpad_down" then
            state.headers_scroll = state.headers_scroll + 1
        end
    elseif tab == 3 then
        -- DNS
        if button == "a" and #state.dns_results == 0 then
            load_dns()
        elseif button == "x" then
            load_dns()
        elseif button == "dpad_up" then
            state.dns_cursor = math.max(0, state.dns_cursor - 1)
        elseif button == "dpad_down" then
            state.dns_cursor = math.min(math.max(0, #state.dns_results - 1), state.dns_cursor + 1)
        end
    elseif tab == 4 then
        -- Probe
        if button == "a" and #state.probe_results == 0 then
            load_probes()
        elseif button == "x" then
            load_probes()
        elseif button == "dpad_up" then
            state.probe_cursor = math.max(0, state.probe_cursor - 1)
        elseif button == "dpad_down" then
            state.probe_cursor = math.min(math.max(0, #state.probe_results - 1), state.probe_cursor + 1)
        end
    end
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)


    draw_header()
    draw_tab_bar()

    if state.active_tab == 1 then
        draw_overview()
    elseif state.active_tab == 2 then
        draw_headers_tab()
    elseif state.active_tab == 3 then
        draw_dns_tab()
    elseif state.active_tab == 4 then
        draw_probe_tab()
    end

    draw_footer({
        {"L1/R1", "Tab", theme.btn_l},
        {"A", "Run", theme.btn_a},
        {"X", "Refresh", theme.btn_x},
    })
end
