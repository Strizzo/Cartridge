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
    -- Deferred loading
    needs_initial_load = true,
    ready_to_load = false,
}

-- ── Drawing Helpers ──────────────────────────────────────────────────────────

local function draw_header()
    screen.draw_gradient_rect(0, 0, 720, 40,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 720, 0, {color=theme.accent})
    screen.draw_text("Net Tool", 12, 10, {color=theme.text, size=20, bold=true})
end

local function draw_tab_bar()
    local y = 40
    screen.draw_rect(0, y, 720, 30, {color=theme.bg_header, filled=true})
    local tx = 10
    for i, label in ipairs(state.tabs) do
        local is_active = (i == state.active_tab)
        local tw = screen.get_text_width(label, 12, is_active)
        local tab_w = tw + 16
        if is_active then
            screen.draw_rect(tx, y + 4, tab_w, 22, {color=theme.accent, filled=true, radius=4})
            screen.draw_text(label, tx + 8, y + 7, {color={20, 20, 30}, size=12, bold=true})
        else
            screen.draw_rect(tx, y + 4, tab_w, 22, {color=theme.card_bg, filled=true, radius=4})
            screen.draw_text(label, tx + 8, y + 7, {color=theme.text_dim, size=12})
        end
        tx = tx + tab_w + 6
    end
    screen.draw_line(0, y + 30, 720, y + 30, {color=theme.border})
end

local function draw_footer(hints)
    screen.draw_rect(0, 684, 720, 36, {color=theme.bg_header, filled=true})
    screen.draw_line(0, 684, 720, 684, {color=theme.border})
    local x = 10
    for _, h in ipairs(hints) do
        local w = screen.draw_button_hint(h[1], h[2], x, 692, {color=h[3], size=12})
        x = x + w + 14
    end
end

local function draw_card(x, y, w, h, label, value, value_color)
    screen.draw_card(x, y, w, h, {bg=theme.card_bg, border=theme.card_border, radius=6})
    screen.draw_text(label, x + 12, y + 8, {color=theme.text_dim, size=12, bold=true})
    screen.draw_text(value or "—", x + 12, y + 28, {color=value_color or theme.text, size=14})
end

local function draw_loading(msg)
    local text = msg or "Loading..."
    local tw = screen.get_text_width(text, 16, false)
    screen.draw_text(text, (720 - tw) / 2, 360, {color=theme.text_dim, size=16})
end

-- ── Data Loading ─────────────────────────────────────────────────────────────

local function load_overview()
    state.loading = true
    state.error_msg = ""

    -- Public IP via ipinfo.io
    local ok, resp = pcall(http.get_cached, "https://ipinfo.io/json", 60)
    if ok and resp.ok then
        local dok, data = pcall(json.decode, resp.body)
        if dok and data then
            state.public_ip = data.ip or "Unknown"
            state.geo = {
                city = data.city or "",
                region = data.region or "",
                country = data.country or "",
                org = data.org or "",
                timezone = data.timezone or "",
                loc = data.loc or "",
            }
        else
            state.error_msg = "Failed to parse IP data"
        end
    else
        state.error_msg = "Failed to fetch IP info"
    end

    state.loading = false
end

local function load_headers()
    state.headers_lines = {}
    local ok, resp = pcall(http.get, "https://httpbin.org/headers")
    if ok and resp.ok then
        local dok, data = pcall(json.decode, resp.body)
        if dok and data and data.headers then
            for k, v in pairs(data.headers) do
                state.headers_lines[#state.headers_lines + 1] = {key = k, value = v}
            end
            table.sort(state.headers_lines, function(a, b) return a.key < b.key end)
        end
    end
    if #state.headers_lines == 0 then
        state.headers_lines[#state.headers_lines + 1] = {key = "Error", value = "Could not fetch headers"}
    end
end

local function load_dns()
    state.dns_loading = true
    state.dns_results = {}
    for _, target in ipairs(state.dns_targets) do
        local ok, resp = pcall(http.get_cached, "https://dns.google/resolve?name=" .. target .. "&type=A", 120)
        local result = {name = target, status = "error", ips = {}}
        if ok and resp.ok then
            local dok, data = pcall(json.decode, resp.body)
            if dok and data then
                result.status = (data.Status == 0) and "ok" or ("code:" .. tostring(data.Status))
                if data.Answer then
                    for _, ans in ipairs(data.Answer) do
                        if ans.type == 1 then  -- A record
                            result.ips[#result.ips + 1] = ans.data
                        end
                    end
                end
            end
        end
        state.dns_results[#state.dns_results + 1] = result
    end
    state.dns_loading = false
end

local function load_probes()
    state.probe_loading = true
    state.probe_results = {}
    for _, target in ipairs(state.probe_targets) do
        local start_t = os.clock()
        local ok, resp = pcall(http.get, target.url)
        local elapsed = math.floor((os.clock() - start_t) * 1000)
        local result = {
            name = target.name,
            url = target.url,
            status = "error",
            code = 0,
            time_ms = elapsed,
        }
        if ok and resp then
            result.code = resp.status or 0
            result.status = resp.ok and "up" or "down"
        end
        state.probe_results[#state.probe_results + 1] = result
    end
    state.probe_loading = false
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
    screen.draw_text("Network connectivity appears normal", 20, y, {color=theme.positive, size=13})
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

    if state.dns_loading then
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
        screen.draw_card(12, y, 696, card_h, {bg=bg, border=border, radius=6})

        -- Domain name
        screen.draw_text(r.name, 24, y + 6, {color=theme.text, size=14, bold=true})

        -- Status
        local status_color = r.status == "ok" and theme.positive or theme.negative
        screen.draw_pill(r.status:upper(), 620, y + 6, status_color.r, status_color.g, status_color.b, {text_color={20,20,30}, size=10})

        -- IPs
        local ip_str = #r.ips > 0 and table.concat(r.ips, ", ") or "No A records"
        screen.draw_text(ip_str, 24, y + 28, {color=theme.text_dim, size=12})

        y = y + card_h + 6
    end
end

local function draw_probe_tab()
    local y = 82

    if state.probe_loading then
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
        screen.draw_card(12, y, 696, card_h, {bg=bg, border=border, radius=6})

        -- Name
        screen.draw_text(r.name, 24, y + 6, {color=theme.text, size=14, bold=true})

        -- Status pill
        local status_color = r.status == "up" and theme.positive or theme.negative
        screen.draw_pill(r.status:upper(), 580, y + 6, status_color.r, status_color.g, status_color.b, {text_color={20,20,30}, size=10})

        -- Response time
        local time_str = r.time_ms .. "ms"
        local time_color = r.time_ms < 500 and theme.positive or (r.time_ms < 2000 and theme.text_warning or theme.negative)
        screen.draw_pill(time_str, 630, y + 6, time_color.r, time_color.g, time_color.b, {text_color={20,20,30}, size=10})

        -- URL + HTTP code
        local meta = r.url .. "  [" .. r.code .. "]"
        screen.draw_text(meta, 24, y + 28, {color=theme.text_dim, size=12, max_width=660})

        y = y + card_h + 6
    end
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    state.loading = true
end

function on_update(dt)
    if state.ready_to_load then
        state.ready_to_load = false
        load_overview()
    end
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
            state.headers_lines = {}
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
            state.dns_results = {}
            load_dns()
        elseif button == "dpad_up" then
            state.dns_cursor = math.max(0, state.dns_cursor - 1)
        elseif button == "dpad_down" then
            state.dns_cursor = math.min(#state.dns_results - 1, state.dns_cursor + 1)
        end
    elseif tab == 4 then
        -- Probe
        if button == "a" and #state.probe_results == 0 then
            load_probes()
        elseif button == "x" then
            state.probe_results = {}
            load_probes()
        elseif button == "dpad_up" then
            state.probe_cursor = math.max(0, state.probe_cursor - 1)
        elseif button == "dpad_down" then
            state.probe_cursor = math.min(#state.probe_results - 1, state.probe_cursor + 1)
        end
    end
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    if state.needs_initial_load then
        state.needs_initial_load = false
        state.ready_to_load = true
    end

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
