local M=require("model")
local V=require("view")
local S={places={},selected=1,units="metric",cache={},pending={},flight={},errors={},verified={},
    mode="landscape",cursor=1,results={},result_cursor=1,search_gen=0,search_status="",query="",
    hour=1,day=1,now=true,clock=0,wall=0,timer=0,notice="",notice_until=0,storage_failures={}}
local DEFAULT={name="Luxembourg",lat=49.6116,lon=6.1319,country="LU"}
local function redraw() app.request_redraw() end
local function notice(s) S.notice=s;S.notice_until=S.clock+5;redraw() end
local function load(k) local ok,v=pcall(storage.load,k);return ok and v or nil end
local function save(k,v)
    -- Some runtime storage backends do not propagate filesystem write errors.
    -- Verify through the same public API so a full/read-only disk is visible.
    local ok,verified=pcall(function()
        storage.save(k,v)
        return json.encode(storage.load(k))==json.encode(v)
    end)
    S.storage_failures[k]=not ok or not verified
    S.storage_error=false
    for _,failed in pairs(S.storage_failures) do if failed then S.storage_error=true end end
end
local function persist()
    save("preferences",{version=1,places=S.places,selected=S.places[S.selected] and S.places[S.selected].key,units=S.units})
end
local function save_cache()
    local entries={}
    for _,p in ipairs(S.places) do if S.cache[p.key] then entries[p.key]=M.pack(S.cache[p.key]) end end
    save("forecasts",{version=1,entries=entries})
end
local function place() return S.places[S.selected] end
local function forecast() local p=place();return p and S.cache[p.key] end
local function exists(key) for _,p in ipairs(S.places) do if p.key==key then return true end end end
local function selected_values()
    S.place=place();S.forecast=forecast()
    local f=S.forecast
    if f then
        S.hour=math.max(1,math.min(S.hour,#f.hours))
        S.day=f.hours[S.hour].day
        S.sample=(S.now and f.current) or f.hours[S.hour]
        S.selected_day=f.days[S.day]
        S.stale=not S.verified[S.place.key] or S.wall-f.fetched>=M.TTL or S.wall<f.fetched
    else S.sample=nil;S.selected_day=nil;S.stale=false end
    S.loading=S.place and S.flight[S.place.key]~=nil
    S.error=S.place and S.errors[S.place.key]
    redraw()
end
local function reset_hour()
    local f=forecast();S.hour=f and M.nearest(f,f.current and f.current.time) or 1;S.day=1;S.now=true
    selected_values()
end
local function request(url,meta)
    local count=0;for _ in pairs(S.pending) do count=count+1 end
    if count>=12 then return nil end
    local ok,id=pcall(http.get_async,url)
    if ok and type(id)=="number" then meta.started=S.clock;S.pending[id]=meta;return id end
end
local function fetch(force)
    local p=place();if not p or S.flight[p.key] then return end
    local f=forecast()
    if not force and f and S.verified[p.key] and S.wall-f.fetched<M.TTL and S.wall>=f.fetched then return end
    S.errors[p.key]=nil
    local id=request(M.url(p),{kind="forecast",key=p.key})
    if id then S.flight[p.key]=id else S.errors[p.key]="Request unavailable. X to retry." end
    selected_values()
end
local function select_place(i)
    if #S.places==0 then return end
    S.selected=math.max(1,math.min(i,#S.places));reset_hour();persist();fetch(false)
end
local function invalidate_search()
    S.search_gen=S.search_gen+1;S.search_id=nil;S.search_status="";S.results={};S.result_cursor=1
end
local function search(query)
    invalidate_search();S.query=M.str(query,80):match("^%s*(.-)%s*$") or ""
    if #S.query<2 then S.search_status="Enter at least two letters.";return end
    S.mode="search";S.search_status="Searching places..."
    S.search_id=request("https://geocoding-api.open-meteo.com/v1/search?name="..M.encode(S.query).."&count=8&language=en&format=json",
        {kind="search",gen=S.search_gen})
    if not S.search_id then S.search_status="Search unavailable. X to retry." end
end
local function keyboard()
    invalidate_search();S.mode="search";S.keyboard=true;text_input.show("Find a city or postal code",S.query,false)
end
local function add(p)
    for i,v in ipairs(S.places) do
        if v.key==p.key then S.mode="landscape";select_place(i);notice("Already saved. Location selected.");return end
    end
    if #S.places>=M.MAX_PLACES then notice("Eight places saved. Remove one first.");return end
    S.places[#S.places+1]=p;S.mode="landscape";invalidate_search();select_place(#S.places)
end
local function decode(r)
    if not r or not r.ok or type(r.body)~="string" then return nil end
    local ok,v=pcall(json.decode,r.body);return ok and type(v)=="table" and not v.error and v or nil
end
local function receive(r,meta)
    if meta.kind=="forecast" then
        if S.flight[meta.key]~=r.id then return end
        S.flight[meta.key]=nil
        if not exists(meta.key) then return end
        local raw=decode(r);local f=raw and M.forecast(raw,S.wall)
        if f then
            local old=forecast();local stamp=old and old.hours[S.hour] and old.hours[S.hour].time
            S.cache[meta.key]=f;S.verified[meta.key]=true;S.errors[meta.key]=nil;save_cache()
            if place() and place().key==meta.key then
                S.hour=M.nearest(f,S.now and f.current and f.current.time or stamp)
            end
        else
            S.errors[meta.key]=r.ok and "Forecast unavailable. X to retry." or "Offline / request failed. X to retry."
        end
    elseif meta.gen==S.search_gen and S.search_id==r.id then
        S.search_id=nil;S.results={};local data=decode(r)
        if data then
            local seen={}
            for i,p in ipairs(type(data.results)=="table" and data.results or {}) do
                if i>24 or #S.results==8 then break end
                local v=M.place(p)
                if v and not seen[v.key] then S.results[#S.results+1]=v;seen[v.key]=true end
            end
            S.search_status=#S.results==0 and "No places found. Y to search again." or "A to save and open this place."
        else S.search_status="Search failed. X to retry, Y to edit." end
    end
    selected_values()
end
function on_init()
    app.set_idle_fps(2);S.wall=os.time()
    local prefs=load("preferences")
    if type(prefs)=="table" and prefs.version==1 and type(prefs.places)=="table" then
        local seen={}
        for i,p in ipairs(prefs.places) do
            if i>32 or #S.places==M.MAX_PLACES then break end
            local v=M.place(p)
            if v and not seen[v.key] then S.places[#S.places+1]=v;seen[v.key]=true end
        end
        S.units=prefs.units=="imperial" and "imperial" or "metric"
        for i,p in ipairs(S.places) do if p.key==prefs.selected then S.selected=i end end
    else S.places={M.place(DEFAULT)} end
    local saved=load("forecasts")
    if type(saved)=="table" and saved.version==1 and type(saved.entries)=="table" then
        for _,p in ipairs(S.places) do
            local e=saved.entries[p.key]
            if type(e)=="table" and type(e.fetched)=="number" and e.fetched>0 and e.fetched<1e11 then
                S.cache[p.key]=M.forecast(e.raw,e.fetched)
            end
        end
    end
    reset_hour();fetch(true)
end
function on_update(dt)
    S.clock=S.clock+math.max(0,dt);S.timer=S.timer+math.max(0,dt)
    if S.timer>=30 then S.timer=0;S.wall=os.time();selected_values();fetch(false) end
    if S.notice~="" and S.clock>=S.notice_until then S.notice="";redraw() end
    if S.keyboard then
        -- poll even when is_active() turns false on keyboard submission.
        local result=text_input.poll()
        if result~=nil then
            S.keyboard=false
            if type(result)=="string" then search(result) else S.mode="places";invalidate_search() end
            redraw()
        end
    end
    for _,r in ipairs(http.poll()) do
        local meta=S.pending[r.id]
        if meta then S.pending[r.id]=nil;receive(r,meta) end
    end
    for id,meta in pairs(S.pending) do
        if S.clock-meta.started>35 then
            S.pending[id]=nil;receive({id=id,ok=false},meta)
        end
    end
end
local aliases={up="dpad_up",down="dpad_down",left="dpad_left",right="dpad_right"}
function on_input(button,action)
    if action~="press" and action~="repeat" then return end
    local b=aliases[button] or button
    if action=="repeat" and not b:match("^dpad_") and not b:match("^[lr][12]$") then return end
    if b=="select" then return end
    if S.mode=="remove" then
        if b=="b" then S.mode="places"
        elseif b=="a" then
            local p=table.remove(S.places,S.cursor)
            if p then S.cache[p.key]=nil;S.verified[p.key]=nil;S.errors[p.key]=nil;S.flight[p.key]=nil end
            if S.cursor<S.selected then S.selected=S.selected-1 end
            S.selected=math.max(1,math.min(S.selected,#S.places));S.cursor=math.max(1,math.min(S.cursor,#S.places))
            S.mode="places";persist();save_cache();reset_hour();fetch(false)
        end
    elseif S.mode=="places" then
        if b=="b" or b=="start" then S.mode="landscape"
        elseif b=="dpad_up" then S.cursor=math.max(1,S.cursor-1)
        elseif b=="dpad_down" then S.cursor=math.min(math.max(1,#S.places),S.cursor+1)
        elseif b=="y" then keyboard()
        elseif b=="x" and S.places[S.cursor] then S.mode="remove"
        elseif b=="a" and S.places[S.cursor] then S.mode="landscape";select_place(S.cursor)
        elseif b=="l1" or b=="r1" then
            local to=S.cursor+(b=="l1" and -1 or 1)
            if to>=1 and to<=#S.places then
                local selected=place();S.places[S.cursor],S.places[to]=S.places[to],S.places[S.cursor];S.cursor=to
                for i,p in ipairs(S.places) do if p==selected then S.selected=i end end
                persist();notice("Saved order updated.")
            end
        elseif b=="r2" then S.units=S.units=="metric" and "imperial" or "metric";persist() end
    elseif S.mode=="search" then
        if b=="b" then invalidate_search();S.mode="places"
        elseif b=="y" then keyboard()
        elseif b=="x" and not S.search_id then search(S.query)
        elseif b=="dpad_up" then S.result_cursor=math.max(1,S.result_cursor-1)
        elseif b=="dpad_down" then S.result_cursor=math.min(math.max(1,#S.results),S.result_cursor+1)
        elseif b=="a" and S.results[S.result_cursor] then add(S.results[S.result_cursor]) end
    else
        if b=="start" or b=="y" then S.mode="places";S.cursor=S.selected
        elseif b=="x" then fetch(true)
        elseif b=="a" then S.mode=S.mode=="details" and "landscape" or "details"
        elseif b=="b" then if S.mode=="details" then S.mode="landscape" else reset_hour() end
        elseif b=="dpad_up" then select_place(S.selected==1 and #S.places or S.selected-1)
        elseif b=="dpad_down" then select_place(S.selected%math.max(1,#S.places)+1)
        else
            local f=forecast()
            if f then
                if b=="l1" or b=="r1" or b=="l2" or b=="r2" then
                    local delta=({l1=-1,r1=1,l2=-6,r2=6})[b]
                    S.hour=math.max(1,math.min(#f.hours,S.hour+delta));S.now=false
                elseif b=="dpad_left" or b=="dpad_right" then
                    local day=math.max(1,math.min(#f.days,S.day+(b=="dpad_left" and -1 or 1)))
                    local stamp=f.hours[S.hour].time:sub(12,13)
                    local indices=f.days[day].indices
                    if #indices>0 then
                        S.hour=indices[1]
                        for _,i in ipairs(indices) do if f.hours[i].time:sub(12,13)==stamp then S.hour=i;break end end
                        S.now=false
                    end
                end
            end
        end
    end
    selected_values()
end
function on_render() V.render(S,M) end
function on_destroy() persist() end
