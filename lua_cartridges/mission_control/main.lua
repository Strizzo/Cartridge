local model = require('model')
local view = require('view')
local S = {page='settings',servers={},server=1,settings_cursor=1,interval=2,now=0,
    enabled=false,epoch=0,revision=0,health='DISCONNECTED',message='',notice='',failures=0,
    next_poll=0,sessions={},visible={},groups={'All projects'},group=1,filter=1,cursor=1,
    command=1,scroll=0,column=0,total=0,dirty=true,confirm=nil}
local FILTERS={'ALL UNITS','AGENTS','PROCESSES','NEEDS INPUT'}
local function dirty() S.dirty=true end
local function save()
    local ok=pcall(storage.save,'mission_control_settings',{version=1,servers=S.servers,selected=S.server,interval=S.interval})
    if not ok then S.notice='Could not save settings. Check storage space.' end
end
local function selected()
    for _,s in ipairs(S.sessions) do if s.id==S.selected_id then return s end end
end
local function ready()
    return S.enabled and S.health=='LIVE' and S.last_success and S.now-S.last_success<=math.max(10,S.interval*3)
end
local function rebuild()
    local previous=S.groups[S.group]
    local groups,seen={'All projects'},{}
    for _,s in ipairs(S.sessions) do if not seen[s.session_name] then groups[#groups+1]=s.session_name;seen[s.session_name]=true end end
    table.sort(groups,function(a,b) if a==b then return false elseif a=='All projects' then return true elseif b=='All projects' then return false else return a<b end end)
    S.groups=groups;S.group=1
    for i,g in ipairs(groups) do if g==previous then S.group=i end end
    S.visible={}
    for _,s in ipairs(S.sessions) do
        local match=S.filter==1 or (S.filter==2 and s.session_type=='claude_code') or
            (S.filter==3 and s.session_type~='claude_code') or (S.filter==4 and s.status=='waiting')
        if match and (S.group==1 or s.session_name==groups[S.group]) then S.visible[#S.visible+1]=s end
    end
    S.cursor=math.max(1,math.min(S.cursor,#S.visible));local found=false
    for i,s in ipairs(S.visible) do if s.id==S.selected_id then S.cursor=i;found=true end end
    if not found and S.page~='detail' and S.page~='output' then S.selected_id=S.visible[S.cursor] and S.visible[S.cursor].id end
end
local function choose(index)
    S.cursor=math.max(1,math.min(index,#S.visible))
    S.selected_id=S.visible[S.cursor] and S.visible[S.cursor].id
    S.command=1;S.scroll=0;S.column=0;S.confirm=nil;S.notice=''
end
local function disconnect()
    S.enabled=false;S.epoch=S.epoch+1;S.health='DISCONNECTED';S.message='Polling stopped. A connects the selected server.'
    S.confirm=nil;S.last_success=nil;S.sessions={};S.selected_id=nil;S.total=0;rebuild();dirty()
    -- Keep outstanding request handles until drained: server changes cannot flood workers.
end
local function connect(index)
    if not S.servers[index] then return end
    disconnect();S.server=index;S.settings_cursor=index;S.enabled=true;S.health='CONNECTING'
    S.message='Waiting for the first state from VibeBoy.';S.notice='';S.failures=0;S.next_poll=S.now
    S.page='board';S.group=1;S.filter=1;save()
end
local function fail(message)
    S.failures=S.failures+1;S.health='OFFLINE';S.message=message
    S.next_poll=S.now+math.min(30,2^math.min(S.failures,5));S.confirm=nil
end
local function poll_state()
    if not S.enabled or S.state_request or S.action_request or S.now<S.next_poll then return end
    local ok,id=pcall(http.get_async,S.servers[S.server].url..'/api/state')
    if ok then S.state_request={id=id,epoch=S.epoch,revision=S.revision,started=S.now}
    else fail('Request queue unavailable. Retrying automatically.') end
    dirty()
end
local function proposal(command, custom)
    local session=selected()
    if not session or not ready() then S.notice='Connect and refresh before sending commands.';return end
    if S.action_request then S.notice='A command is still awaiting acknowledgement.';return end
    S.confirm={kind='action',epoch=S.epoch,id=session.id,name=session.session_name,
        command=command,signature=model.signature(command),custom=custom,send=false,page=1}
end
local function action_send(c)
    local session=selected()
    if not ready() or c.epoch~=S.epoch or not session or c.id~=session.id or S.action_request then
        S.notice='Target changed or connection unavailable. Command cancelled.';return
    end
    if not c.custom then
        local found=false
        for _,option in ipairs(session.commands) do if model.signature(option)==c.signature then found=true end end
        if not found then S.notice='Prompt changed. Review the current command.';return end
    end
    local body=json.encode({action=c.command.action,session_id=c.id,payload=c.command.payload})
    local ok,id=pcall(http.post_async,S.servers[S.server].url..'/api/action',body)
    if ok then
        S.revision=S.revision+1;S.action_request={id=id,epoch=S.epoch,target=c.id,action=c.command.action,started=S.now}
        S.notice='Sending command. Waiting for server acknowledgement.'
    else S.notice='Command was not queued. Please try again.' end
end
local function edit(index)
    if not index and #S.servers>=model.limits.servers then S.notice='Eight servers saved. Remove one first.';return end
    local old=S.servers[index] or {name='',url=''}
    S.draft={index=index,name=old.name,url=old.url,field=1};S.page='edit';S.notice=''
end
local function keyboard(kind,label,value)
    S.keyboard={kind=kind,epoch=S.epoch,id=S.selected_id}
    text_input.show(label,value or '',false,kind=='compose' and 1024 or kind=='url' and 240 or 48)
end
local function keyboard_result(value)
    local k=S.keyboard;S.keyboard=nil
    if type(value)~='string' then return end
    if k.kind=='name' and S.draft then S.draft.name=model.clean(value,48)
    elseif k.kind=='url' and S.draft then
        local url=model.url(value)
        if url then S.draft.url=url;S.notice='' else S.notice='Enter host[:port] or http(s)://host:port. No path.' end
    elseif k.kind=='compose' then
        if k.epoch~=S.epoch or k.id~=S.selected_id then S.notice='Target changed. Response discarded.'
        elseif value:match('%S') and #value<=4096 then
            proposal({label=value,action='send_response',payload={text=value},kind='CUSTOM RESPONSE'},true)
        else S.notice='Response must contain 1 to 4096 bytes.' end
    end
    dirty()
end
local function accept_state(resp, request)
    if request.epoch~=S.epoch or request.revision~=S.revision or not S.enabled then return end
    if not resp.ok then fail('Cannot reach VibeBoy (HTTP '..tostring(resp.status or 0)..'). Check server and network.');return end
    local ok,data=pcall(json.decode,resp.body)
    if not ok then fail('Invalid JSON from server. Verify the HTTP address.');return end
    local list,err,total=model.parse(data)
    if not list then fail(err);return end
    -- Keep selection and option identity stable when priority/order changes.
    local old=selected();local old_command=old and old.commands[S.command]
    S.sessions=list;S.total=total;S.last_success=S.now;S.health='LIVE';S.failures=0
    S.message='State received';S.next_poll=S.now+S.interval
    S.latency=math.floor(tonumber(resp.elapsed_ms) or 0)
    rebuild()
    local current=selected()
    if current then
        S.command=1
        if old_command then for i,c in ipairs(current.commands) do if model.signature(c)==model.signature(old_command) then S.command=i end end end
        S.scroll=math.min(S.scroll,math.max(0,#current.screen_content-1))
    elseif S.page=='detail' or S.page=='output' then
        S.notice='This pane has closed. B returns to the operations board.';S.confirm=nil
    end
    local c=S.confirm
    if c and c.kind=='action' then
        local valid=current and current.id==c.id
        if valid and not c.custom then
            valid=false
            for _,option in ipairs(current.commands) do if model.signature(option)==c.signature then valid=true end end
        end
        if not valid then S.confirm=nil;S.notice='Prompt or target changed. Review the latest state.' end
    end
end
local function accept_action(resp, request)
    if request.epoch~=S.epoch then return end
    local ok,data=pcall(json.decode,resp.body or '')
    if resp.ok and ok and type(data)=='table' and data.type=='ack' and data.session_id==request.target and data.action==request.action then
        S.notice='Server acknowledged '..request.action..'. Check output for the result.'
    elseif ok and type(data)=='table' and data.type=='error' then
        S.notice='Server rejected command: '..model.clean(data.message,180)
    else
        S.notice='Delivery unknown. Check output before sending again.'
    end
    -- POST is never automatically retried: it might already have taken effect.
    S.next_poll=S.now
end
function on_init()
    app.set_idle_fps(5)
    local ok,data=pcall(storage.load,'mission_control_settings')
    if ok and type(data)=='table' then
        for _,server in ipairs(type(data.servers)=='table' and data.servers or {}) do
            if #S.servers>=model.limits.servers then break end
            if type(server)=='table' then
                local url=model.url(server.url or '')
                if url then S.servers[#S.servers+1]={name=model.clean(server.name,48),url=url} end
            end
        end
        for _,n in ipairs({1,2,5,10}) do if data.interval==n then S.interval=n end end
        S.server=math.max(1,math.min(math.floor(tonumber(data.selected) or 1),#S.servers))
    end
    if #S.servers>0 then connect(S.server) end
end
function on_update(dt)
    S.now=S.now+math.max(0,tonumber(dt) or 0)
    if S.keyboard then local r=text_input.poll();if r~=nil then keyboard_result(r) end end
    for _,resp in ipairs(http.poll()) do
        if S.state_request and resp.id==S.state_request.id then
            local request=S.state_request;S.state_request=nil;accept_state(resp,request);dirty()
        elseif S.action_request and resp.id==S.action_request.id then
            local request=S.action_request;S.action_request=nil;accept_action(resp,request);dirty()
        end
    end
    if S.enabled and S.last_success and S.now-S.last_success>math.max(10,S.interval*3) and S.health=='LIVE' then
        S.health='STALE';S.message='No recent state. Commands paused until the connection recovers.';S.confirm=nil;dirty()
    end
    poll_state()
    local result=S.dirty;S.dirty=false;return result
end
function on_input(button,action)
    if action~='press' and action~='repeat' then return end
    local b=button:gsub('^dpad_','')
    if action=='repeat' and b~='up' and b~='down' and b~='left' and b~='right' then return end
    if S.keyboard then return end
    dirty()
    if S.confirm then
        local c=S.confirm
        if b=='b' then S.confirm=nil
        elseif b=='left' then c.send=false
        elseif b=='right' then c.send=true
        elseif b=='up' then c.page=math.max(1,c.page-1)
        elseif b=='down' then c.page=c.page+1
        elseif b=='a' and action=='press' then
            S.confirm=nil
            if c.send then
                if c.kind=='action' then action_send(c)
                elseif c.kind=='delete' then
                    if c.index==S.server then disconnect() end
                    table.remove(S.servers,c.index)
                    if c.index<S.server then S.server=S.server-1 end
                    S.server=math.max(1,math.min(S.server,#S.servers));S.settings_cursor=math.max(1,math.min(c.index,#S.servers));save()
                end
            end
        end
        return
    end
    if S.page=='edit' then
        if b=='b' then S.draft=nil;S.page='settings';S.notice=''
        elseif b=='up' then S.draft.field=math.max(1,S.draft.field-1)
        elseif b=='down' then S.draft.field=math.min(3,S.draft.field+1)
        elseif b=='a' then
            if S.draft.field==1 then keyboard('name','Server name',S.draft.name)
            elseif S.draft.field==2 then keyboard('url','Server host:port (default 8766)',S.draft.url)
            else
                local url=model.url(S.draft.url)
                if not url then S.notice='Set a valid server address before saving.';return end
                local i=S.draft.index or (#S.servers+1)
                if i==S.server and S.enabled then disconnect() end
                S.servers[i]={name=S.draft.name~='' and S.draft.name or url,url=url}
                S.settings_cursor=i;S.page='settings';S.draft=nil;S.notice='Server saved. A connects.';save()
            end
        end
        return
    end
    if S.page=='settings' then
        if b=='up' then S.settings_cursor=math.max(1,S.settings_cursor-1)
        elseif b=='down' then S.settings_cursor=math.min(#S.servers,S.settings_cursor+1)
        elseif b=='a' then if #S.servers==0 then edit() else connect(S.settings_cursor) end
        elseif b=='x' and #S.servers>0 then edit(S.settings_cursor)
        elseif b=='y' then edit()
        elseif b=='l2' and S.servers[S.settings_cursor] then S.confirm={kind='delete',index=S.settings_cursor,name=S.servers[S.settings_cursor].name,send=false,page=1}
        elseif b=='r2' then disconnect();S.notice='Disconnected. No further requests will be started.'
        elseif b=='start' then
            local rates={1,2,5,10};for i,n in ipairs(rates) do if n==S.interval then S.interval=rates[i%#rates+1];break end end;save()
        elseif b=='b' and #S.servers>0 then S.page='board';rebuild() end
        return
    end
    if b=='start' then S.page='settings';S.settings_cursor=S.server;return end
    if b=='x' then
        if not S.enabled then connect(S.server) else S.next_poll=S.now;S.notice='Refresh requested.' end
        return
    end
    if S.page=='board' then
        if b=='up' then choose(S.cursor-1)
        elseif b=='down' then choose(S.cursor+1)
        elseif b=='left' or b=='right' then S.filter=(S.filter-1+(b=='right' and 1 or -1))%#FILTERS+1;rebuild();choose(1)
        elseif b=='l1' or b=='r1' then S.group=(S.group-1+(b=='r1' and 1 or -1))%#S.groups+1;rebuild();choose(1)
        elseif b=='a' and selected() then S.page='detail';S.command=1;S.scroll=0;S.column=0
        elseif b=='b' then S.page='settings' end
    else
        local session=selected()
        if b=='b' then
            S.page=S.page=='output' and 'detail' or 'board';S.scroll=0;S.column=0;rebuild()
            if not session then choose(1) end
        elseif b=='y' and session then S.page=S.page=='output' and 'detail' or 'output';S.scroll=0;S.column=0
        elseif b=='up' and session then S.scroll=math.min(math.max(0,#session.screen_content-1),S.scroll+3)
        elseif b=='down' then S.scroll=math.max(0,S.scroll-3)
        elseif S.page=='output' then
            if b=='left' then S.column=math.max(0,S.column-12)
            elseif b=='right' then S.column=math.min(456,S.column+12)
            elseif b=='a' then S.scroll=0;S.column=0 end
        elseif session then
            if b=='left' or b=='l2' then S.command=math.max(1,S.command-1)
            elseif b=='right' or b=='r2' then S.command=math.min(#session.commands,S.command+1)
            elseif b=='l1' or b=='r1' then choose(S.cursor+(b=='r1' and 1 or -1))
            elseif b=='a' then
                local command=session.commands[S.command]
                if command.action=='compose' then
                    if ready() and not S.action_request then keyboard('compose','Response to '..session.session_name,'') else S.notice='Wait for a live connection and any pending command.' end
                else proposal(command,false) end
            end
        end
    end
end
function on_render() view.draw(S,selected(),FILTERS,ready()) end
function on_destroy() S.enabled=false;S.epoch=S.epoch+1 end
