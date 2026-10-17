-- Original procedural landscape. All geometry is local; nothing is fetched or
-- measured during rendering. Terrain contours are prepared once at load time.
local V={}
local C={paper={242,234,216},ink={27,49,48},muted={86,101,92},red={209,65,39},line={199,197,172},light={255,247,227}}
local function rect(x,y,w,h,c) if w>0 and h>0 then screen.draw_rect(x,y,w,h,{color=c,filled=true}) end end
local function line(x,y,x2,y2,c,w) screen.draw_line(x,y,x2,y2,{color=c,width=w or 1}) end
-- Filled rectangles keep RGB consistent across SDL_gfx builds. Cache the
-- raster spans at module load, including all radii used by the cloud shapes.
local discs={}
for r=1,45 do
    local spans={};local start,previous=0,nil
    for y=-r,r+1 do
        local width=y<=r and math.floor(math.sqrt(r*r-y*y)) or -1
        if width~=previous then
            if previous then spans[#spans+1]={-previous,start,previous*2+1,y-start} end
            previous=width;start=y
        end
    end
    discs[r]=spans
end
local function circle(x,y,r,c)
    for _,s in ipairs(discs[math.max(1,math.min(45,math.floor(r)))]) do rect(x+s[1],y+s[2],s[3],s[4],c) end
end
local function text(s,x,y,size,c,w) screen.draw_text(tostring(s),x,y,{size=size or 16,color=c or C.ink,bold=true,max_width=w or 660}) end
local function display(s,x,y,size,c) screen.draw_display_text(tostring(s),x,y,size,c or C.ink) end
local function footer(items)
    rect(0,684,720,36,C.ink)
    for _,h in ipairs(items) do
        text(h[1],h[3],695,12,C.paper,100);text(h[2],h[3]+h[4],695,12,{174,193,177},130)
    end
end
local terrain={}
for x=0,668,4 do
    local t=x/672
    terrain[#terrain+1]={x=x,
        far=175-52*math.exp(-((t-.67)/.15)^2)-34*math.exp(-((t-.23)/.16)^2),
        mid=202-21*math.sin(t*8+.8)-13*math.cos(t*13),
        near=math.min(281,254-36*math.sin(t*4.1+.6))}
end
local function cloud(x,y,s,c)
    circle(x,y,13*s,c);circle(x+18*s,y-9*s,20*s,c);circle(x+42*s,y,15*s,c)
    rect(x-8*s,y,51*s,13*s,c)
end
local function glyph(kind,x,y,c)
    if kind=="clear" then
        circle(x,y,9,c)
        for i=0,7 do local a=i*math.pi/4;line(x+math.cos(a)*13,y+math.sin(a)*13,x+math.cos(a)*17,y+math.sin(a)*17,c,2) end
    elseif kind=="snow" then
        line(x-12,y,x+12,y,c,2);line(x-7,y-10,x+7,y+10,c,2);line(x+7,y-10,x-7,y+10,c,2)
    elseif kind=="fog" then for j=-1,1 do rect(x-16+j*2,y+j*7,30,3,c) end
    elseif kind=="unknown" then text("?",x-6,y-12,22,c,30)
    else
        if kind=="partly" then circle(x+9,y-8,8,C.red) end
        cloud(x-14,y-2,.46,c)
        if kind=="rain" or kind=="storm" then
            for j=0,2 do line(x-9+j*10,y+10,x-12+j*10,y+17,c,2) end
        end
        if kind=="storm" then line(x+6,y+4,x,y+13,C.red,3);line(x,y+13,x+7,y+13,C.red,3) end
    end
end
local function landscape(sample,day,M)
    local _,kind=M.condition(sample and sample.weather_code)
    local night=sample and sample.is_day==0
    local hour=tonumber(sample and sample.time:sub(12,13)) or 12
    -- Sunrise/sunset are local ISO strings, including on DST transition days.
    local rise=day and day.sunrise and tonumber(day.sunrise:sub(12,13)) or 6
    local set=day and day.sunset and tonumber(day.sunset:sub(12,13)) or 18
    local dusk=not night and (math.abs(hour-rise)<=1 or math.abs(hour-set)<=1)
    local sky=night and {25,53,67} or dusk and {235,168,117} or {208,221,196}
    local far=night and {53,83,88} or {136,162,144}
    local mid=night and {36,70,76} or {82,130,119}
    local near=night and {20,49,57} or {31,84,79}
    local water=night and {43,81,99} or {133,183,175}
    local cloud_color=night and {109,131,138} or {242,233,207}
    if kind=="rain" or kind=="storm" then
        sky=night and {26,44,56} or {142,171,174};far={94,128,135};mid={52,98,110};water={89,143,155}
    elseif kind=="snow" then
        sky=night and {37,61,79} or {207,220,220};far={169,191,195};mid={123,157,166};near={65,114,124}
    elseif kind=="fog" then sky={195,207,191};far={171,189,171};mid={141,166,149} end
    local ox,oy=24,168
    rect(ox,oy,672,282,sky)
    if night then
        for i=1,22 do local x=300+(i*97)%349;local y=14+(i*43)%109;circle(ox+x,oy+y,i%4==0 and 2 or 1,C.paper) end
    end
    local sx=ox+414+(hour%24)/24*152
    local sy=oy+83-26*math.sin((hour-6)/12*math.pi)
    if kind=="clear" or kind=="partly" or kind=="snow" or dusk then
        circle(sx,sy,44,night and C.paper or C.red)
        if night then circle(sx+18,sy-12,40,sky) end
        if not night then
            for i=1,3 do line(sx-58-i*7,sy-28+i*16,sx-49-i*7,sy-28+i*16,C.red,2) end
        end
    end
    if kind~="clear" and kind~="unknown" then
        cloud(ox+408,oy+91,1.15,cloud_color)
        if kind~="partly" then cloud(ox+543,oy+46,.85,cloud_color);cloud(ox+299,oy+37,.58,cloud_color) end
    end
    rect(ox,oy+182,672,100,water)
    for _,p in ipairs(terrain) do rect(ox+p.x,oy+p.far,4,226-p.far,far) end
    for _,p in ipairs(terrain) do rect(ox+p.x,oy+p.mid,4,242-p.mid,mid) end
    -- A widening river separates near hills and terraces.
    for i=0,12 do
        local y=oy+226+i*4;local x=ox+386-i*12
        rect(x,y,90+i*17,2,night and {88,124,131} or {180,205,183})
    end
    for _,p in ipairs(terrain) do
        if p.x<272 or p.x>589 then rect(ox+p.x,oy+p.near,4,282-p.near,near) end
    end
    -- Architectural landmark: a tiny coastal lookout, not a location claim.
    rect(ox+548,oy+186,10,45,C.paper);rect(ox+540,oy+177,26,12,C.ink)
    rect(ox+544,oy+168,18,9,C.red);rect(ox+551,oy+191,4,8,C.red)
    line(ox+536,oy+230,ox+581,oy+230,near,3)
    for i=0,5 do
        local x=ox+27+i*31;local y=oy+230+i%3*5
        line(x,y,x+7,y-24,night and {93,126,108} or {181,191,118},2)
        line(x+4,y-12,x-5,y-22,night and {93,126,108} or {181,191,118},2)
    end
    if kind=="rain" or kind=="storm" then
        for i=0,24 do local x=ox+302+(i*59)%343;local y=oy+20+(i*37)%230;line(x,y,x-5,y+16,{207,222,210},2) end
        if kind=="storm" then line(ox+508,oy+94,ox+492,oy+127,C.paper,4);line(ox+492,oy+127,ox+508,oy+123,C.paper,4);line(ox+508,oy+123,ox+494,oy+148,C.paper,4) end
    elseif kind=="snow" then
        for i=0,33 do local x=ox+280+(i*73)%375;local y=oy+16+(i*31)%251;circle(x,y,2,C.light) end
        for i=0,7 do rect(ox+i*23,oy+236+(i%3)*5,18,3,C.paper) end
    elseif kind=="fog" then
        for i=0,3 do rect(ox+272+i*24,oy+137+i*29,342-i*22,7,{210,218,198}) end
    end
    -- The left of the composition is deliberately quiet for legible data.
    return night and C.paper or C.ink
end
local function header(S,title,kicker)
    rect(0,0,720,720,C.paper)
    display(title or "OUTSIDE",24,11,58)
    text(kicker or "A WINDOW TO THE WEATHER",26,79,12,C.muted,475)
    rect(581,23,115,45,C.red);text("FIELD / 01",592,38,14,C.paper,98)
    line(24,104,696,104,C.ink,2)
    if S.place then
        text(S.place.name:upper(),24,120,25,C.ink,424)
        text("UP/DN "..S.selected.."/"..#S.places.." · "..S.place.country,488,127,13,C.muted,206)
    else text("YOUR FIRST PLACE",24,123,23,C.ink,640) end
end
local function status(S,M)
    local message,color="",C.muted
    if S.notice~="" then message=S.notice;color=C.red
    elseif S.storage_error then message="Storage unavailable. Changes are kept this session.";color=C.red
    elseif S.error then message=(S.forecast and "STALE CACHE · " or "")..S.error;color=C.red
    elseif S.loading then message=S.forecast and "CACHED FORECAST · Refreshing..." or "CONNECTING · Fetching your forecast..."
    elseif S.stale then message="STALE CACHE · X to refresh";color=C.red
    elseif S.forecast then
        message="OPEN-METEO  /  "..S.forecast.tz.."  /  Updated "..M.time(S.forecast.current and S.forecast.current.time)
    end
    text(message,24,660,13,color,672)
end
local function empty(S,M)
    landscape(nil,nil,M)
    local title=S.place and (S.loading and "LOOKING OUT" or "NO FORECAST") or "FIND YOUR OUTSIDE"
    display(title,42,205,S.place and 36 or 30)
    text(S.place and (S.loading and "The landscape is waiting for live weather." or "Connect to Wi-Fi, then press X to retry.")
        or "Save a city to open a window on its weather.",43,260,16,C.ink,530)
    text("Y  SAVED PLACES + SEARCH",24,484,20,C.ink,640)
    text("LEFT / RIGHT   Choose a forecast day",24,530,16,C.muted)
    text("L1 / R1   Scrub one hour     L2 / R2   Six hours",24,563,16,C.muted)
    text("UP / DOWN   Switch saved locations",24,596,16,C.muted)
    status(S,M);footer({{"Y","Places",24,22},{"X","Retry",205,22},{"SELECT","Quit",504,60}})
end
local function hourly(S,M)
    local f=S.forecast;local h=f.hours[S.hour]
    local first=math.max(1,math.min(S.hour-3,#f.hours-7))
    text("L1 / R1   HOURS",24,463,12,C.muted,180)
    text((S.now and "CURRENT · " or "FORECAST · ")..h.time:sub(1,10).." · LOCAL TIME",238,463,12,C.muted,458)
    for j=0,7 do
        local i=first+j;local sample=f.hours[i]
        if sample then
            local x=24+j*84;local active=i==S.hour
            if active then rect(x,485,79,53,C.ink) end
            text(M.time(sample.time),x+9,490,13,active and C.paper or C.muted,68)
            text(M.temp(sample.temperature_2m,S.units),x+9,512,18,active and C.paper or C.ink,68)
            local chance=sample.precipitation_probability
            if chance and chance>=30 then rect(x+65,519,5,math.max(3,12*chance/100),active and C.paper or C.red) end
        end
    end
end
local function week(S,M)
    for i,d in ipairs(S.forecast.days) do
        local x=24+(i-1)*96;local selected=i==S.day
        if selected then rect(x,550,91,98,C.red) else line(x,550,x+90,550,C.line,1) end
        local c=selected and C.paper or C.ink
        text(M.weekday(d.date),x+10,560,13,c,74)
        local _,kind=M.condition(d.weather_code);glyph(kind,x+66,583,c)
        text(M.temp(d.temperature_2m_max,S.units).." / "..M.temp(d.temperature_2m_min,S.units),x+8,606,14,c,78)
        text(M.value(d.precipitation_probability_max,"%"),x+10,629,11,c,74)
    end
end
local function main(S,M)
    header(S)
    if not S.forecast then empty(S,M);return end
    local r,d=S.sample,S.selected_day
    local c=landscape(r,d,M)
    text(S.now and "CURRENT CONDITIONS" or M.weekday(d.date).."  /  "..M.time(r.time),43,185,13,c,310)
    local temperature=M.temp(r.temperature_2m,S.units)..(S.units=="imperial" and "F" or "C")
    display(temperature,39,204,#temperature>7 and 70 or 94,c)
    local label=M.condition(r.weather_code)
    text(label:upper(),43,317,21,c,345)
    text("Feels "..M.temp(r.apparent_temperature,S.units).."   ·   "..M.wind(r.wind_speed_10m,S.units),43,350,14,c,352)
    rect(24,419,268,31,C.ink)
    text("RISE "..M.time(d.sunrise).."   SET "..M.time(d.sunset),37,429,12,C.paper,248)
    hourly(S,M);week(S,M);status(S,M)
    footer({{"A","Details",24,20},{"B","Now",160,20},{"Y","Places",278,20},{"X","Retry",420,20},{"L/R","Day",556,35}})
end
local function details(S,M)
    header(S,"CLOSER LOOK","EVERY HOUR HAS ITS OWN STORY")
    if not S.forecast then empty(S,M);return end
    local r,d=S.sample,S.selected_day
    text((S.now and "CURRENT" or "FORECAST").." / "..r.time:gsub("T","  ").." / LOCAL",24,164,15,C.muted,660)
    display(M.temp(r.temperature_2m,S.units)..(S.units=="imperial" and "F" or "C"),24,187,61)
    text(M.condition(r.weather_code),310,204,24,C.ink,384)
    text("Feels like "..M.temp(r.apparent_temperature,S.units),310,242,16,C.muted,380)
    local values={
        {"WIND / "..M.direction(r.wind_direction_10m),M.wind(r.wind_speed_10m,S.units)},
        {"GUSTS",M.wind(r.wind_gusts_10m,S.units)},
        {"HUMIDITY",M.value(r.relative_humidity_2m,"%")},
        {"PRECIPITATION",M.rain(r.precipitation,S.units)},
        {"CHANCE / HOUR",M.value(S.forecast.hours[S.hour].precipitation_probability,"%")},
        {"PRESSURE",M.value(r.surface_pressure," hPa")},
    }
    for i,v in ipairs(values) do
        local x=24+(i-1)%3*228;local y=288+math.floor((i-1)/3)*77
        line(x,y,x+216,y,C.line);text(v[1],x,y+9,12,C.muted,211);text(v[2],x,y+31,23,C.ink,211)
    end
    text(M.weekday(d.date).." · "..d.date.." / DAILY OUTLOOK",24,454,15,C.muted,664)
    local daily={
        {"HIGH / LOW",M.temp(d.temperature_2m_max,S.units).." / "..M.temp(d.temperature_2m_min,S.units)},
        {"RAIN / SNOW WATER",M.rain(d.precipitation_sum,S.units)},
        {"UV MAX",M.value(d.uv_index_max,"",1)},
    }
    for i,v in ipairs(daily) do local x=24+(i-1)*228;text(v[1],x,488,12,C.muted,216);display(v[2],x,509,30) end
    rect(24,565,672,69,C.ink)
    text("SUNRISE",40,577,12,C.paper,150);text(M.time(d.sunrise),40,599,23,C.paper,150)
    text("SUNSET",266,577,12,C.paper,150);text(M.time(d.sunset),266,599,23,C.paper,150)
    text("DAYLIGHT",488,577,12,C.paper,184)
    text(d.daylight_duration and string.format("%dh %02dm",math.floor(d.daylight_duration/3600),math.floor(d.daylight_duration/60)%60) or "--",488,599,23,C.paper,184)
    status(S,M);footer({{"B","Back",24,20},{"L1/R1","Hour",151,56},{"L/R","Day",360,36},{"Y","Places",530,22}})
end
local function places(S,M)
    header(S,"YOUR PLACES","SAVED LOCATIONS / "..#S.places.." OF 8")
    if S.mode=="remove" then
        rect(24,190,672,246,C.ink);display("REMOVE PLACE?",44,215,42,C.paper)
        text(S.places[S.cursor].name,44,293,27,C.paper,626)
        text("This also removes its saved forecast.",44,351,18,C.paper,620)
        footer({{"A","Remove",24,22},{"B","Cancel",222,22}});return
    end
    if #S.places==0 then
        display("GO SOMEWHERE.",24,210,45)
        text("Press Y to search for a city or postal code.",24,282,20,C.muted,665)
    end
    for i,p in ipairs(S.places) do
        local y=170+(i-1)*54;local focused=i==S.cursor
        if focused then rect(24,y,672,49,C.ink) else line(24,y+49,696,y+49,C.line) end
        local c=focused and C.paper or C.ink
        text(string.format("%02d",i),37,y+14,17,focused and {212,170,119} or C.red,42)
        text(p.name,86,y+8,20,c,366)
        text(p.region~="" and p.region or p.country,86,y+32,11,focused and C.paper or C.muted,360)
        text(i==S.selected and "ACTIVE" or (S.cache[p.key] and "CACHED" or p.country),550,y+16,13,c,125)
    end
    text("L1 / R1  Reorder      R2  Units: "..(S.units=="metric" and "°C · km/h · mm" or "°F · mph · in"),24,616,15,C.muted,672)
    status(S,M);footer({{"A","Open",24,20},{"Y","Add",159,22},{"X","Remove",288,22},{"B","Back",478,22}})
end
local function search(S,M)
    header(S,"FIND A PLACE","GEOCODING BY OPEN-METEO / GEONAMES")
    rect(24,164,672,51,C.ink);text(S.query=="" and "City or postal code" or S.query,40,178,23,C.paper,640)
    text(S.search_status,24,229,15,C.muted,670)
    for i,p in ipairs(S.results) do
        local y=261+(i-1)*47;local focused=i==S.result_cursor
        if focused then rect(24,y,672,44,C.red) else line(24,y+44,696,y+44,C.line) end
        local c=focused and C.paper or C.ink
        text(p.name,37,y+5,20,c,407)
        text(p.region,37,y+29,11,c,409)
        text(p.country,485,y+8,14,c,192)
    end
    if S.notice~="" then text(S.notice,24,656,14,C.red,672) end
    footer({{"A","Save",24,20},{"Y","Search",167,22},{"X","Retry",342,22},{"B","Back",511,22}})
end
function V.render(S,M)
    screen.clear(C.paper[1],C.paper[2],C.paper[3])
    if S.mode=="places" or S.mode=="remove" then places(S,M)
    elseif S.mode=="search" then search(S,M)
    elseif S.mode=="details" then details(S,M)
    else main(S,M) end
end
return V
