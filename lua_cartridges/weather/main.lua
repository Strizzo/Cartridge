-- Weather App for Cartridge OS (Lua)
-- Current weather and 5-day forecast using Open-Meteo API

local U = ui.public
local C = U.C

local API_BASE = "https://api.open-meteo.com/v1/forecast"

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

-- WMO interpretation: https://open-meteo.com/en/docs#weathervariables
local function condition_from_code(code)
    local label, icon = "Unknown", "unknown"
    if code == 0 then label, icon = "Clear sky", "clear"
    elseif code == 1 then label, icon = "Mainly clear", "partly_cloudy"
    elseif code == 2 then label, icon = "Partly cloudy", "partly_cloudy"
    elseif code == 3 then label, icon = "Overcast", "cloudy"
    elseif code == 45 or code == 48 then label, icon = "Fog", "fog"
    elseif code == 51 or code == 53 or code == 55 then label, icon = "Drizzle", "drizzle"
    elseif code == 56 or code == 57 then label, icon = "Freezing drizzle", "drizzle"
    elseif code == 61 or code == 63 or code == 65 then label, icon = "Rain", "rain"
    elseif code == 66 or code == 67 then label, icon = "Freezing rain", "rain"
    elseif code == 71 or code == 73 or code == 75 then label, icon = "Snow", "snow"
    elseif code == 77 then label, icon = "Snow grains", "snow"
    elseif code == 80 or code == 81 or code == 82 then label, icon = "Showers", "showers"
    elseif code == 85 or code == 86 then label, icon = "Snow showers", "snow"
    elseif code == 95 then label, icon = "Thunderstorm", "storm"
    elseif code == 96 or code == 99 then label, icon = "Storm with hail", "storm" end
    return {label=label, icon="assets/conditions/"..icon..".png", color={242,237,228}}
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
    -- Async HTTP bookkeeping: request id -> handler(resp). Generations let
    -- a superseded load (city change) drop its late responses.
    pending = {},
    load_gen = 0,
}

-- ── API Functions (async) ────────────────────────────────────────────────────
-- Requests go through http.get_async; on_update drains http.poll() and
-- dispatches each response to the handler registered here.

local function request(url, handler)
    local ok, id = pcall(http.get_async, url)
    if ok and id then
        state.pending[id] = handler
        return id
    end
    handler(nil)
    return nil
end

local function decode(resp)
    if not resp or not resp.ok then return nil end
    local ok, data = pcall(json.decode, resp.body)
    if ok then return data end
    return nil
end

local function fetch_current()
    local city = CITIES[state.city_idx]
    state.current_loading = true
    state.current_error = false
    local gen = state.load_gen

    local url = API_BASE
        .. "?latitude=" .. city.lat .. "&longitude=" .. city.lon
        .. "&current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m,surface_pressure"
        .. "&hourly=temperature_2m"
        .. "&daily=sunrise,sunset"
        .. "&timezone=auto&forecast_days=1"

    request(url, function(resp)
        if gen ~= state.load_gen then return false end
        local data = decode(resp)
        if type(data) == "table" and type(data.current) == "table"
            and type(data.current.temperature_2m) == "number" then
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
        state.current_loading = false
        return true
    end)
end

local function fetch_forecast()
    local city = CITIES[state.city_idx]
    state.forecast_loading = true
    state.forecast_error = false
    local gen = state.load_gen

    local url = API_BASE
        .. "?latitude=" .. city.lat .. "&longitude=" .. city.lon
        .. "&daily=weather_code,temperature_2m_max,temperature_2m_min,apparent_temperature_max,apparent_temperature_min,sunrise,sunset,precipitation_sum,wind_speed_10m_max"
        .. "&timezone=auto"

    request(url, function(resp)
        if gen ~= state.load_gen then return false end
        local data = decode(resp)
        if type(data) ~= "table" or type(data.daily) ~= "table"
            or type(data.daily.time) ~= "table" then
            state.forecast_error = true
            state.forecast_loading = false
            return true
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
        state.forecast_cursor = math.max(1, math.min(state.forecast_cursor, #state.forecast_days))
        state.forecast_loading = false
        return true
    end)
end

local function load_all()
    -- New generation: responses from a previous city are ignored.
    state.load_gen = state.load_gen + 1
    fetch_current()
    fetch_forecast()
end

-- ── Public layouts ───────────────────────────────────────────────────────────

local function number(value, decimals)
    if type(value) ~= "number" then return "--" end
    return string.format("%." .. (decimals or 0) .. "f", value)
end

local function draw_weather_header(forecast)
    local loading = forecast and state.forecast_loading or (not forecast and state.current_loading)
    local failed = forecast and state.forecast_error or (not forecast and state.current_error)
    local status = loading and "FETCHING" or (failed and "RETRY X" or "OPEN-METEO")
    U.header("WEATHER", "CONDITIONS / FIVE DAY OUTLOOK", status,
        failed and C.red or (loading and C.gold or C.muted))
    U.tabs({"Current", "Forecast", "Cities"}, state.tab_index)
end

local function draw_current_screen()
    draw_weather_header(false)
    local city = CITIES[state.city_idx]
    U.display(city.name, 24, 160, 28, C.paper, 430)
    U.text(city.country .. " / CELSIUS", 510, 168, 14, C.muted, 186)

    local w = state.current
    if not w then
        if state.current_loading then
            U.empty("FETCHING CONDITIONS", "Requesting weather for " .. city.name .. ". You can browse the other tabs while it loads.")
        else
            U.empty("WEATHER UNAVAILABLE", "Could not retrieve current conditions for " .. city.name .. ". Press X to retry, or select another city.")
        end
        U.footer({{"L1/R1", "Tab"}, {"X", "Retry"}})
        return
    end

    local cond = condition_from_code(w.weather_code)
    U.panel(24, 196, 672, 174, C.panel, C.red)
    U.display(string.format("%+.0f°C", w.temperature), 40, 202, 92, C.paper, 430)
    U.text(cond.label, 42, 304, 18, C.paper, 440, true)
    U.text("Feels like " .. number(w.feels_like) .. "°C", 42, 338, 16, C.muted, 340)
    screen.draw_image(cond.icon, 526, 204, {w=136, h=136})
    U.text("RISE " .. w.sunrise .. " / SET " .. w.sunset, 430, 348, 14, C.muted, 248)

    local metrics = {{"HUMIDITY", w.humidity, "%", C.teal}, {"WIND", w.wind_speed, "km/h", C.paper},
        {"PRESSURE", w.pressure, "hPa", C.gold}}
    for i, metric in ipairs(metrics) do
        local x = 24 + (i - 1) * 228
        U.panel(x, 386, 216, 86, C.panel, metric[4])
        U.text(metric[1], x + 16, 398, 14, C.muted, 184, true)
        U.display(number(metric[2]), x + 16, 418, 38, C.paper, 120)
        U.text(metric[3], x + 148, 444, 14, C.muted, 52)
    end

    U.panel(24, 488, 672, 162, C.panel)
    U.text("TODAY / HOURLY TEMPERATURE", 40, 502, 14, C.muted, 360, true)
    local temps = w.hourly_temps or {}
    if #temps > 0 then
        local lo, hi = temps[1], temps[1]
        for _, t in ipairs(temps) do lo = math.min(lo, t); hi = math.max(hi, t) end
        U.text(number(lo) .. "° LOW / " .. number(hi) .. "° HIGH", 444, 502, 14, C.paper, 236)
        U.sparkline(temps, 40, 540, 640, 66, C.red, C.line)
        U.text("00:00", 40, 626, 14, C.muted, 100)
        U.text("LOCAL TIME", 300, 626, 14, C.muted, 160)
        U.text("23:00", 622, 626, 14, C.muted, 58)
    else
        U.text("Hourly temperatures were not supplied.", 40, 552, 18, C.muted, 640)
    end
    U.footer({{"L1/R1", "Tab"}, {"X", state.current_error and "Retry" or "Refresh"}})
end

local function draw_forecast_screen()
    draw_weather_header(true)
    local city = CITIES[state.city_idx]
    U.display(city.name, 24, 160, 28, C.paper, 430)
    U.text(state.forecast_error and "SAVED / RETRY X" or "FIVE DAY OUTLOOK", 500, 168, 14,
        state.forecast_error and C.gold or C.muted, 196)
    local days = state.forecast_days
    if #days == 0 then
        if state.forecast_loading then
            U.empty("FETCHING FORECAST", "Requesting the next five days for " .. city.name .. ".")
        else
            U.empty("FORECAST UNAVAILABLE", "No forecast was returned for " .. city.name .. ". Press X to retry, or choose another city.")
        end
        U.footer({{"L1/R1", "Tab"}, {"X", "Retry"}})
        return
    end

    for i, day in ipairs(days) do
        local y = 194 + (i - 1) * 62
        local fg, secondary = U.row(24, y, 672, 56, i == state.forecast_cursor)
        local cond = condition_from_code(day.weather_code)
        U.display(day.weekday:upper(), 40, y + 8, 30, fg, 88)
        U.text(day.date:sub(6), 136, y + 20, 16, secondary, 70)
        screen.draw_image(cond.icon, 220, y + 4, {w=48, h=48})
        U.text(cond.label, 282, y + 20, 16, fg, 214)
        U.text(number(day.temp_max) .. "° / " .. number(day.temp_min) .. "°", 520, y + 14, 22, fg, 162, true)
    end

    local day = days[state.forecast_cursor] or days[1]
    local cond = condition_from_code(day.weather_code)
    U.panel(24, 516, 672, 134, C.panel2, C.red)
    U.text(day.weekday:upper() .. " / " .. day.date, 40, 530, 14, C.muted, 310, true)
    U.text(cond.label, 40, 558, 18, C.paper, 304, true)
    U.text("Sunrise " .. day.sunrise .. " / Sunset " .. day.sunset, 40, 604, 14, C.muted, 330)
    U.text("PRECIPITATION", 400, 532, 14, C.muted, 276)
    U.text(number(day.precipitation, 1) .. " mm", 400, 554, 18, C.paper, 276, true)
    U.text("WIND / " .. number(day.wind_max) .. " km/h", 400, 604, 16, C.paper, 276)
    U.footer({{"L1/R1", "Tab"}, {"Up/Down", "Day"}, {"X", "Refresh"}})
end

local function draw_settings_screen()
    U.header("WEATHER", "CITY DIRECTORY", "SELECT A", C.muted)
    U.tabs({"Current", "Forecast", "Cities"}, 3)
    U.text("Choose a city", 24, 164, 18, C.paper, 330, true)
    U.text("ACTIVE / " .. CITIES[state.city_idx].name, 390, 166, 14, C.muted, 306)
    for i, city in ipairs(CITIES) do
        local y = 194 + (i - 1) * 56
        local fg, secondary = U.row(24, y, 672, 50, i == state.settings_cursor)
        U.text(city.name, 40, y + 5, 18, fg, 440, true)
        U.text(string.format("%.2f, %.2f", city.lat, city.lon), 40, y + 29, 14, secondary, 440)
        U.text(city.country, 530, y + 16, 16, secondary, 52)
        U.text(i == state.city_idx and "ACTIVE" or "", 600, y + 17, 14, fg, 80, true)
    end
    U.footer({{"L1/R1", "Tab"}, {"Up/Down", "City"}, {"A", "Select"}})
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
    state.settings_cursor = state.city_idx
    load_all()
end

function on_update(dt)
    local dirty = false
    for _, resp in ipairs(http.poll()) do
        local handler = state.pending[resp.id]
        if handler then
            state.pending[resp.id] = nil
            if handler(resp) ~= false then dirty = true end
        end
    end
    if dirty then app.request_redraw() end
    return dirty
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local old_tab, old_forecast, old_city = state.tab_index, state.forecast_cursor, state.settings_cursor

    if button == "x" and action == "press" and state.tab_index ~= 3 then
        if not state.current_loading and not state.forecast_loading then
            load_all()
            app.request_redraw()
        end
        return
    end

    -- Tab switching (all screens)
    if button == "l1" and action == "press" then
        state.tab_index = math.max(1, state.tab_index - 1)
        if old_tab ~= state.tab_index then app.request_redraw() end
        return
    elseif button == "r1" and action == "press" then
        state.tab_index = math.min(3, state.tab_index + 1)
        if old_tab ~= state.tab_index then app.request_redraw() end
        return
    end

    if state.tab_index == 1 then
        -- Current screen: no navigation
    elseif state.tab_index == 2 then
        -- Forecast screen
        if button == "dpad_up" then
            state.forecast_cursor = math.max(1, state.forecast_cursor - 1)
        elseif button == "dpad_down" then
            state.forecast_cursor = math.max(1, math.min(#state.forecast_days, state.forecast_cursor + 1))
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
                app.request_redraw()
            end
        end
    end
    if old_forecast ~= state.forecast_cursor or old_city ~= state.settings_cursor then
        app.request_redraw()
    end
end

function on_render()
    if state.tab_index == 1 then
        draw_current_screen()
    elseif state.tab_index == 2 then
        draw_forecast_screen()
    elseif state.tab_index == 3 then
        draw_settings_screen()
    end
end
