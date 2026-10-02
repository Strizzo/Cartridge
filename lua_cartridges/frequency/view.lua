-- Frequency: a quiet, printed radio atlas. Artwork is baked; no animation loop.
local V={}
local C={paper={239,235,223},ink={25,47,56},dim={92,104,101},line={208,207,192},
    orange={193,70,33},peach={250,151,100},cream={249,247,239},green={39,107,88},ocean={25,47,56},muted={154,178,172}}
local function rect(x,y,w,h,c,r) screen.draw_rect(x,y,w,h,{color=c,radius=0,filled=true}) end
local function text(t,x,y,size,color,bold,w)
    screen.draw_text(tostring(t or ''),x,y,{size=size or 16,color=color or C.ink,bold=bold or false,max_width=w})
end
local function line(x1,y1,x2,y2,c,w) screen.draw_line(x1,y1,x2,y2,{color=c or C.line,width=w or 1}) end
local function dot(x,y,r,c)
    -- Opaque core primitives avoid SDL_gfx packed-color differences on hosts.
    rect(x-r+1,y-r,2*r-1,2*r+1,c)
    rect(x-r,y-r+1,2*r+1,2*r-1,c)
end
local function heading(t,x,y,size,c) screen.draw_display_text(t,x,y,size,c or C.ink) end
local function hint(key,label,x,y)
    rect(x,y,math.max(22,#key*7+8),19,C.ink,3)
    text(key,x+4,y+1,11,C.cream,true)
    text(label,x+math.max(22,#key*7+8)+6,y+1,12,C.ink)
end
local function star(x,y,on)
    -- Original little diamond marker; no borrowed or fake station logos.
    local c=on and C.orange or C.dim
    line(x,y-5,x+5,y,c,2);line(x+5,y,x,y+5,c,2);line(x,y+5,x-5,y,c,2);line(x-5,y,x,y-5,c,2)
    if on then dot(x,y,2,c) end
end
local function footer(s)
    rect(0,684,720,36,C.paper);line(24,684,696,684)
    if s.menu or s.view=='settings' or s.view=='about' or s.view=='details' then
        hint('A','Choose',24,694);hint('B','Back',154,694);hint('L2/R2','Volume',274,694);hint('SELECT','Quit',538,694)
    elseif s.view=='countries' then
        hint('A','Country / retry',24,694);hint('Y','Find',226,694);hint('B','Back',348,694);hint('START','Menu',538,694)
    else
        hint('A','Play/pause',24,694);hint('B','Stop/back',167,694);hint('X','Save',313,694);hint('Y','Search',413,694);hint('START','Menu',552,694)
    end
end
local function title(s)
    text('A WORLD OF INDEPENDENT SOUND',24,17,11,C.dim,true)
    heading('Frequency',22,32,52)
    text('RADIO ATLAS',527,21,12,C.dim,true)
    rect(537,46,159,31,C.ink,5)
    dot(552,61,3,s.status.state=='playing' and C.peach or C.muted)
    local label=({playing='ON AIR',paused='PAUSED',connecting='CONNECTING',buffering='BUFFERING',error='STREAM ERROR',ended='STREAM ENDED'})[s.status.state] or 'READY TO TUNE'
    text(label,563,53,12,C.cream,true)
    line(24,94,696,94)
end
local function player(s)
    rect(24,607,672,67,C.ink,6)
    -- Dial is an app illustration, never represented as a station's branding.
    screen.draw_image('assets/icon.png',33,616,{w=48,h=48})
    local labels={stopped='STOPPED',connecting='CONNECTING',buffering='BUFFERING',playing='PLAYING',paused='PAUSED',ended='STREAM ENDED',error='STREAM ERROR'}
    local state=s.status.state
    text(labels[state] or 'UNAVAILABLE',94,615,10,state=='error' and C.peach or C.muted,true)
    text(s.player and s.player.name or 'Choose a station. Press A to listen.',94,631,17,C.cream,true,437)
    local detail='Direct radio  /  L2 -  R2 +'
    if state=='error' then detail='A retries selected station; try MP3 if codec unsupported.'
    elseif state=='paused' then detail='A resumes this station  /  B stops from Explore'
    elseif state=='ended' then detail='Broadcast ended. Press A on this station to reconnect.'
    elseif state=='buffering' or state=='connecting' then detail='Waiting for audio from the broadcaster. B stops.' end
    text(detail,94,652,10,state=='error' and C.peach or C.muted,false,480)
    text(string.format('%02d',math.floor(s.volume*100+0.5)),613,617,22,C.cream,true)
    text('VOL',654,623,10,C.muted)
    for i=1,10 do rect(600+(i-1)*7,652,4,8,i<=math.floor(s.volume*10+0.5) and C.peach or C.dim) end
end
local function scroll(total,cursor,top,height,visible)
    if total<=visible then return end
    rect(690,top,3,height,C.line)
    local h=math.max(10,height*visible/total)
    rect(690,top+(height-h)*(cursor-1)/math.max(1,total-1),3,h,C.orange)
end
local function empty(title_text,detail,y)
    heading(title_text,39,y,30)
    text(detail,40,y+40,14,C.dim,false,635)
end
local function map(s,rows,Station)
    screen.draw_image('assets/world.png',24,150,{w=672,h=252})
    -- Coordinate dots only; no country-centroid substitution.
    local pins=0
    for i,station in ipairs(rows) do
        local x,y=Station.project(station)
        if x then
            pins=pins+1
            screen.draw_image('assets/pin.png',x-3,y-3,{w=7,h=7})
        end
    end
    local selected=rows[s.selected]
    local x,y=Station.project(selected)
    if x then
        screen.draw_image('assets/pin-selected.png',x-9,y-9,{w=19,h=19})
        line(x-13,y,x-10,y,C.peach);line(x+10,y,x+13,y,C.peach)
        line(x,y-13,x,y-10,C.peach);line(x,y+10,x,y+13,C.peach)
        local caption=selected.country~='' and selected.country or selected.countrycode
        if caption=='' then caption='Reported position' end
        local cx=math.min(488,math.max(34,x+14));local cy=math.max(164,math.min(365,y+12))
        rect(cx,cy,194,25,C.ink,3);text(caption,cx+8,cy+4,12,C.cream,true,177)
    end
    if s.focus=='map' then
        rect(25,151,178,26,C.orange);text('MAP / D-PAD MOVES PINS',33,158,10,C.cream,true)
    end
    rect(24,402,672,28,C.ink)
    text(pins..' reported locations  /  '..(#rows-pins)..' without coordinates',35,409,11,C.muted)
    text('NATURAL EARTH',579,409,10,C.muted)
    if s.focus=='map' then
        text('B returns to the station list',24,438,12,C.orange,true)
    else
        text('UP/DOWN stations   LEFT countries   RIGHT map',24,438,12,C.dim)
    end
    text('L1 / R1  genre',571,438,12,C.dim)
end
local function explorer(s,rows,Station)
    local collection=s.view=='favorites' and 'Favorites' or s.view=='recent' and 'Recently tuned' or nil
    rect(24,105,285,34,C.ink,4)
    text(collection or (s.country=='' and 'All countries' or s.country),36,113,16,C.cream,true,258)
    rect(319,105,174,34,C.cream,4);text(s.genres[s.genre],331,113,16,C.ink,true,150)
    if s.query~='' and not collection then text('“'..s.query..'”',509,114,14,C.orange,false,184)
    else text(collection and (#rows..' stations') or ('DIRECTORY  /  '..(math.floor(s.offset/100)+1)),509,115,12,C.dim,true,183) end
    map(s,rows,Station)
    local status=collection and (#rows..' saved records') or (#rows..' candidates / '..s.raw_count..' checked')
    if not collection then
        if s.loading then status=s.stale and 'Refreshing saved page...' or 'Tuning the directory...'
        elseif s.error then status=s.stale and 'OFFLINE / Saved page may be stale' or 'OFFLINE / Directory unavailable'
        elseif s.skipped>0 then status=status..' / '..s.skipped..' omitted' end
    end
    text(status,24,465,12,s.error and C.orange or C.dim,true,530)
    if not collection then text(s.more and 'MORE IN MENU' or 'END OF PAGE',579,465,10,C.dim,true) end
    if #rows==0 then
        if collection then empty(s.view=='favorites' and 'Your collection starts here.' or 'No stations tuned yet.',s.view=='favorites' and 'Press X on any station to keep it here.' or 'Play a station to add it to your recent history.',497)
        elseif s.loading then empty('Finding your next frequency.','Discovering mirrors and loading one small directory page.',497)
        elseif s.error then empty('The world is a little quiet.','Check your connection. Press A to retry, or open saved stations.',497)
        elseif s.raw_count>0 then empty('No compatible streams on this page.','HLS, Opus and HE-AAC are unsupported. Try another genre or page.',497)
        else empty('No stations found.','Try another country, genre, or clear your search with Y.',497) end
        return
    end
    local visible=(s.status.state=='error' or s.notice) and 2 or 3
    local first=math.max(1,math.min(s.selected-1,#rows-visible+1))
    for i=first,math.min(#rows,first+visible-1) do
        local station=rows[i];local y=488+(i-first)*36;local chosen=i==s.selected
        if chosen then rect(24,y,672,34,C.cream,3);rect(24,y,3,34,C.orange) end
        text(string.format('%02d',i),36,y+8,12,chosen and C.orange or C.dim,true)
        text(station.name,76,y+5,17,C.ink,chosen,346)
        local location=station.countrycode~='' and station.countrycode or (station.custom and 'URL' or '--')
        text(location..' / '..(station.codec~='' and station.codec or 'AUTO'),449,y+8,11,C.dim,true,148)
        if station.geo_lat then dot(620,y+16,3,C.green) else line(617,y+16,623,y+16,C.dim) end
        star(659,y+16,Station.index(s.favorites,station.stationuuid)~=nil)
    end
    scroll(#rows,s.selected,492,visible==2 and 62 or 98,visible)
end
local function countries(s)
    heading('Choose a country',24,113,38)
    text('Stations without coordinates are included here.',25,162,15,C.dim)
    text(s.country_filter~='' and ('FILTER: '..s.country_filter) or 'Y to find a country by name',25,190,13,C.orange,true,650)
    local status=s.country_loading and 'Loading countries...' or s.country_error and 'OFFLINE / A on first row retries' or 'LEFT / RIGHT pages  ·  Page '..(math.floor(s.country_offset/100)+1)
    text(status,25,219,13,C.dim,true,665)
    local first=math.max(1,math.min(s.country_cursor-3,#s.countries-6))
    for i=first,math.min(#s.countries,first+6) do
        local c=s.countries[i];local y=253+(i-first)*45;local chosen=i==s.country_cursor
        if chosen then rect(24,y,672,41,C.cream,3);rect(24,y,3,41,C.orange) end
        text(c.name,40,y+9,19,C.ink,chosen,470)
        if i>1 then text(tostring(c.stationcount)..' listed',544,y+13,13,C.dim,false,138) end
    end
    scroll(#s.countries,s.country_cursor,259,298,7)
    if #s.countries==1 and not s.country_loading and not s.country_error then text('No countries match this search. Y to change it.',40,315,16,C.dim) end
    text('Counts include formats that this player cannot decode.',24,579,12,C.dim)
end
local function settings(s)
    heading('Your receiver',24,113,38)
    text('Small controls. A much bigger world.',25,163,16,C.dim)
    for i,label in ipairs(s.settings_items) do
        local y=212+(i-1)*65;local chosen=i==s.settings_cursor
        rect(24,y,672,55,chosen and C.cream or C.paper,4)
        if chosen then rect(24,y,3,55,C.orange) end
        text(label,40,y+16,20,C.ink,chosen)
        if i==1 then text('<  '..math.floor(s.volume*100+0.5)..'%  >',542,y+16,20,C.orange,true) end
    end
    text('L2 / R2 adjust volume on every screen.',25,496,15,C.dim)
    text('MP3 is prioritized. AAC support depends on its profile.',25,525,14,C.dim)
    text('No HLS, HE-AAC, or Opus. Direct streams only.',25,547,14,C.dim)
    if not s.notice then text('Nothing plays automatically when Frequency opens.',25,579,13,C.green) end
end
local function about(s)
    heading('A world worth listening to.',24,113,36)
    local lines={
        {'DIRECTORY', 'Radio Browser / community-maintained, public-domain data.'},
        {'CARTOGRAPHY', 'Natural Earth 1:110m, v5.1.2 / public domain.'},
        {'HONEST GEOGRAPHY', 'Pins use reported coordinates. Missing locations stay in lists.'},
        {'AUDIO', 'Native playback. Broadcasters own their streams and content.'},
        {'LIMITS', 'MP3, AAC-LC, Vorbis, FLAC, WAV; stereo up to 96 kHz.'},
        {'METADATA', 'AAC may be HE-AAC. Directory labels can be wrong or stale.'},
        {'PRIVACY', 'Mirrors and DNS discovery see requests. Play requests count clicks.'},
    }
    for i,row in ipairs(lines) do local y=179+(i-1)*53;text(row[1],25,y,11,C.orange,true);text(row[2],25,y+18,14,C.ink,false,670) end
    if not s.notice then text('radio-browser.info  /  naturalearthdata.com',25,570,13,C.dim) end
end
local function details(s)
    local station=s.detail
    heading('Station notes',24,113,38)
    if not station then empty('Select a station first.','Browse the atlas, favorites, or recent history.',199);return end
    text(station.name,25,166,24,C.ink,true,668)
    local location=station.country~='' and station.country or 'Country not provided'
    if station.state~='' then location=station.state..', '..location end
    local coordinates=station.geo_lat and string.format('%.4f latitude / %.4f longitude',station.geo_lat,station.geo_long) or 'Not provided / available in country lists only'
    local rows={
        {'LOCATION',location},
        {'COORDINATES',coordinates},
        {'FORMAT',(station.codec~='' and station.codec or 'Unknown')..' / '..(station.bitrate>0 and station.bitrate..' kbps' or 'bitrate not provided')},
        {'TAGS',station.tags~='' and station.tags or 'Not provided'},
        {'LANGUAGES',station.language~='' and station.language or 'Not provided'},
        {'WEBSITE',station.homepage~='' and station.homepage or 'Not provided'},
    }
    for i,row in ipairs(rows) do local y=218+(i-1)*51;text(row[1],25,y,11,C.orange,true);text(row[2],25,y+17,15,C.ink,false,670) end
    text('Metadata is supplied by the directory and may be incomplete.',25,546,13,C.dim)
    if not s.notice then text(station.codec=='AAC' and 'AAC profile is unknown. If playback fails, choose MP3.' or 'No station logo is shown; the dial belongs to Frequency.',25,574,13,C.dim) end
end
local function menu(s)
    rect(174,121,522,481,C.ink,8)
    text('YOUR FREQUENCY',200,144,12,C.muted,true)
    for i,label in ipairs(s.menu_items) do
        local y=173+(i-1)*42
        if i==s.menu_cursor then rect(190,y,490,41,C.cream,4) end
        text(string.format('%02d',i),203,y+11,12,i==s.menu_cursor and C.orange or C.muted,true)
        text(label,245,y+8,20,i==s.menu_cursor and C.ink or C.cream,i==s.menu_cursor)
    end
    text('SELECT quits  /  audio stops on exit',200,579,12,C.muted)
end
function V.draw(s,rows,Station)
    screen.clear(C.paper[1],C.paper[2],C.paper[3])
    title(s)
    if s.view=='countries' then countries(s)
    elseif s.view=='settings' then settings(s)
    elseif s.view=='about' then about(s)
    elseif s.view=='details' then details(s)
    else explorer(s,rows,Station) end
    player(s)
    local browsing=s.view=='explore' or s.view=='favorites' or s.view=='recent'
    if s.status.state=='error' and not s.menu and browsing then
        rect(24,561,672,38,C.orange)
        text('Playback failed',35,565,12,C.cream,true)
        text(s.status.error~='' and s.status.error or 'Broadcaster unavailable. Try again or choose another station.',35,582,11,C.cream,false,645)
    end
    if s.notice then rect(24,561,672,38,C.ink);text(s.notice,36,573,13,C.cream,false,643) end
    if s.menu then menu(s) end
    footer(s)
end
return V
