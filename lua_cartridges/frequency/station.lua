-- Radio Browser records are untrusted, partial directory metadata.
local M = {}
local codecs = {MP3=true, AAC=true, ['AAC-LC']=true, VORBIS=true, ['OGG VORBIS']=true, FLAC=true, WAV=true, PCM=true}
function M.text(v, limit)
    if type(v) ~= 'string' then return '' end
    v = v:gsub('[%z\1-\31\127]', ' ')
    -- Cut only at UTF-8 character boundaries.
    local n = limit or 180
    if #v > n then
        local cut = n + 1
        while cut > 1 and (v:byte(cut) or 0) >= 128 and (v:byte(cut) or 0) < 192 do cut = cut - 1 end
        v = v:sub(1, cut - 1)
    end
    return v
end
function M.url(v)
    return type(v) == 'string' and #v <= 2048 and not v:find('[%s%c]')
        and v:match('^https?://[^/]+') ~= nil
end
function M.normalize(raw, custom)
    if type(raw) ~= 'table' then return nil end
    local url = raw.url_resolved
    if not M.url(url) then return nil end -- Never play unresolved playlist URLs.
    local codec = M.text(raw.codec, 32):upper()
    if tonumber(raw.hls) == 1 or raw.hls == true or url:lower():match('%.m3u8')
        or url:lower():match('%.m3u[%?#]?') or url:lower():match('%.pls[%?#]?') then return nil end
    if not custom and not codecs[codec] then return nil end
    if not custom and tonumber(raw.lastcheckok) == 0 then return nil end
    local id = M.text(raw.stationuuid, 100)
    if id == '' and not custom then return nil end
    local s = {stationuuid=custom and ('custom:'..url) or id,
        name=M.text(raw.name), url_resolved=url, codec=codec, hls=0,
        country=M.text(raw.country, 80), countrycode=M.text(raw.countrycode, 2):upper(),
        state=M.text(raw.state, 80), language=M.text(raw.language, 80), tags=M.text(raw.tags, 180),
        bitrate=math.max(0, math.min(10000, tonumber(raw.bitrate) or 0)),
        homepage=M.url(raw.homepage) and raw.homepage or '',
        favicon=M.url(raw.favicon) and raw.favicon or '', custom=custom or nil}
    if s.name == '' then s.name = custom and 'Custom stream' or 'Unnamed station' end
    local lat, lon = tonumber(raw.geo_lat), tonumber(raw.geo_long)
    if lat and lon and lat == lat and lon == lon and lat >= -90 and lat <= 90 and lon >= -180 and lon <= 180 then
        s.geo_lat, s.geo_long = lat, lon
    end
    return s
end
function M.list(raw, limit)
    local result, seen = {}, {}
    if type(raw) ~= 'table' then return result end
    for i = 1, math.min(#raw, limit or 100) do
        local v = raw[i]
        local s = M.normalize(v, type(v) == 'table' and v.custom == true)
        if s and not seen[s.stationuuid] then
            seen[s.stationuuid] = true; result[#result+1] = s
        end
    end
    return result
end
function M.index(list, id)
    for i, s in ipairs(list) do if s.stationuuid == id then return i end end
end
function M.project(s)
    if s and s.geo_lat and s.geo_long then
        -- Full-world equirectangular projection, identical to assets/world.png.
        return 24 + (s.geo_long + 180) / 360 * 672, 150 + (90 - s.geo_lat) / 180 * 252
    end
end
function M.neighbor(list, current, direction)
    local x, y = M.project(list[current])
    if not x then
        for i,s in ipairs(list) do if M.project(s) then return i end end
        return current
    end
    local best, score = current, math.huge
    for i,s in ipairs(list) do
        local sx, sy = M.project(s)
        if sx and i ~= current then
            local dx, dy = sx-x, sy-y
            local forward = direction == 'dpad_right' and dx or direction == 'dpad_left' and -dx
                or direction == 'dpad_down' and dy or -dy
            local cross = (direction == 'dpad_left' or direction == 'dpad_right') and dy or dx
            if forward > 0.1 then
                local distance = forward*forward + cross*cross*3
                if distance < score then best, score = i, distance end
            end
        end
    end
    return best
end
return M
