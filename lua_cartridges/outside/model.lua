-- Open-Meteo transport values stay in SI units. ISO labels stay in the
-- forecast's time zone; never run them through the device's os.time/date.
local M = {}
M.MAX_PLACES = 8
M.TTL = 1800
local function number(v)
    return type(v) == "number" and v == v and math.abs(v) < 1e8 and v or nil
end
M.number = number
local function str(v, limit)
    if type(v) ~= "string" then return "" end
    -- Trim by code points, not bytes: city names may contain UTF-8.
    local out = {}; local ok = pcall(function()
        for _, c in utf8.codes(v:gsub("%c", " ")) do
            if #out == (limit or 80) then break end
            out[#out+1] = utf8.char(c)
        end
    end)
    return ok and table.concat(out) or ""
end
M.str = str
function M.place(p)
    if type(p) ~= "table" then return nil end
    local lat, lon = number(p.lat or p.latitude), number(p.lon or p.longitude)
    local name = str(p.name, 48)
    if not lat or not lon or math.abs(lat)>90 or math.abs(lon)>180 or name=="" then return nil end
    return {name=name, lat=lat, lon=lon, country=str(p.country_code or p.country, 32),
        region=str(p.admin1 or p.region,48), key=string.format("%.4f,%.4f",lat,lon)}
end
function M.encode(s) return (s:gsub("[^%w%-_%.~]", function(c) return string.format("%%%02X", c:byte()) end)) end
local fields = {"temperature_2m", "apparent_temperature", "relative_humidity_2m", "weather_code", "is_day",
    "precipitation", "wind_speed_10m", "wind_direction_10m", "wind_gusts_10m", "cloud_cover", "surface_pressure"}
local daily_fields = {"weather_code", "temperature_2m_max", "temperature_2m_min", "precipitation_sum",
    "precipitation_probability_max", "wind_speed_10m_max", "uv_index_max", "daylight_duration"}
function M.url(p)
    return "https://api.open-meteo.com/v1/forecast?latitude="..p.lat.."&longitude="..p.lon
        .."&current="..table.concat(fields,",").."&hourly="..table.concat(fields,",")..",precipitation_probability"
        .."&daily="..table.concat(daily_fields,",")..",sunrise,sunset"
        .."&timezone=auto&forecast_days=7&timeformat=iso8601&temperature_unit=celsius&wind_speed_unit=kmh&precipitation_unit=mm"
end
local function date(s) return type(s)=="string" and s:match("^%d%d%d%d%-%d%d%-%d%d$") ~= nil end
local function iso(s) return type(s)=="string" and s:match("^%d%d%d%d%-%d%d%-%d%dT%d%d:%d%d$") ~= nil end
local function cell(t,k,i) return type(t[k])=="table" and t[k][i] or nil end
local function row(t,i)
    local r={time=i and cell(t,"time",i) or t.time}
    if not iso(r.time) then return nil end
    for _,k in ipairs(fields) do r[k]=number(i and cell(t,k,i) or t[k]) end
    if i then r.precipitation_probability=number(cell(t,"precipitation_probability",i)) end
    return r
end
function M.forecast(raw, fetched)
    if type(raw)~="table" or raw.error then return nil end
    local h,d=raw.hourly,raw.daily
    if type(h)~="table" or type(h.time)~="table" or type(d)~="table" or type(d.time)~="table" then return nil end
    local out={hours={},days={},tz=str(raw.timezone),abbrev=str(raw.timezone_abbreviation,16),fetched=fetched}
    local by_date={}
    for i=1,math.min(7,#d.time) do
        if date(d.time[i]) and not by_date[d.time[i]] then
            local day={date=d.time[i],indices={}}
            for _,k in ipairs(daily_fields) do day[k]=number(cell(d,k,i)) end
            local rise,set=cell(d,"sunrise",i),cell(d,"sunset",i)
            day.sunrise=iso(rise) and rise or nil;day.sunset=iso(set) and set or nil
            out.days[#out.days+1]=day;by_date[day.date]=#out.days
        end
    end
    local useful=false
    for i=1,math.min(180,#h.time) do
        local r=row(h,i)
        if r and by_date[r.time:sub(1,10)] then
            r.day=by_date[r.time:sub(1,10)]
            out.hours[#out.hours+1]=r
            local indices=out.days[r.day].indices;indices[#indices+1]=#out.hours
            if r.temperature_2m then useful=true end
        end
    end
    if not useful or #out.days==0 then return nil end
    out.current=type(raw.current)=="table" and row(raw.current) or nil
    if out.current and not out.current.temperature_2m then out.current=nil end
    return out
end
function M.nearest(f,time)
    local stamp=type(time)=="string" and time:sub(1,13) or ""
    for i,h in ipairs(f.hours) do if h.time:sub(1,13)==stamp then return i end end
    return 1
end
function M.pack(f)
    local raw={current=f.current,timezone=f.tz,timezone_abbreviation=f.abbrev,hourly={time={}},daily={time={}}}
    local keys={table.unpack(fields)};keys[#keys+1]="precipitation_probability"
    for _,k in ipairs(keys) do raw.hourly[k]={} end
    for i,r in ipairs(f.hours) do
        raw.hourly.time[i]=r.time
        for _,k in ipairs(keys) do raw.hourly[k][i]=r[k] or false end
    end
    local dk={table.unpack(daily_fields)};dk[#dk+1]="sunrise";dk[#dk+1]="sunset"
    for _,k in ipairs(dk) do raw.daily[k]={} end
    for i,r in ipairs(f.days) do
        raw.daily.time[i]=r.date
        for _,k in ipairs(dk) do raw.daily[k][i]=r[k] or false end
    end
    return {fetched=f.fetched,raw=raw}
end
function M.time(s) return iso(s) and s:sub(12,16) or "--:--" end
function M.weekday(s)
    if not date(s) then return "---" end
    local y,m,d=tonumber(s:sub(1,4)),tonumber(s:sub(6,7)),tonumber(s:sub(9,10))
    local offsets={0,3,2,5,0,3,5,1,4,6,2,4}
    if m<1 or m>12 then return "---" end
    if m<3 then y=y-1 end
    return ({"SUN","MON","TUE","WED","THU","FRI","SAT"})[(y+math.floor(y/4)-math.floor(y/100)+math.floor(y/400)+offsets[m]+d)%7+1]
end
function M.condition(code)
    local names={ [0]="Clear sky",[1]="Mostly clear",[2]="Partly cloudy",[3]="Overcast",[45]="Fog",[48]="Freezing fog",
        [51]="Light drizzle",[53]="Drizzle",[55]="Heavy drizzle",[56]="Freezing drizzle",[57]="Freezing drizzle",
        [61]="Light rain",[63]="Rain",[65]="Heavy rain",[66]="Freezing rain",[67]="Freezing rain",
        [71]="Light snow",[73]="Snow",[75]="Heavy snow",[77]="Snow grains",[80]="Rain showers",[81]="Rain showers",
        [82]="Heavy showers",[85]="Snow showers",[86]="Snow showers",[95]="Thunderstorm",[96]="Storm / hail",[97]="Heavy storm",[99]="Storm / hail" }
    local kind="unknown"
    if code==0 or code==1 then kind="clear" elseif code==2 then kind="partly" elseif code==3 then kind="cloud"
    elseif code==45 or code==48 then kind="fog"
    elseif code==71 or code==73 or code==75 or code==77 or code==85 or code==86 then kind="snow"
    elseif code==95 or code==96 or code==97 or code==99 then kind="storm"
    elseif names[code] then kind="rain" end
    return names[code] or "Unavailable",kind
end
function M.temp(n,units,suffix)
    n=number(n);if not n then return "--" end
    return string.format("%.0f",units=="imperial" and n*9/5+32 or n)..(suffix==false and "" or "°")
end
function M.value(n,suffix,decimals)
    return number(n) and string.format("%."..(decimals or 0).."f",n)..(suffix or "") or "--"
end
function M.wind(n,u) return M.value(number(n) and n*(u=="imperial" and 0.621371 or 1),u=="imperial" and " mph" or " km/h") end
function M.rain(n,u) return M.value(number(n) and n/(u=="imperial" and 25.4 or 1),u=="imperial" and " in" or " mm",u=="imperial" and 2 or 1) end
function M.direction(n)
    return number(n) and ({"N","NE","E","SE","S","SW","W","NW"})[math.floor((n%360+22.5)/45)%8+1] or "--"
end
return M
