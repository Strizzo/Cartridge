local Station = require('station')
local Directory = require('directory')
local View = require('view')
local PAGE = 100
local S = {
    view='explore', focus='list', selected=1, stations={}, favorites={}, recent={},
    genre=1, genres={'All sounds','Jazz','Classical','Ambient','Electronic','Rock','Pop','News','World','Soul'},
    query='', country='', country_code='', offset=0, more=false, raw_count=0, skipped=0,
    loading=true, stale=false, error=nil, generation=0, query_delay=nil,
    countries={{name='All countries',stationcount=0}}, country_cursor=1,
    country_filter='', country_offset=0, country_more=false, country_loading=false, country_gen=0,
    menu=false, menu_cursor=1, menu_items={'Explore','Countries','Favorites','Recent','Settings','Refresh directory','Next page','Previous page','Station details'},
    settings_cursor=1, settings_items={'Volume','Play a custom stream','About Frequency','Clear recent history'},
    volume=0.7, player=nil, status={state='stopped',error=''}, keyboard=nil,
    notice=nil, notice_time=0, initialized=false, alive=true,
}
local directory=Directory.new()
local function redraw() if app then app.request_redraw() end end
local function notice(message) S.notice=message;S.notice_time=5;redraw() end
local function load(key)
    local ok,value=pcall(storage.load,key)
    return ok and type(value)=='table' and value or nil
end
local function persist()
    local ok=pcall(storage.save,'frequency.v1',{version=1,volume=S.volume,
        favorites=S.favorites,recent=S.recent,last_station=S.last_station})
    if not ok then notice('Could not save. Changes are kept for this session.') end
end
local function list()
    if S.view=='favorites' then return S.favorites elseif S.view=='recent' then return S.recent end
    return S.stations
end
local function selected() return list()[S.selected] end
local function fingerprint()
    return table.concat({S.country_code,S.query,S.genres[S.genre],tostring(S.offset)},'\n')
end
local function cache()
    pcall(storage.save,'frequency.cache.v1',{version=1,key=fingerprint(),stations=S.stations,
        more=S.more,raw_count=S.raw_count,skipped=S.skipped,saved_at=os.time()})
end
local function fetch()
    if not S.alive then return end
    S.query_delay=nil
    local gen,key=S.generation,fingerprint()
    local path='/json/stations/search?limit=100&offset='..S.offset..'&order=clickcount&reverse=true&hidebroken=true'
    if S.country_code~='' then path=path..'&countrycode='..S.country_code end
    if S.query~='' then path=path..'&name='..Directory.encode(S.query) end
    if S.genre>1 then path=path..'&tag='..Directory.encode(S.genres[S.genre]:lower()) end
    S.loading=true;S.error=nil
    directory:fetch(path,function() return S.alive and gen==S.generation end,function(data,err)
        S.loading=false
        if not data then
            S.error=err;S.stale=#S.stations>0;redraw();return
        end
        local rows,seen={},{}
        local raw_count=math.min(#data,PAGE)
        for i=1,raw_count do
            local station=Station.normalize(data[i],false)
            if station and not seen[station.stationuuid] then
                seen[station.stationuuid]=true;rows[#rows+1]=station
            end
        end
        -- Stable MP3-first partition keeps each page's directory popularity order.
        local ordered={}
        for _,station in ipairs(rows) do if station.codec=='MP3' then ordered[#ordered+1]=station end end
        for _,station in ipairs(rows) do if station.codec~='MP3' then ordered[#ordered+1]=station end end
        rows=ordered
        local old=selected()
        S.stations=rows;S.raw_count=raw_count;S.skipped=raw_count-#rows
        S.more=#data>=PAGE;S.stale=false;S.error=nil;S.cache_time=os.time()
        if S.view=='explore' then
            S.selected=Station.index(rows,old and old.stationuuid or S.last_station and S.last_station.stationuuid) or 1
        end
        if key==fingerprint() then cache() end
        redraw()
    end)
end
local function schedule(changed)
    S.generation=S.generation+1;S.loading=true;S.error=nil
    if changed then S.stations={};S.selected=1;S.more=false;S.raw_count=0;S.skipped=0;S.stale=false
    else S.stale=#S.stations>0 end
    S.query_delay=0.25
end
local function fetch_countries()
    S.country_gen=S.country_gen+1
    local gen=S.country_gen
    S.country_loading=true;S.country_error=nil
    local path='/json/countries'
    if S.country_filter~='' then path=path..'/'..Directory.encode(S.country_filter) end
    path=path..'?limit=100&offset='..S.country_offset..'&order=name&hidebroken=true'
    directory:fetch(path,function() return S.alive and gen==S.country_gen end,function(data,err)
        S.country_loading=false
        if not data then S.country_error=err;redraw();return end
        S.countries={{name='All countries',stationcount=0}}
        for i=1,math.min(#data,100) do
            local c=data[i]
            if type(c)=='table' and Station.text(c.name,80)~='' then
                local code=Station.text(c.iso_3166_1,2):upper()
                if code:match('^[A-Z][A-Z]$') then
                    S.countries[#S.countries+1]={name=Station.text(c.name,80),code=code,stationcount=tonumber(c.stationcount) or 0}
                end
            end
        end
        S.country_more=#data>=100;S.country_cursor=1;redraw()
    end)
end
local function stop()
    if audio and audio.stop_stream then pcall(audio.stop_stream) end
    S.status={state='stopped',error=''};redraw()
end
local function play(station)
    if not station then
        if S.view=='explore' and not S.loading then schedule(false) end
        return
    end
    if not audio or not audio.stream then notice('Streaming requires the current audio + network runtime.');return end
    local same=S.player and S.player.stationuuid==station.stationuuid
    if same and (S.status.state=='playing' or S.status.state=='paused') then
        local ok,err=pcall(audio.pause_stream,S.status.state~='paused')
        if not ok then notice(Station.text(tostring(err),160)) end
        return
    end
    if same and (S.status.state=='connecting' or S.status.state=='buffering') then return end
    local ok,err=pcall(audio.stream,station.url_resolved)
    S.player=station
    if not ok then
        S.status={state='error',error=Station.text(tostring(err),180)}
        S.local_audio_error=true;redraw();return
    end
    S.local_audio_error=false;S.status={state='connecting',error='',url=station.url_resolved}
    S.last_station=station
    if S.view=='recent' then S.selected=1 end
    local existing=Station.index(S.recent,station.stationuuid)
    if existing then table.remove(S.recent,existing) end
    table.insert(S.recent,1,station)
    while #S.recent>30 do table.remove(S.recent) end
    persist()
    directory:click(station)
    redraw()
end
local function favorite()
    local station=selected();if not station then return end
    local i=Station.index(S.favorites,station.stationuuid)
    if i then table.remove(S.favorites,i);notice('Removed from favorites')
    elseif #S.favorites>=100 then notice('Favorites full (100). Remove one to save another.');return
    else table.insert(S.favorites,station);notice('Saved to favorites') end
    S.selected=math.max(1,math.min(S.selected,#list()))
    persist()
end
local function volume(delta)
    S.volume=math.max(0,math.min(1,math.floor((S.volume+delta)*100+0.5)/100))
    if audio and audio.set_stream_volume then pcall(audio.set_stream_volume,S.volume) end
    persist();redraw()
end
local function keyboard(kind)
    S.keyboard=kind
    local labels={search='Search station names (blank clears)',country='Find a country (blank clears)',custom='Play direct HTTP(S) audio URL'}
    text_input.show(labels[kind],kind=='search' and S.query or kind=='country' and S.country_filter or '',false,kind=='custom' and 1024 or 100)
end
local function change_view(view)
    S.view=view;S.selected=1;S.menu=false;S.focus='list'
    if view=='countries' and #S.countries==1 and not S.country_loading then fetch_countries() end
end
local function menu_action()
    local index=S.menu_cursor;S.menu=false
    if index==9 then S.detail=selected();S.view='details';return end
    if index<=5 then change_view(({'explore','countries','favorites','recent','settings'})[index])
    elseif index==6 then
        if S.view=='countries' then if not S.country_loading then fetch_countries() end
        elseif not S.loading then change_view('explore');schedule(false) end
    elseif index==7 or index==8 then
        if S.view=='countries' then
            if not S.country_loading and ((index==7 and S.country_more) or (index==8 and S.country_offset>0)) then
                S.country_offset=math.max(0,S.country_offset+(index==7 and PAGE or -PAGE));fetch_countries()
            end
        elseif not S.loading and ((index==7 and S.more) or (index==8 and S.offset>0)) then
            change_view('explore');S.offset=math.max(0,S.offset+(index==7 and PAGE or -PAGE));schedule(true)
        else notice(S.loading and 'Wait for this page to finish.' or 'No more directory pages.') end
    end
end
function on_init()
    local saved=load('frequency.v1')
    if saved and saved.version==1 then
        S.favorites=Station.list(saved.favorites,100);S.recent=Station.list(saved.recent,30)
        local v=tonumber(saved.volume)
        if v and v==v then S.volume=math.max(0,math.min(1,v)) end
        if type(saved.last_station)=='table' then S.last_station=Station.normalize(saved.last_station,saved.last_station.custom==true) end
    end
    local old=load('frequency.cache.v1')
    if old and old.version==1 and old.key==fingerprint() then
        S.stations=Station.list(old.stations,100);S.stale=#S.stations>0
        S.more=old.more==true;S.raw_count=tonumber(old.raw_count) or #S.stations
        S.skipped=tonumber(old.skipped) or 0;S.cache_time=tonumber(old.saved_at)
        S.selected=Station.index(S.stations,S.last_station and S.last_station.stationuuid) or 1
    end
    if audio and audio.set_stream_volume then pcall(audio.set_stream_volume,S.volume) end
    if app then app.set_idle_fps(5) end
    -- Restored records are selection/history only. Never call audio.stream here.
    directory:discover(function()
        S.initialized=true
        if not S.query_delay then fetch() end
    end)
end
function on_update(dt)
    if not S.alive then return end
    directory:poll(dt)
    if S.query_delay then
        S.query_delay=S.query_delay-dt
        if S.query_delay<=0 and S.initialized then fetch() end
    end
    -- Poll by our pending mode, not is_active(): submission closes the keyboard first.
    if S.keyboard then
        local result=text_input.poll()
        if result~=nil then
            local kind=S.keyboard;S.keyboard=nil
            if type(result)=='string' then
                local clean=Station.text(result,kind=='custom' and 2048 or 100):match('^%s*(.-)%s*$')
                if kind=='custom' then
                    local custom=Station.normalize({url_resolved=clean,name=clean:match('^https?://([^/]+)') or 'Custom stream',codec='',country='Custom stream'},true)
                    if custom then change_view('recent');play(custom) else notice('Use a direct HTTP(S) audio URL. Playlists and HLS are unsupported.') end
                elseif kind=='country' then
                    S.country_filter=clean;S.country_offset=0;fetch_countries()
                else
                    S.query=clean;S.offset=0;change_view('explore');schedule(true)
                end
            end
            redraw()
        end
    end
    if S.player and not S.local_audio_error and audio and audio.stream_status then
        local ok,status=pcall(audio.stream_status)
        if ok and type(status)=='table' and (status.url==S.player.url_resolved or status.state=='stopped') then
            if status.state~=S.status.state or status.error~=S.status.error then
                S.status={state=status.state or 'error',error=Station.text(status.error,180),url=status.url}
                redraw()
            end
        end
    end
    if S.notice then
        S.notice_time=S.notice_time-dt
        if S.notice_time<=0 then S.notice=nil;redraw() end
    end
end
function on_input(button,action)
    if action~='press' and action~='repeat' then return end
    if button=='select' then return end -- Runtime owns quit.
    if action=='repeat' and not button:match('^dpad_') and button~='l2' and button~='r2' then return end
    if button=='l2' then volume(-0.05);return elseif button=='r2' then volume(0.05);return end
    if button=='start' then S.menu=not S.menu;S.menu_cursor=1;return end
    if S.menu then
        if button=='dpad_up' then S.menu_cursor=math.max(1,S.menu_cursor-1)
        elseif button=='dpad_down' then S.menu_cursor=math.min(#S.menu_items,S.menu_cursor+1)
        elseif button=='a' then menu_action() elseif button=='b' then S.menu=false end
        return
    end
    if button=='b' then
        if S.view~='explore' then change_view('explore')
        elseif S.focus=='map' then S.focus='list'
        else stop() end
        return
    end
    if S.view=='about' or S.view=='details' then return end
    if S.view=='settings' then
        if button=='dpad_up' then S.settings_cursor=math.max(1,S.settings_cursor-1)
        elseif button=='dpad_down' then S.settings_cursor=math.min(4,S.settings_cursor+1)
        elseif S.settings_cursor==1 and button=='dpad_left' then volume(-0.05)
        elseif S.settings_cursor==1 and button=='dpad_right' then volume(0.05)
        elseif button=='a' then
            if S.settings_cursor==2 then keyboard('custom')
            elseif S.settings_cursor==3 then S.view='about'
            elseif S.settings_cursor==4 then S.recent={};persist();notice('Recent history cleared') end
        end
        return
    end
    if S.view=='countries' then
        if button=='y' then keyboard('country')
        elseif button=='dpad_up' then S.country_cursor=math.max(1,S.country_cursor-1)
        elseif button=='dpad_down' then S.country_cursor=math.min(#S.countries,S.country_cursor+1)
        elseif button=='dpad_left' and S.country_offset>0 and not S.country_loading then S.country_offset=S.country_offset-PAGE;fetch_countries()
        elseif button=='dpad_right' and S.country_more and not S.country_loading then S.country_offset=S.country_offset+PAGE;fetch_countries()
        elseif button=='a' then
            if S.country_error and S.country_cursor==1 then fetch_countries()
            else
                local c=S.countries[S.country_cursor]
                if c then S.country=S.country_cursor==1 and '' or c.name;S.country_code=S.country_cursor==1 and '' or c.code;S.offset=0;change_view('explore');schedule(true) end
            end
        end
        return
    end
    if button=='y' then keyboard('search');return end
    if button=='x' then favorite();return end
    if button=='a' then play(selected());return end
    if button=='l1' or button=='r1' then
        S.genre=((S.genre-1+(button=='r1' and 1 or -1))%#S.genres)+1
        S.offset=0;change_view('explore');schedule(true);return
    end
    if S.focus=='map' then
        if button:match('^dpad_') then S.selected=Station.neighbor(list(),S.selected,button) end
    elseif button=='dpad_up' then S.selected=math.max(1,S.selected-1)
    elseif button=='dpad_down' then S.selected=math.min(math.max(1,#list()),S.selected+1)
    elseif button=='dpad_left' then change_view('countries')
    elseif button=='dpad_right' then
        local pin=Station.neighbor(list(),0,'dpad_right')
        if list()[pin] and Station.project(list()[pin]) then
            S.focus='map'
            if not Station.project(selected()) then S.selected=pin end
        else notice('No coordinates on this page. Stations are available in the list.') end
    end
end
function on_render() View.draw(S,list(),Station) end
function on_destroy()
    S.alive=false;directory:close();stop()
end
