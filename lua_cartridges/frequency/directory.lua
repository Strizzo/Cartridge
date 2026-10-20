-- All network I/O is asynchronous. A bounded retry chain is owned by its query.
local D = {}; D.__index = D
local seeds = {'de1.api.radio-browser.info', 'de2.api.radio-browser.info', 'nl1.api.radio-browser.info'}
local function host(value)
    if type(value) ~= 'string' then return nil end
    value = value:lower():gsub('%.$', '')
    if #value > 253 or not value:match('^[a-z0-9][a-z0-9%.%-]*%.radio%-browser%.info$') then return nil end
    for label in value:gmatch('[^.]+') do
        if #label > 63 or label:sub(1,1) == '-' or label:sub(-1) == '-' then return nil end
    end
    if value:find('..',1,true) then return nil end
    return value
end
D.valid_host = host
function D.encode(value)
    return tostring(value):gsub('([^%w%-_%.~])', function(c) return string.format('%%%02X', c:byte()) end)
end
function D.new()
    local self = setmetatable({pending={}, mirrors={}, preferred=nil, alive=true, now=0}, D)
    for _,name in ipairs(seeds) do self.mirrors[#self.mirrors+1] = name end
    return self
end
function D:merge(names, prefer)
    local list, seen = {}, {}
    local function add(value)
        local h = host(value)
        if h and not seen[h] and #list < 12 then seen[h]=true; list[#list+1]=h end
    end
    if prefer then for _,n in ipairs(names) do add(n) end end
    for _,n in ipairs(self.mirrors) do add(n) end
    if not prefer then for _,n in ipairs(names) do add(n) end end
    self.mirrors = list
end
function D:request(url, callback)
    if not self.alive then return end
    local count=0; for _ in pairs(self.pending) do count=count+1 end
    if count >= 16 then callback(nil, 'Request queue busy'); return end
    local ok, id = pcall(http.get_async, url)
    if not ok or not id then callback(nil, 'Request queue busy'); return end
    self.pending[id] = {callback=callback, deadline=self.now+14}
end
function D:poll(dt)
    self.now=self.now+math.max(0,dt or 0)
    local ok, responses = pcall(http.poll)
    if ok and type(responses)=='table' then
        for _,r in ipairs(responses) do
            local p=self.pending[r.id]; self.pending[r.id]=nil
            if p and self.alive then p.callback(r) end
        end
    end
    local expired={}
    for id,p in pairs(self.pending) do if self.now>=p.deadline then expired[#expired+1]=id end end
    for _,id in ipairs(expired) do
        local p=self.pending[id]; self.pending[id]=nil
        if p and self.alive then p.callback(nil,'Request timed out') end
    end
end
local function decode(r, array)
    if not r or not r.ok or type(r.body) ~= 'string' then return nil end
    if r.status and (r.status<200 or r.status>=300) then return nil end
    if array and not r.body:match('^%s*%[') then return nil end
    local ok,data=pcall(json.decode,r.body)
    if ok and type(data)=='table' then return data end
end
function D:fetch(path, valid, callback)
    local tried, attempts = {}, 0
    local function attempt()
        if not self.alive or not valid() then return end
        local h
        if self.preferred and not tried[self.preferred] then h=self.preferred end
        if not h then for _,name in ipairs(self.mirrors) do if not tried[name] then h=name;break end end end
        if not h or attempts>=4 then callback(nil,'Directory unavailable. Check connection and retry.');return end
        tried[h]=true;attempts=attempts+1
        self:request('https://'..h..path, function(r)
            if not valid() then return end
            local data=decode(r,true)
            if data then self.preferred=h; callback(data,nil,h) else attempt() end
        end)
    end
    attempt()
end
function D:discover(callback)
    self:request('https://dns.google/resolve?name=_api._tcp.radio-browser.info&type=SRV',function(r)
        local data=decode(r,false);local names={}
        if data and data.Status==0 and type(data.Answer)=='table' then
            for _,a in ipairs(data.Answer) do
                if type(a)=='table' and a.type==33 and type(a.data)=='string' then
                    local priority,weight,port,name=a.data:match('^(%d+)%s+(%d+)%s+(%d+)%s+(%S+)%s*$')
                    if priority and weight and tonumber(port)==443 and host(name) then names[#names+1]=host(name) end
                end
            end
        end
        -- Rotate valid discovery results so every client needn't choose the first mirror.
        if #names>1 then
            local start=(os.time()%#names)+1;local rotated={}
            for i=0,#names-1 do rotated[#rotated+1]=names[(start+i-1)%#names+1] end
            names=rotated
        end
        self:merge(names,true)
        callback()
        -- Lua has no native DNS lookup. /servers complements failed/partial DoH.
        self:fetch('/json/servers',function() return self.alive end,function(servers)
            if not servers then return end
            local discovered={}
            for i=1,math.min(#servers,32) do
                if type(servers[i])=='table' then discovered[#discovered+1]=servers[i].name end
            end
            self:merge(discovered,false)
        end)
    end)
end
function D:click(station)
    if station.custom then return end
    -- Best effort, no retry: a browse, favorite, resume, or reconnect never counts.
    self:request('https://'..(self.preferred or self.mirrors[1])..'/json/url/'..D.encode(station.stationuuid),function() end)
end
function D:close() self.alive=false;self.pending={} end
return D
