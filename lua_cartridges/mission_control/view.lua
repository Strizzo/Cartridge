-- Fixed 720px composition. Rendering reads prepared state only; no HTTP or measurements.
local model=require('model')
local V={}
local C={paper={239,232,210},ink={26,37,37},muted={91,106,100},red={197,57,37},
    teal={29,104,94},line={193,192,172},white={250,246,230},gold={225,168,55},dark={19,32,33}}
local function rect(x,y,w,h,c) screen.draw_rect(x,y,w,h,{color=c,filled=true}) end
local function text(s,x,y,size,c,w,bold)
    screen.draw_text(model.clean(tostring(s or ''),4096),x,y,{size=size or 16,color=c or C.ink,max_width=w or 684,bold=bold or false})
end
local function display(s,x,y,size,c) screen.draw_display_text(s,x,y,size,c or C.ink) end
local function line(x,y,w,c) rect(x,y,w,1,c or C.line) end
local function card(x,y,w,h,c,border) ui.card(x,y,w,h,{bg=c or C.white,border=border or C.line,shadow=false,radius=0}) end
local function badge(label,x,y,w,c)
    rect(x,y,w,25,c);text(label,x+8,y+4,12,C.white,w-16,true)
end
local function mark(x,y,scale,c)
    local q=scale/48
    rect(x,y,48*q,48*q,c or C.red)
    rect(x+9*q,y+9*q,9*q,30*q,C.paper);rect(x+30*q,y+9*q,9*q,30*q,C.paper)
    rect(x+18*q,y+18*q,12*q,9*q,C.paper);rect(x+21*q,y+9*q,6*q,6*q,C.paper)
end
local function status_color(s)
    if s=='waiting' or s=='error' then return C.red end
    if s=='running' or s=='thinking' then return C.teal end
    return C.muted
end
local function footer(items)
    rect(0,681,720,39,C.ink)
    local x=18
    for _,h in ipairs(items) do
        local width=math.max(23,#h[1]*7+10)
        rect(x,689,width,22,C.paper);text(h[1],x+5,692,11,C.ink,width-6,true)
        text(h[2],x+width+7,694,11,C.paper,h[3] or 70,true)
        x=x+width+(h[3] or 70)+17
    end
end
local function wrapped(s,x,y,width,rows,size,c)
    for i,t in ipairs(model.wrap(s,width,rows)) do text(t,x,y+(i-1)*(size+7),size,c) end
end
local function heading(S,title,subtitle)
    text('MC / 01     REMOTE OPERATIONS',18,16,12,C.muted,420,true)
    text('CARTRIDGE SYSTEMS',516,16,12,C.ink,186,true)
    mark(18,46,48)
    display(title,80,40,52)
    text(subtitle,20,108,14,C.muted,550)
    text('720 / CONTROL',566,109,12,C.red,136,true)
    line(18,133,684,C.ink)
    local online=S.health=='LIVE'
    badge(S.health,18,145,132,online and C.teal or C.red)
    local server=S.servers[S.server]
    local status=server and server.name or 'NO SERVER CONFIGURED'
    if online then status=status..'   /   '..tostring(S.latency or 0)..' ms   /   '..math.floor(S.now-(S.last_success or S.now))..'s ago'
    elseif S.enabled then
        status=S.message
        if S.health=='OFFLINE' then status=status..' Retry '..math.max(0,math.ceil(S.next_poll-S.now))..'s' end
    end
    text(status,163,149,14,C.ink,538)
end
local function notice(S,fallback)
    if S.notice~='' then
        rect(18,628,684,44,C.gold);wrapped(S.notice,28,634,85,2,12,C.ink)
    elseif fallback then text(fallback,18,651,12,C.muted,684) end
end
local function board(S)
    heading(S,'MISSION CONTROL','Your sessions. One operations board.')
    local waiting,active=0,0
    for _,s in ipairs(S.sessions) do
        if s.status=='waiting' then waiting=waiting+1 end
        if s.status=='running' or s.status=='thinking' then active=active+1 end
    end
    for i,m in ipairs({{waiting,'NEEDS INPUT',C.red},{active,'IN PROGRESS',C.teal},{#S.sessions,'TOTAL PANES',C.ink}}) do
        local x=18+(i-1)*232
        rect(x,186,220,85,m[3]);display(string.format('%02d',m[1]),x+12,190,49,C.paper)
        text(m[2],x+85,221,12,C.paper,128,true)
    end
    local filters={'ALL','AGENTS','PROCESSES','INPUT'}
    for i,label in ipairs(filters) do
        local x=18+(i-1)*174
        rect(x,286,162,29,i==S.filter and C.ink or C.line)
        text(label,x+10,292,13,i==S.filter and C.paper or C.muted,144,true)
    end
    text('L1 / R1',18,330,12,C.red,78,true)
    text(S.groups[S.group],100,325,19,C.ink,425,true)
    text(S.group..' / '..#S.groups..' GROUPS',539,329,12,C.muted,163,true)
    if #S.visible==0 then
        card(18,361,684,224,C.white)
        display(S.enabled and (S.health=='CONNECTING' and 'ESTABLISHING LINK' or 'NO UNITS HERE') or 'LINK DISCONNECTED',36,392,32)
        local message
        if S.health=='CONNECTING' then message='Fetching sessions from the selected VibeBoy server. Controls remain available.'
        elseif not S.enabled then message='Press X to reconnect or Start to choose a server.'
        elseif S.health=='OFFLINE' then message=S.message..' Press X to retry now. Start opens server settings.'
        elseif #S.sessions==0 then message='VibeBoy is reachable but reports no panes. Start a tmux session on the server.'
        else message='No panes match this filter and project. Left / Right changes filter; L1 / R1 changes group.' end
        wrapped(message,36,453,64,4,17,C.muted)
    else
        local start=math.floor((S.cursor-1)/4)*4+1
        for i=start,math.min(start+3,#S.visible) do
            local s=S.visible[i];local y=358+(i-start)*59;local focus=i==S.cursor
            card(18,y,684,54,focus and C.ink or C.white,focus and C.ink or C.line)
            rect(18,y,5,54,status_color(s.status))
            text(string.format('%02d',i),30,y+9,13,focus and C.gold or C.muted,30,true)
            text(s.session_name,70,y+6,18,focus and C.paper or C.ink,398,true)
            local kind=s.session_type=='claude_code' and 'AGENT' or 'PROCESS'
            text(kind..' / '..s.pane_command..' / '..s.pane_id,71,y+31,11,focus and C.line or C.muted,440)
            badge(s.status:upper(),548,y+15,141,status_color(s.status))
        end
        text('UP / DOWN SELECT    '..S.cursor..' / '..#S.visible..' PANES',18,607,11,C.muted,500,true)
        text('A  INSPECT >',555,604,14,C.red,147,true)
    end
    notice(S,S.total>128 and 'Showing 128 panes. Narrow the server workload to see all panes.' or 'Project groups follow tmux session names. SELECT quits to CartridgeOS.')
    footer({{'A','INSPECT',66},{'X','REFRESH',68},{'L/R','FILTER',64},{'START','SERVERS',72},{'B','BACK',40}})
end
local function output(session,S,y,rows,full)
    local lines=session.screen_content
    rect(18,y,684,rows*23+41,C.dark)
    local finish=math.max(0,#lines-S.scroll)
    local first=math.max(1,finish-rows+1)
    text('TERMINAL / '..(S.scroll==0 and 'FOLLOWING LATEST' or 'SCROLLED'),30,y+10,11,C.gold,440,true)
    text((#lines==0 and 0 or first)..'-'..finish..' / '..#lines,526,y+10,11,C.line,162,true)
    if #lines==0 then text('No terminal output reported yet.',31,y+50,17,C.line,640) end
    for i=first,finish do text(model.slice(lines[i],S.column+1,full and 74 or 78),31,y+34+(i-first)*23,16,C.paper,657) end
end
local function detail(S,session,live)
    heading(S,S.page=='output' and 'TERMINAL FEED' or 'UNIT INSPECTOR',session and session.session_name or 'Pane unavailable')
    if not session then
        display('PANE CLOSED',24,252,44,C.red);wrapped('This pane is no longer in the latest server state. Return to the board to select another unit.',24,325,62,4,18,C.muted)
        notice(S);footer({{'B','BOARD',70},{'START','SERVERS',72}});return
    end
    badge(session.status:upper(),18,187,126,status_color(session.status))
    text(session.pane_command..' / '..session.pane_id,158,192,14,C.ink,394,true)
    text(session.permission_mode~='' and ('MODE '..session.permission_mode:upper()) or 'LIVE PANE',556,194,11,C.muted,146,true)
    if S.page=='output' then
        output(session,S,226,15,true)
        text('UP / DOWN HISTORY     LEFT / RIGHT PAN     A LATEST     COL '..(S.column+1),18,623,11,C.muted,684,true)
        notice(S)
        footer({{'A','LATEST',60},{'Y','INSPECT',65},{'X','REFRESH',66},{'B','BACK',44},{'SEL','QUIT',42}})
    else
        output(session,S,226,8,false)
        local command=session.commands[S.command]
        rect(18,471,684,143,C.white)
        rect(18,471,5,143,command.kind=='DISRUPTIVE' and C.red or C.teal)
        text(command.kind,32,484,12,C.red,510,true)
        text(S.command..' / '..#session.commands,608,484,12,C.muted,82,true)
        wrapped(command.label,32,511,59,2,18,C.ink)
        text('L2 / R2  COMMANDS',32,588,11,C.muted,310,true)
        text(live and 'A  REVIEW >' or 'COMMANDS PAUSED',484,583,14,live and C.red or C.muted,202,true)
        notice(S,'UP / DOWN output     L1 / R1 pane     Y full terminal     SELECT quit')
        footer({{'A','REVIEW',60},{'Y','OUTPUT',65},{'X','REFRESH',66},{'B','BOARD',50},{'START','SERVERS',66}})
    end
end
local function settings(S)
    heading(S,'SERVER STATIONS','Choose where Mission Control connects.')
    text('SAVED ENDPOINTS',18,191,13,C.ink,360,true)
    text('POLL EVERY '..S.interval..'s / START',429,191,13,C.red,273,true)
    if #S.servers==0 then
        display('ADD YOUR FIRST SERVER',24,264,34)
        wrapped('Run VibeBoy on your computer, then enter its reachable host and HTTP port. Default port: 8766.',24,331,61,4,18,C.muted)
        wrapped('A or Y opens setup. The controller keyboard handles names, hostnames, IPv4 and bare IPv6 addresses.',24,440,65,3,16,C.muted)
    else
        local first=math.floor((S.settings_cursor-1)/5)*5+1
        for i=first,math.min(first+4,#S.servers) do
            local srv=S.servers[i];local y=221+(i-first)*64;local focus=S.settings_cursor==i
            card(18,y,684,58,focus and C.ink or C.white)
            text(string.format('%02d',i),30,y+10,14,focus and C.gold or C.red,40,true)
            text(srv.name,77,y+7,18,focus and C.paper or C.ink,520,true)
            text(srv.url,77,y+34,12,focus and C.line or C.muted,596)
            if i==S.server and S.enabled then text('LINK',635,y+9,11,focus and C.gold or C.teal,53,true) end
        end
    end
    line(18,558,684)
    text('R2 DISCONNECT    L2 REMOVE    START POLL RATE',18,574,13,C.red,684,true)
    text('Direct HTTP(S) or a tunnel established outside the app.',18,601,13,C.muted,684)
    notice(S,'Trusted private network only: the current VibeBoy API has no authentication.')
    footer({{'A','CONNECT',66},{'X','EDIT',42},{'Y','ADD',40},{'B','BOARD',56},{'SEL','QUIT',42}})
end
local function edit(S)
    heading(S,S.draft.index and 'EDIT STATION' or 'NEW STATION','D-pad chooses a field. A opens the controller keyboard.')
    for i,row in ipairs({{'01 / NAME',S.draft.name~='' and S.draft.name or 'Choose a server name'},
        {'02 / ADDRESS',S.draft.url~='' and S.draft.url or 'host:8766'}, {'03 / SAVE','Save server settings'}}) do
        local y=218+(i-1)*109;local focus=S.draft.field==i
        card(18,y,684,91,focus and C.ink or C.white)
        text(row[1],32,y+13,12,focus and C.gold or C.red,620,true)
        text(row[2],32,y+42,22,focus and C.paper or C.ink,640,true)
    end
    wrapped('Address examples: 192.168.1.10:8766 or workstation.local:8766. HTTP and HTTPS are supported. Paths and credentials are not accepted.',22,564,87,3,13,C.muted)
    notice(S)
    footer({{'A','EDIT / SAVE',102},{'UP/DN','FIELD',56},{'B','CANCEL',66},{'SEL','QUIT',42}})
end
local function confirm(S)
    local c=S.confirm
    screen.clear(C.ink[1],C.ink[2],C.ink[3])
    text('MISSION CONTROL / COMMAND REVIEW',24,24,13,C.paper,660,true)
    rect(18,64,684,72,C.red);display(c.kind=='delete' and 'REMOVE STATION?' or 'TRANSMIT COMMAND?',32,72,42,C.paper)
    text('TARGET',24,163,12,C.gold,90,true);text(c.name,116,156,23,C.paper,580,true)
    if c.kind=='action' then
        text(c.id,24,194,12,C.line,672)
        text(c.command.action:upper(),24,235,14,C.gold,672,true)
        local content=c.command.payload.text or c.command.payload.keys or c.command.label
        local lines=model.wrap(content,63,200)
        local pages=math.max(1,math.ceil(#lines/9));local page=math.min(c.page,pages)
        rect(18,271,684,274,C.dark)
        for i=(page-1)*9+1,math.min(page*9,#lines) do text(lines[i],32,287+(i-(page-1)*9-1)*26,18,C.paper,655) end
        text('UP / DOWN REVIEW TEXT     '..page..' / '..pages,24,553,11,C.line,660,true)
        text('Sends input to the live pane. Review before confirming.',24,586,16,C.paper,672)
    else
        wrapped('Remove this saved address from the handheld? The remote server and its sessions are unaffected.',24,252,58,5,21,C.paper)
    end
    rect(18,625,332,42,c.send and C.dark or C.paper)
    text('< CANCEL',36,635,18,c.send and C.paper or C.ink,290,true)
    rect(370,625,332,42,c.send and C.red or C.dark)
    text(c.kind=='delete' and 'REMOVE >' or 'SEND >',390,635,18,C.paper,290,true)
    footer({{'L/R','CHOOSE',76},{'A','CONFIRM',78},{'B','CANCEL',72}})
end
function V.draw(S,session,filters,live)
    screen.clear(C.paper[1],C.paper[2],C.paper[3])
    if S.confirm then confirm(S)
    elseif S.page=='settings' then settings(S)
    elseif S.page=='edit' then edit(S)
    elseif S.page=='board' then board(S)
    else detail(S,session,live) end
end
return V
