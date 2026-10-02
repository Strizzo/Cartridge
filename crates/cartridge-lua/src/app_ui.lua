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

return ui
