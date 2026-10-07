-- Protocol boundary: retain only bounded, displayable fields from VibeBoy.
local M = {}
M.limits = {servers=8, sessions=128, lines=240, columns=512, options=24}
function M.clean(v, limit)
    if type(v) ~= 'string' then return '' end
    v = v:gsub('\27%[[%d;?]*[ -/]*[@-~]', ''):gsub('[%z\1-\8\11-\31\127]', ''):gsub('\t', '    ')
    local chars = {}
    local ok = pcall(function()
        for _, c in utf8.codes(v) do
            if #chars >= (limit or 160) then break end
            chars[#chars+1] = utf8.char(c)
        end
    end)
    return ok and table.concat(chars) or '[invalid text]'
end
function M.slice(s, first, count)
    local start = utf8.offset(s, first)
    if not start then return '' end
    local stop = utf8.offset(s, first+count)
    return s:sub(start, stop and stop-1 or #s)
end
function M.wrap(s, width, maxlines)
    local lines, line = {}, ''
    for word in (M.clean(s, 4096)..' '):gmatch('(.-)%s+') do
        if utf8.len(line)+utf8.len(word)+1 > width and line ~= '' then
            lines[#lines+1] = line; line = ''
        end
        while utf8.len(word) > width do
            if line ~= '' then lines[#lines+1]=line; line='' end
            lines[#lines+1]=M.slice(word,1,width); word=M.slice(word,width+1,4096)
        end
        line = line == '' and word or line..' '..word
        if #lines >= maxlines then break end
    end
    if line ~= '' and #lines < maxlines then lines[#lines+1]=line end
    return lines
end
function M.url(value)
    if type(value)~='string' or #value>240 then return nil end
    local v = M.clean(value, 240):match('^%s*(.-)%s*$')
    if not v:match('^https?://') then v='http://'..v end
    v = v:gsub('/+$','')
    local scheme, authority = v:match('^(https?)://([^/]+)$')
    if not scheme or authority:find('[%s@?#]') then return nil end
    local host, port
    if authority:match('^[%x:%.]+$') and select(2,authority:gsub(':',''))>=2 then
        -- Accept bare IPv6 as well as bracketed addresses.
        host='['..authority..']'
    elseif authority:sub(1,1)=='[' then
        host,port=authority:match('^(%[[%x:%.]+%]):(%d+)$')
        if not host then host=authority:match('^(%[[%x:%.]+%])$') end
    else
        host,port=authority:match('^([%w%.%-]+):(%d+)$')
        if not host then host=authority:match('^([%w%.%-]+)$') end
    end
    if not host or host == '' then return nil end
    if port and (tonumber(port)<1 or tonumber(port)>65535) then return nil end
    return scheme..'://'..host..':'..(port or '8766')
end
local priorities = {waiting=1,error=2,thinking=3,running=4,stale=5,idle=6}
function M.commands(s)
    local result = {}
    local function add(label, action, payload, kind)
        result[#result+1]={label=label,action=action,payload=payload or {},kind=kind or 'CONTROL'}
    end
    for _,o in ipairs(s.response_options) do add(o.text,'send_response',{text=o.text},'REPLY / '..o.category:upper()) end
    for _,o in ipairs(s.detected_choices) do add(o.label,'send_keys',{keys=o.key_sequence},'PROMPT CHOICE') end
    add('Write a response / command','compose',nil,'KEYBOARD')
    if s.session_type=='claude_code' then add('Send Tab / accept ghost text','accept_ghost') end
    add('Send Escape','escape')
    add('Interrupt process / Ctrl+C','interrupt',nil,'DISRUPTIVE')
    add('Suspend process / Ctrl+Z','suspend',nil,'DISRUPTIVE')
    return result
end
function M.signature(c)
    return c.action..'\0'..(c.payload.text or c.payload.keys or '')
end
function M.parse(data)
    if type(data)~='table' or type(data.sessions)~='table' then return nil,'Expected a VibeBoy sessions map' end
    local list, total = {}, 0
    -- Reject malformed rows rather than displaying an apparently empty success.
    for id,s in pairs(data.sessions) do
        if type(id)~='string' or #id>512 or type(s)~='table' or type(s.session_name)~='string' or type(s.pane_id)~='string' then
            return nil,'Invalid session data'
        end
        total=total+1
        if total<=M.limits.sessions then
            local row={id=id, session_name=M.clean(s.session_name,96), pane_id=M.clean(s.pane_id,96),
                pane_command=M.clean(s.pane_command,96),session_type=M.clean(s.session_type,48),
                status=M.clean(s.status,24),permission_mode=M.clean(s.permission_mode,24),
                last_updated=tonumber(s.last_updated),screen_content={},response_options={},detected_choices={}}
            local content=type(s.screen_content)=='table' and s.screen_content or {}
            local last=#content
            -- tmux captures the entire pane height; don't follow hundreds of blank rows.
            while last>0 and (type(content[last])~='string' or not content[last]:match('%S')) do last=last-1 end
            for i=math.max(1,last-M.limits.lines+1),last do row.screen_content[#row.screen_content+1]=M.clean(content[i],M.limits.columns) end
            for i,o in ipairs(type(s.response_options)=='table' and s.response_options or {}) do
                if i>M.limits.options then break end
                -- Preserve exact command text; never send a silently truncated suggestion.
                if type(o)=='table' and type(o.text)=='string' and #o.text>0 and #o.text<=4096 then
                    row.response_options[#row.response_options+1]={text=o.text,category=M.clean(o.category,32)}
                end
            end
            for i,o in ipairs(type(s.detected_choices)=='table' and s.detected_choices or {}) do
                if i>M.limits.options then break end
                if type(o)=='table' and type(o.key_sequence)=='string' and #o.key_sequence>0 and #o.key_sequence<=1024 then
                    row.detected_choices[#row.detected_choices+1]={label=M.clean(o.label,512),key_sequence=o.key_sequence}
                end
            end
            row.commands=M.commands(row)
            list[#list+1]=row
        end
    end
    table.sort(list,function(a,b)
        local ap,bp=priorities[a.status] or 7,priorities[b.status] or 7
        if ap~=bp then return ap<bp end
        if a.session_name~=b.session_name then return a.session_name<b.session_name end
        return a.id<b.id
    end)
    return list,nil,total
end
return M
