-- Shared app chrome. Content layouts remain under each cartridge's control.
local ui = {}
local neo = theme.ui == "neo"
local rect = screen.draw_rect
local text = screen.draw_text

function ui.header(title, status, status_color)
    rect(0, 0, 720, 40, {color=theme.bg, filled=true})
    local right_width = status and math.min(260, screen.get_text_width(status, 12, false)) or 0
    local max_title = 680 - (status and (right_width + 24) or 0)
    if neo then
        local label = title:upper()
        -- Display titles are measured once through the font cache. UTF-8-safe
        -- truncation only matters for an unusually long paper ID/title.
        if screen.get_display_text_width(label, 30) > max_title then
            local chars = {}
            for _,code in utf8.codes(label) do chars[#chars+1] = utf8.char(code) end
            while #chars > 0 and screen.get_display_text_width(table.concat(chars).."..",30) > max_title do
                chars[#chars] = nil
            end
            label = table.concat(chars)..".."
        end
        screen.draw_display_text(label, 18, 1, 30, theme.text)
        rect(0,36,720,4,{color=theme.accent,filled=true})
        for x=650,710,12 do screen.draw_line(x,36,x+4,39,{color=theme.bg}) end
    else
        text(title,12,10,{color=theme.text,size=20,bold=true,max_width=max_title})
        screen.draw_line(0,39,720,39,{color=theme.accent})
    end
    if status then
        text(status,702-right_width,14,{color=status_color or theme.text_dim,size=12,max_width=right_width})
    end
end

function ui.footer(hints)
    rect(0,684,720,36,{color=theme.bg,filled=true})
    screen.draw_line(18,684,702,684,{color=theme.border})
    local x=18
    for _,hint in ipairs(hints) do
        if not neo then
            x=x+screen.draw_button_hint(hint[1],hint[2],x,692,{color=hint[3],size=12})+14
        else
            local key, label=tostring(hint[1]):upper(),tostring(hint[2]):upper()
            local w=math.max(24,screen.get_text_width(key,11,true)+12)
            local filled = key=="A" or key=="B" or key=="X" or key=="Y"
            local color = (key=="B" or key=="X") and theme.accent or theme.text
            rect(x,691,w,23,{color=color,filled=filled})
            text(key,x+6,696,{color=filled and theme.bg or theme.text,size=11,bold=true})
            text(label,x+w+8,697,{color=theme.text_dim,size=10,max_width=700-x-w-8})
            x=x+w+screen.get_text_width(label,10,false)+24
        end
    end
end

function ui.card(x,y,w,h,opts)
    opts=opts or {}
    if not neo then return screen.draw_card(x,y,w,h,opts) end
    local bg=opts.bg or theme.card_bg
    if bg.r==theme.card_highlight.r and bg.g==theme.card_highlight.g and bg.b==theme.card_highlight.b then
        bg=theme.card_bg -- dense rows retain readable muted text on focus
    end
    rect(x,y,w,h,{color=bg,filled=true})
    local border=opts.border or theme.card_border
    rect(x,y,w,h,{color=border,filled=false})
    if border.r==theme.accent.r and border.g==theme.accent.g and border.b==theme.accent.b then
        rect(x,y,4,h,{color=theme.accent,filled=true})
    end
end

function ui.rect(x,y,w,h,opts)
    if neo and opts then opts.radius=0 end
    return rect(x,y,w,h,opts)
end

function ui.pill(label,x,y,r,g,b,opts)
    if not neo then return screen.draw_pill(label,x,y,r,g,b,opts) end
    opts=opts or {}
    local size=opts.size or 11
    local width=screen.get_text_width(label,size,true)+12
    rect(x,y,width,size+10,{color={r,g,b},filled=true})
    text(label,x+6,y+4,{color=opts.text_color or theme.bg,size=size,bold=true})
    return width
end

-- Fixed 720px components for the public app layouts. Legacy chrome above stays
-- compatible with installed cartridges. Flat fills, bounded layout caches and
-- event-driven redraws avoid a compositor or a continuous animation loop.
local P = {
    X=24, W=672, TOP=112, BOTTOM=650, FOOTER=676,
    C={bg={18,22,24},black={18,22,24},panel={28,33,35},panel2={40,45,47},
       paper={242,237,228},muted={166,174,170},quiet={120,133,127},
       red={238,83,51},green={153,195,119},gold={231,185,88},
       teal={116,190,184},line={63,73,70}}
}
local C=P.C
local selected_muted={63,77,71}
local options, option_count={},0
local function opts(size,color,width,bold)
    if option_count>=256 then options={};option_count=0 end
    local by_color=options[color]
    if not by_color then by_color={};options[color]=by_color end
    local key=tostring(size)..':'..tostring(width or '')..':'..tostring(bold or false)
    local result=by_color[key]
    if not result then
        result={size=size,color=color,max_width=width,bold=bold or false}
        by_color[key]=result;option_count=option_count+1
    end
    return result
end
function P.clean(value)
    local value=tostring(value or ''):sub(1,65536)
    value=value:gsub('[%z\1-\8\11\12\14-\31\127]',' ')
    if not utf8.len(value) then value=value:gsub('[\128-\255]','?') end
    return value
end
function P.rect(x,y,w,h,color)
    if w<=0 or h<=0 then return end
    screen.draw_rect(x,y,w,h,{color=color or C.panel,filled=true})
end
function P.rule(x,y,w,color) P.rect(x,y,w,1,color or C.line) end
function P.text(value,x,y,size,color,width,bold)
    screen.draw_text(P.clean(value),x,y,opts(size or 16,color or C.paper,width or math.max(0,696-x),bold))
end
local fitted,fit_order={},{}
local function fit(value,width,size,display,bold)
    local key=value..'\0'..width..':'..size..':'..tostring(display)..':'..tostring(bold)
    if fitted[key] then return fitted[key] end
    local measure=display and function(s) return screen.get_display_text_width(s,size) end
        or function(s) return screen.get_text_width(s,size,bold or false) end
    local result=value
    if measure(value)>width then
        local chars={}
        for _,cp in utf8.codes(value) do chars[#chars+1]=utf8.char(cp) end
        local low,high=0,#chars
        while low<high do
            local mid=math.floor((low+high+1)/2)
            if measure(table.concat(chars,'',1,mid)..'...')<=width then low=mid else high=mid-1 end
        end
        result=measure('...')<=width and table.concat(chars,'',1,low)..'...' or ''
    end
    fitted[key]=result;fit_order[#fit_order+1]=key
    if #fit_order>128 then fitted[table.remove(fit_order,1)]=nil end
    return result
end
function P.display(value,x,y,size,color,width)
    size=math.max(10,math.min(180,math.floor(size or 42)))
    value=fit(P.clean(value),math.max(0,width or (696-x)),size,true)
    screen.draw_display_text(value,math.floor(x),math.floor(y),size,color or C.paper)
end
function P.header(title,kicker,status,status_color)
    screen.clear(C.bg[1],C.bg[2],C.bg[3])
    P.text(kicker or 'CARTRIDGE / UTILITY',24,12,12,C.muted,480,true)
    -- Native image textures are cached; a missing icon has a geometric fallback.
    local ok=pcall(screen.draw_image,'icon.png',24,36,{w=44,h=44})
    if not ok then
        P.rect(24,36,44,44,C.red);P.rect(33,44,8,28,C.paper);P.rect(48,44,11,28,C.paper)
    end
    P.display(string.upper(title),82,31,42,C.paper,status and 410 or 610)
    if status then P.text(status,504,51,14,status_color or C.muted,192,true) end
    P.rule(24,99,672,C.line);P.rect(24,99,72,3,C.red)
end
function P.footer(hints)
    P.rect(0,676,720,44,C.bg);P.rule(24,676,672,C.line)
    local x=24
    local count=math.min(#hints,5)
    local slots={};local total=0
    for i=1,count do
        local key=P.clean(hints[i][1]):upper()
        local label=P.clean(hints[i][2]):upper()
        key=fit(key,60,12,false,true)
        local kw=math.min(72,math.max(24,screen.get_text_width(key,12,true)+12))
        local lw=math.max(24,math.min(168,screen.get_text_width(label,12,false)))
        slots[i]={key=key,label=label,kw=kw,lw=lw};total=total+kw+lw+24
    end
    local desired_extra=0
    for _,slot in ipairs(slots) do desired_extra=desired_extra+slot.lw-24 end
    local shrink=desired_extra>0 and math.min(1,math.max(0,desired_extra-math.max(0,total-672))/desired_extra) or 1
    for _,slot in ipairs(slots) do
        local lw=24+(slot.lw-24)*shrink
        local key_color=slot.key=='B' and C.red or C.paper
        P.rect(x,688,slot.kw,24,key_color)
        P.text(slot.key,x+6,692,12,C.bg,slot.kw-8,true)
        P.text(slot.label,x+slot.kw+8,692,12,C.muted,lw)
        x=x+slot.kw+lw+24
    end
end
function P.panel(x,y,w,h,color,accent)
    P.rect(x,y,w,h,color or C.panel)
    if accent then P.rect(x,y,4,h,accent) end
end
function P.row(x,y,w,h,selected)
    P.panel(x,y,w,h,selected and C.paper or C.panel,selected and C.red or nil)
    return selected and C.bg or C.paper, selected and selected_muted or C.muted
end
function P.tabs(labels,active,y)
    y=y or 112
    local gap=8;local w=(672-gap*(#labels-1))/#labels
    for i,label in ipairs(labels) do
        local x=24+(i-1)*(w+gap)
        P.rect(x,y,w,36,i==active and C.paper or C.panel)
        P.text(P.clean(label):upper(),x+12,y+9,14,i==active and C.bg or C.muted,w-24,true)
        if i==active then P.rect(x,y+34,w,2,C.red) end
    end
end
function P.metric(x,y,w,h,label,value,unit,accent)
    P.panel(x,y,w,h,C.panel,accent)
    P.text(P.clean(label):upper(),x+14,y+12,12,C.muted,w-28,true)
    local unit_width=unit and unit~='' and math.min(w/3,screen.get_text_width(P.clean(unit),14,false)) or 0
    P.display(value,x+14,y+32,math.min(60,h-38),accent or C.paper,w-28-(unit_width>0 and unit_width+12 or 0))
    if unit_width>0 then P.text(unit,x+w-14-unit_width,y+h-26,14,C.muted,unit_width) end
end
local wrapped,wrap_order={},{}
function P.wrap(value,width,size,max_lines,bold)
    value=P.clean(value);size=size or 16;max_lines=math.max(1,math.min(1024,max_lines or 32))
    local key=value..'\0'..width..':'..size..':'..max_lines..':'..tostring(bold)
    if wrapped[key] then return wrapped[key] end
    local lines={}
    local measure=function(s) return screen.get_text_width(s,size,bold or false) end
    local function push(s)
        if #lines<max_lines then lines[#lines+1]=s end
    end
    for paragraph in (value..'\n'):gmatch('(.-)\n') do
        local current=''
        for word in paragraph:gmatch('%S+') do
            local candidate=current=='' and word or current..' '..word
            if measure(candidate)<=width then current=candidate
            else
                if current~='' then push(current) end
                current=word
                if measure(current)>width then
                    local chunk=''
                    for _,cp in utf8.codes(current) do
                        local ch=utf8.char(cp)
                        if chunk~='' and measure(chunk..ch)>width then push(chunk);chunk=ch else chunk=chunk..ch end
                    end
                    current=chunk
                end
            end
            if #lines>=max_lines then break end
        end
        if #lines<max_lines then push(current) end
        if #lines>=max_lines then break end
    end
    if #lines==0 then lines={''} end
    wrapped[key]=lines;wrap_order[#wrap_order+1]=key
    if #wrap_order>32 then wrapped[table.remove(wrap_order,1)]=nil end
    return lines
end
function P.empty(title,body,y)
    y=y or 230
    P.panel(24,y,672,190,C.panel,C.red)
    P.display(title,44,y+20,36,C.paper,632)
    for i,line in ipairs(P.wrap(body or '',628,16,4)) do P.text(line,44,y+78+(i-1)*25,16,C.muted,628) end
end
function P.sparkline(data,x,y,w,h,color,baseline)
    if #data<2 or w<=0 or h<=0 then return end
    screen.draw_sparkline(data,x,y,w,h,{color=color or C.teal,baseline_color=baseline or C.line})
end
ui.public=P

return ui
