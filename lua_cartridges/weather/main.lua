-- Weather App for Cartridge OS (Lua)
-- Current weather and 5-day forecast using Open-Meteo API

local API_BASE = "https://api.open-meteo.com/v1/forecast"
local CACHE_TTL = 300

-- ── Cities ───────────────────────────────────────────────────────────────────

local CITIES = {
    {key="new_york", name="New York", country="US", lat=40.7128, lon=-74.0060},
    {key="london", name="London", country="UK", lat=51.5074, lon=-0.1278},
    {key="tokyo", name="Tokyo", country="JP", lat=35.6762, lon=139.6503},
    {key="sydney", name="Sydney", country="AU", lat=-33.8688, lon=151.2093},
    {key="paris", name="Paris", country="FR", lat=48.8566, lon=2.3522},
    {key="berlin", name="Berlin", country="DE", lat=52.5200, lon=13.4050},
    {key="sao_paulo", name="Sao Paulo", country="BR", lat=-23.5505, lon=-46.6333},
    {key="mumbai", name="Mumbai", country="IN", lat=19.0760, lon=72.8777},
}

local WEEKDAYS = {"Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"}

-- ── Weather condition mapping ────────────────────────────────────────────────

local function condition_from_code(code)
    if code == 0 then
        return {label="Clear Sky", color={255, 220, 80}, icon={
            "    \\   |   /",
            "      .---.",
            "  ---( O  )---",
            "      '---'",
            "    /   |   \\",
        }}
    elseif code >= 1 and code <= 3 then
        return {label=code <= 2 and "Partly Cloudy" or "Overcast", color={180, 200, 220}, icon={
            "   \\  /",
            " _ /''.-.",
            "   \\_(   ).",
            "   /(___(__)",
            "",
        }}
    elseif code == 45 or code == 48 then
        return {label="Fog", color={160, 160, 180}, icon={
            " _ - _ - _ -",
            "  _ - _ - _",
            " _ - _ - _ -",
            "  _ - _ - _",
            " _ - _ - _ -",
        }}
    elseif code >= 51 and code <= 55 then
        return {label="Drizzle", color={120, 180, 240}, icon={
            "    .---.",
            "   (     ).",
            "  (______(_)",
            "   ' ' ' '",
            "  ' ' ' '",
        }}
    elseif code >= 61 and code <= 65 then
        return {label="Rain", color={80, 150, 255}, icon={
            "    .---.",
            "   (     ).",
            "  (______(_)",
            "  | | | | |",
            "  | | | | |",
        }}
    elseif code >= 71 and code <= 77 then
        return {label="Snow", color={220, 230, 255}, icon={
            "    .---.",
            "   (     ).",
            "  (______(_)",
            "  * * * * *",
            "   * * * *",
        }}
    elseif code >= 80 and code <= 82 then
        return {label="Showers", color={80, 140, 240}, icon={
            "    .---.",
            "   (     ).",
            "  (______(_)",
            "  /|/|/|/|",
            "  |/|/|/|/",
        }}
    elseif code >= 95 then
        return {label="Thunderstorm", color={200, 180, 60}, icon={
            "    .---.",
            "   (     ).",
            "  (______(_)",
            "    / / /",
            "   / / /",
        }}
    end
    return {label="Unknown", color={180, 180, 180}, icon={"?????","?   ?","    ??","   ?","   ."}}
end

local function temp_color(temp_c)
    if temp_c <= -10 then return {100, 160, 255}
    elseif temp_c <= 0 then return {120, 190, 255}
    elseif temp_c <= 10 then return {140, 210, 230}
    elseif temp_c <= 20 then return {200, 220, 140}
    elseif temp_c <= 30 then return {255, 200, 80}
    elseif temp_c <= 35 then return {255, 150, 60}
    else return {255, 90, 60}
    end
end

local function format_time(iso)
    if not iso or iso == "" then return "--:--" end
    local t = iso:match("T(%d+:%d+)")
    return t or iso:sub(1, 5)
end

-- ── State ────────────────────────────────────────────────────────────────────

local state = {
    tab_index = 1,  -- 1=current, 2=forecast, 3=settings
    city_idx = 1,
    -- Current weather
    current = nil,
    current_loading = true,
    current_error = false,
    -- Forecast
    forecast_days = {},
    forecast_loading = true,
    forecast_error = false,
    forecast_cursor = 1,
    forecast_scroll = 0,
    -- Settings
    settings_cursor = 1,
    settings_scroll = 0,
    -- Animation
    tick = 0,
}

-- ── Drawing Helpers ──────────────────────────────────────────────────────────

local function draw_header(title, right_text, right_color)
    screen.draw_gradient_rect(0, 0, 720, 40,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 720, 0, {color=theme.accent})
    screen.draw_text(title, 12, 10, {color=theme.text, size=20, bold=true})
    if right_text then
        local rc = right_color or theme.text_dim
        local rw = screen.get_text_width(right_text, 12, false)
        screen.draw_text(right_text, 704 - rw, 14, {color=rc, size=12})
    end
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

local function draw_tab_indicator(active)
    local tab_y = 42
    local tab_w = 240
    local labels = {"Current", "Forecast", "Settings"}
    for i, label in ipairs(labels) do
        local tx = (i - 1) * tab_w
        local is_active = (i == active)
        local col = is_active and theme.text or theme.text_dim
        local tw = screen.get_text_width(label, 12, is_active)
        local surface_x = tx + (tab_w - tw) / 2
        screen.draw_text(label, surface_x, tab_y + 4, {color=col, size=12, bold=is_active})
        if is_active then
            screen.draw_rect(tx + 20, tab_y + 24, tab_w - 40, 2, {color=theme.accent, filled=true, radius=1})
        end
    end
    screen.draw_line(0, tab_y + 28, 720, tab_y + 28, {color=theme.border})
end

-- ── API Functions ────────────────────────────────────────────────────────────

local function fetch_current()
    local city = CITIES[state.city_idx]
    state.current_loading = true
    state.current_error = false

    local url = API_BASE
        .. "?latitude=" .. city.lat .. "&longitude=" .. city.lon
        .. "&current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m,surface_pressure"
        .. "&hourly=temperature_2m"
        .. "&daily=sunrise,sunset"
        .. "&timezone=auto&forecast_days=1"

    local net_ok, resp = pcall(http.get_cached, url, CACHE_TTL)
    if net_ok and resp.ok then
        local dok, data = pcall(json.decode, resp.body)
        if dok and data and data.current then
            local cur = data.current
            local hourly = data.hourly and data.hourly.temperature_2m or {}
            local daily = data.daily or {}

            local sunrise_raw = ""
            local sunset_raw = ""
            if daily.sunrise and #daily.sunrise > 0 then sunrise_raw = daily.sunrise[1] end
            if daily.sunset and #daily.sunset > 0 then sunset_raw = daily.sunset[1] end

            state.current = {
                temperature = cur.temperature_2m,
                feels_like = cur.apparent_temperature,
                humidity = cur.relative_humidity_2m,
                wind_speed = cur.wind_speed_10m,
                pressure = cur.surface_pressure,
                weather_code = cur.weather_code,
                hourly_temps = hourly,
                sunrise = format_time(sunrise_raw),
                sunset = format_time(sunset_raw),
            }
        else
            state.current_error = true
        end
    else
        state.current_error = true
    end
    state.current_loading = false
end

local function fetch_forecast()
    local city = CITIES[state.city_idx]
    state.forecast_loading = true
    state.forecast_error = false

    local url = API_BASE
        .. "?latitude=" .. city.lat .. "&longitude=" .. city.lon
        .. "&daily=weather_code,temperature_2m_max,temperature_2m_min,apparent_temperature_max,apparent_temperature_min,sunrise,sunset,precipitation_sum,wind_speed_10m_max"
        .. "&timezone=auto"

    local net_ok, resp = pcall(http.get_cached, url, CACHE_TTL)
    if net_ok and resp.ok then
        local dok, data = pcall(json.decode, resp.body)
        if not dok or not data or not data.daily then
            state.forecast_error = true
            state.forecast_loading = false
            return
        end
        local daily = data.daily
        state.forecast_days = {}

        local count = math.min(5, #daily.time)
        for i = 1, count do
            local date_str = daily.time[i]
            -- Parse weekday from date string (YYYY-MM-DD)
            local year, month, day = date_str:match("(%d+)-(%d+)-(%d+)")
            local weekday = "???"
            if year then
                local t = os.time({year=tonumber(year), month=tonumber(month), day=tonumber(day)})
                local wday = tonumber(os.date("%w", t))  -- 0=Sunday
                local wday_map = {[0]="Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"}
                weekday = wday_map[wday] or "???"
            end

            local sunrise_str = ""
            local sunset_str = ""
            if daily.sunrise and i <= #daily.sunrise then sunrise_str = format_time(daily.sunrise[i]) end
            if daily.sunset and i <= #daily.sunset then sunset_str = format_time(daily.sunset[i]) end

            state.forecast_days[#state.forecast_days + 1] = {
                date = date_str,
                weekday = weekday,
                temp_max = daily.temperature_2m_max[i],
                temp_min = daily.temperature_2m_min[i],
                weather_code = daily.weather_code[i],
                precipitation = (daily.precipitation_sum and daily.precipitation_sum[i]) or 0,
                wind_max = (daily.wind_speed_10m_max and daily.wind_speed_10m_max[i]) or 0,
                sunrise = sunrise_str,
                sunset = sunset_str,
            }
        end
    else
        state.forecast_error = true
    end
    state.forecast_loading = false
end

local function load_all()
    fetch_current()
    fetch_forecast()
end

-- ── Current Weather Screen ───────────────────────────────────────────────────

local function draw_current_screen()
    local city = CITIES[state.city_idx]
    draw_header("Weather",
        state.current_loading and "Loading" or (state.current_error and "Error" or "Updated"),
        state.current_loading and theme.text_dim or (state.current_error and theme.negative or {100, 220, 100}))
    draw_tab_indicator(1)

    if state.current_loading and not state.current then
        local tw = screen.get_text_width("Fetching weather...", 16, false)
        screen.draw_text("Fetching weather...", (720 - tw) / 2, 220, {color=theme.text_dim, size=16})
        draw_footer({{"L1/R1", "Tab", theme.btn_l}})
        return
    end

    if not state.current then
        local tw = screen.get_text_width("No data", 16, false)
        screen.draw_text("No data", (720 - tw) / 2, 220, {color=theme.text_dim, size=16})
        draw_footer({{"L1/R1", "Tab", theme.btn_l}})
        return
    end

    local w = state.current
    local cond = condition_from_code(w.weather_code)
    local content_y = 76

    -- City name
    screen.draw_text(city.name .. ", " .. city.country, 20, content_y, {color=theme.text_dim, size=13})
    content_y = content_y + 22

    -- Main temperature + condition card
    screen.draw_card(6, content_y, 708, 150, {bg=theme.card_bg, border=theme.border, radius=10, shadow=true})

    -- Large temperature
    local tc = temp_color(w.temperature)
    local temp_str = string.format("%+.0f", w.temperature)
    screen.draw_text(temp_str, 30, content_y + 12, {color=tc, size=24, bold=true})
    local deg_x = 30 + screen.get_text_width(temp_str, 24, true) + 4
    screen.draw_text("\194\176C", deg_x, content_y + 14, {color=tc, size=16, bold=true})

    -- Feels like
    screen.draw_text(string.format("Feels like %+.0f\194\176", w.feels_like), 30, content_y + 48, {color=theme.text_dim, size=13})

    -- Condition label pill
    screen.draw_pill(cond.label, 30, content_y + 72,
        cond.color[1], cond.color[2], cond.color[3], {text_color={20,20,30}, size=11})

    -- Sunrise / Sunset
    screen.draw_text("Sunrise " .. w.sunrise .. "   Sunset " .. w.sunset, 30, content_y + 98, {color=theme.text_dim, size=11})

    -- ASCII weather art (right side)
    local art_x = 440
    local art_y = content_y + 16
    for i, line in ipairs(cond.icon) do
        if line and line ~= "" then
            screen.draw_text(line, art_x, art_y + (i - 1) * 18, {color=cond.color, size=14})
        end
    end

    content_y = content_y + 160

    -- Stats cards row
    local stats = {
        {"Humidity", string.format("%.0f%%", w.humidity), {100, 180, 255}},
        {"Wind", string.format("%.0f km/h", w.wind_speed), {120, 220, 180}},
        {"Pressure", string.format("%.0f hPa", w.pressure), {200, 180, 255}},
    }
    local card_w = 220
    local gap = 14
    local start_x = (720 - (card_w * 3 + gap * 2)) / 2
    for i, stat in ipairs(stats) do
        local cx = start_x + (i - 1) * (card_w + gap)
        screen.draw_card(cx, content_y, card_w, 62, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})
        screen.draw_text(stat[1], cx + 12, content_y + 8, {color=theme.text_dim, size=11})
        screen.draw_text(stat[2], cx + 12, content_y + 28, {color=stat[3], size=16, bold=true})
    end
    content_y = content_y + 72

    -- 24h Sparkline card
    if w.hourly_temps and #w.hourly_temps > 0 then
        screen.draw_card(6, content_y, 708, 80, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})
        screen.draw_text("24h Temperature Trend", 18, content_y + 6, {color=theme.text_dim, size=11})
        screen.draw_sparkline(w.hourly_temps, 18, content_y + 24, 684, 46, {color=theme.accent})

        if #w.hourly_temps > 1 then
            local lo = w.hourly_temps[1]
            local hi = w.hourly_temps[1]
            for _, t in ipairs(w.hourly_temps) do
                if t < lo then lo = t end
                if t > hi then hi = t end
            end
            screen.draw_text(string.format("%+.0f\194\176", lo), 632, content_y + 6, {color={120, 190, 255}, size=11})
            screen.draw_text(string.format("%+.0f\194\176", hi), 672, content_y + 6, {color={255, 180, 80}, size=11})
        end
    end

    draw_footer({{"L1/R1", "Tab", theme.btn_l}})
end

-- ── Forecast Screen ──────────────────────────────────────────────────────────

local function draw_forecast_screen()
    local city = CITIES[state.city_idx]
    draw_header("Forecast",
        state.forecast_loading and "Loading" or (state.forecast_error and "Error" or "Updated"),
        state.forecast_loading and theme.text_dim or (state.forecast_error and theme.negative or {100, 220, 100}))
    draw_tab_indicator(2)

    local content_top = 76
    local content_bottom = 684
    local row_height = 74
    local visible_rows = math.floor((content_bottom - content_top) / row_height)

    if state.forecast_loading and #state.forecast_days == 0 then
        local tw = screen.get_text_width("Fetching forecast...", 16, false)
        screen.draw_text("Fetching forecast...", (720 - tw) / 2, 220, {color=theme.text_dim, size=16})
        draw_footer({{"L1/R1", "Tab", theme.btn_l}})
        return
    end

    if #state.forecast_days == 0 then
        local tw = screen.get_text_width("No forecast data", 16, false)
        screen.draw_text("No forecast data", (720 - tw) / 2, 220, {color=theme.text_dim, size=16})
        draw_footer({{"L1/R1", "Tab", theme.btn_l}})
        return
    end

    local days = state.forecast_days
    screen.draw_text(city.name .. " \226\128\148 5 Day Forecast", 20, content_top - 2, {color=theme.text_dim, size=12})

    -- Ensure visibility
    if state.forecast_cursor < state.forecast_scroll + 1 then
        state.forecast_scroll = state.forecast_cursor - 1
    elseif state.forecast_cursor > state.forecast_scroll + visible_rows then
        state.forecast_scroll = state.forecast_cursor - visible_rows
    end

    for vi = 0, visible_rows do
        local idx = state.forecast_scroll + vi + 1
        if idx > #days then break end
        local y = content_top + 18 + vi * row_height
        if y + row_height - 4 > content_bottom then break end

        local day = days[idx]
        local selected = (idx == state.forecast_cursor)
        local card_x = 6
        local card_w = 708
        local card_h = row_height - 4

        if selected then
            screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_highlight, border=theme.accent, radius=8})
        else
            screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_bg, radius=8})
        end

        local cond = condition_from_code(day.weather_code)

        -- Day name + date
        screen.draw_text(day.weekday, card_x + 14, y + 10, {color=theme.text, size=14, bold=true})
        screen.draw_text(day.date, card_x + 14, y + 32, {color=theme.text_dim, size=11})

        -- Condition pill
        screen.draw_pill(cond.label, card_x + 14, y + 50,
            cond.color[1], cond.color[2], cond.color[3], {text_color={20,20,30}, size=10})

        -- Mini weather icon
        for i = 1, math.min(3, #cond.icon) do
            if cond.icon[i] and cond.icon[i] ~= "" then
                screen.draw_text(cond.icon[i], card_x + 200, y + 8 + (i - 1) * 14, {color=cond.color, size=10})
            end
        end

        -- High / Low temps
        local hi_color = temp_color(day.temp_max)
        local lo_color = temp_color(day.temp_min)
        screen.draw_text(string.format("%+.0f\194\176", day.temp_max), card_x + 430, y + 10, {color=hi_color, size=16, bold=true})
        screen.draw_text("Hi", card_x + 430, y + 34, {color=theme.text_dim, size=10})
        screen.draw_text(string.format("%+.0f\194\176", day.temp_min), card_x + 510, y + 10, {color=lo_color, size=16, bold=true})
        screen.draw_text("Lo", card_x + 510, y + 34, {color=theme.text_dim, size=10})

        -- Precipitation + wind
        if day.precipitation > 0 then
            screen.draw_text(string.format("%.1fmm", day.precipitation), card_x + 600, y + 10, {color={100, 180, 255}, size=12})
        else
            screen.draw_text("0mm", card_x + 600, y + 10, {color=theme.text_dim, size=12})
        end
        screen.draw_text(string.format("%.0fkm/h", day.wind_max), card_x + 600, y + 30, {color={120, 220, 180}, size=11})

        -- Sunrise / Sunset
        screen.draw_text("\226\134\145" .. day.sunrise .. " \226\134\147" .. day.sunset, card_x + 580, y + 50, {color=theme.text_dim, size=10})
    end

    draw_footer({
        {"L1/R1", "Tab", theme.btn_l},
        {"\226\134\145\226\134\147", "Scroll", theme.btn_a},
    })
end

-- ── Settings Screen ──────────────────────────────────────────────────────────

local function draw_settings_screen()
    draw_header("Settings")
    draw_tab_indicator(3)

    local content_top = 76
    local content_bottom = 684
    local row_height = 46
    local visible_rows = math.floor((content_bottom - content_top) / row_height)

    screen.draw_text("Select a city", 20, content_top - 2, {color=theme.text_dim, size=12})

    -- Ensure visibility
    if state.settings_cursor < state.settings_scroll + 1 then
        state.settings_scroll = state.settings_cursor - 1
    elseif state.settings_cursor > state.settings_scroll + visible_rows then
        state.settings_scroll = state.settings_cursor - visible_rows
    end

    for vi = 0, visible_rows do
        local idx = state.settings_scroll + vi + 1
        if idx > #CITIES then break end
        local y = content_top + 18 + vi * row_height
        if y + row_height - 4 > content_bottom then break end

        local city = CITIES[idx]
        local is_selected = (idx == state.settings_cursor)
        local is_current = (idx == state.city_idx)

        local card_x = 6
        local card_w = 708
        local card_h = row_height - 4

        if is_selected then
            screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_highlight, border=theme.accent, radius=6})
        else
            screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_bg, radius=6})
        end

        screen.draw_text(city.name, card_x + 16, y + 6, {color=theme.text, size=14, bold=is_selected})
        screen.draw_text(city.country .. "  (" .. string.format("%.2f", city.lat) .. ", " .. string.format("%.2f", city.lon) .. ")",
            card_x + 16, y + 26, {color=theme.text_dim, size=11})

        if is_current then
            screen.draw_pill("Active", card_x + card_w - 80, y + 12,
                theme.positive.r, theme.positive.g, theme.positive.b, {text_color={20,20,30}, size=10})
        end
    end

    -- Scroll indicator
    if #CITIES > visible_rows then
        local bar_x = 714
        local bar_y = content_top + 18
        local bar_h = content_bottom - bar_y - 4
        local thumb_h = math.max(16, math.floor(bar_h * visible_rows / #CITIES))
        local max_scroll = math.max(1, #CITIES - visible_rows)
        local thumb_y = bar_y + math.floor((bar_h - thumb_h) * state.settings_scroll / max_scroll)
        screen.draw_rect(bar_x, bar_y, 4, bar_h, {color=theme.border, filled=true, radius=2})
        screen.draw_rect(bar_x, thumb_y, 4, thumb_h, {color=theme.accent, filled=true, radius=2})
    end

    draw_footer({
        {"L1/R1", "Tab", theme.btn_l},
        {"\226\134\145\226\134\147", "Navigate", theme.btn_a},
        {"A", "Select", theme.btn_a},
    })
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    -- Load saved city
    local saved = storage.load("settings")
    if saved and saved.city then
        for i, c in ipairs(CITIES) do
            if c.key == saved.city then
                state.city_idx = i
                break
            end
        end
    end
    -- Init settings cursor to current city
    state.settings_cursor = state.city_idx

    load_all()
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    -- Tab switching (all screens)
    if button == "l1" and action == "press" then
        state.tab_index = math.max(1, state.tab_index - 1)
        return
    elseif button == "r1" and action == "press" then
        state.tab_index = math.min(3, state.tab_index + 1)
        return
    end

    if state.tab_index == 1 then
        -- Current screen: no navigation
    elseif state.tab_index == 2 then
        -- Forecast screen
        if button == "dpad_up" then
            state.forecast_cursor = math.max(1, state.forecast_cursor - 1)
        elseif button == "dpad_down" then
            state.forecast_cursor = math.min(#state.forecast_days, state.forecast_cursor + 1)
        end
    elseif state.tab_index == 3 then
        -- Settings screen
        if button == "dpad_up" then
            state.settings_cursor = math.max(1, state.settings_cursor - 1)
        elseif button == "dpad_down" then
            state.settings_cursor = math.min(#CITIES, state.settings_cursor + 1)
        elseif button == "a" then
            local new_city_idx = state.settings_cursor
            if new_city_idx ~= state.city_idx then
                state.city_idx = new_city_idx
                storage.save("settings", {city=CITIES[new_city_idx].key})
                state.current = nil
                state.forecast_days = {}
                state.current_loading = true
                state.forecast_loading = true
                load_all()
            end
        end
    end
end

function on_update(dt)
    state.tick = state.tick + 1
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    if state.tab_index == 1 then
        draw_current_screen()
    elseif state.tab_index == 2 then
        draw_forecast_screen()
    elseif state.tab_index == 3 then
        draw_settings_screen()
    end
end
