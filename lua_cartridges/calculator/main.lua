-- Calculator App for Cartridge OS (Lua)
-- Safe expression evaluator with history

-- ── Expression Evaluator (Recursive Descent Parser) ──────────────────────────

local function tokenize(text)
    local tokens = {}
    local i = 1
    while i <= #text do
        local ch = text:sub(i, i)
        if ch == " " or ch == "\t" then
            i = i + 1
        elseif ch:match("%d") or ch == "." then
            local start = i
            local has_dot = false
            while i <= #text do
                local c = text:sub(i, i)
                if c == "." then
                    if has_dot then break end
                    has_dot = true
                    i = i + 1
                elseif c:match("%d") then
                    i = i + 1
                else
                    break
                end
            end
            local num_str = text:sub(start, i - 1)
            tokens[#tokens + 1] = {kind="NUMBER", value=tonumber(num_str) or 0}
        elseif ch == "+" then tokens[#tokens + 1] = {kind="PLUS"}; i = i + 1
        elseif ch == "-" then tokens[#tokens + 1] = {kind="MINUS"}; i = i + 1
        elseif ch == "*" then tokens[#tokens + 1] = {kind="MUL"}; i = i + 1
        elseif ch == "/" then tokens[#tokens + 1] = {kind="DIV"}; i = i + 1
        elseif ch == "%" then tokens[#tokens + 1] = {kind="PERCENT"}; i = i + 1
        elseif ch == "(" then tokens[#tokens + 1] = {kind="LPAREN"}; i = i + 1
        elseif ch == ")" then tokens[#tokens + 1] = {kind="RPAREN"}; i = i + 1
        else
            return nil, "Unexpected character: " .. ch
        end
    end
    tokens[#tokens + 1] = {kind="EOF"}
    return tokens
end

local function create_parser(tokens)
    local pos = 1
    local parser = {}

    local function current()
        return tokens[pos] or {kind="EOF"}
    end

    local function consume(kind)
        local tok = current()
        if tok.kind ~= kind then
            return nil, "Expected " .. kind .. ", got " .. tok.kind
        end
        pos = pos + 1
        return tok
    end

    local function match(...)
        local tok = current()
        for _, kind in ipairs({...}) do
            if tok.kind == kind then
                pos = pos + 1
                return tok
            end
        end
        return nil
    end

    local expr, term, unary, postfix, primary

    primary = function()
        local tok = current()
        if tok.kind == "NUMBER" then
            pos = pos + 1
            return tok.value
        elseif tok.kind == "LPAREN" then
            pos = pos + 1
            local val = expr()
            if val == nil then return nil end
            local _, err = consume("RPAREN")
            if err then return nil end
            return val
        end
        return nil
    end

    postfix = function()
        local val = primary()
        if val == nil then return nil end
        while match("PERCENT") do
            val = val / 100.0
        end
        return val
    end

    unary = function()
        if match("MINUS") then
            local val = unary()
            if val == nil then return nil end
            return -val
        end
        return postfix()
    end

    term = function()
        local left = unary()
        if left == nil then return nil end
        while true do
            local tok = match("MUL", "DIV")
            if not tok then break end
            local right = unary()
            if right == nil then return nil end
            if tok.kind == "MUL" then
                left = left * right
            else
                if right == 0 then return nil end -- division by zero
                left = left / right
            end
        end
        return left
    end

    expr = function()
        local left = term()
        if left == nil then return nil end
        while true do
            local tok = match("PLUS", "MINUS")
            if not tok then break end
            local right = term()
            if right == nil then return nil end
            if tok.kind == "PLUS" then
                left = left + right
            else
                left = left - right
            end
        end
        return left
    end

    parser.parse = function()
        local result = expr()
        if result == nil then return nil end
        if current().kind ~= "EOF" then return nil end
        return result
    end

    return parser
end

local function evaluate(expression)
    if not expression or expression:match("^%s*$") then return nil end
    local tokens, err = tokenize(expression)
    if not tokens then return nil end
    local parser = create_parser(tokens)
    return parser.parse()
end

local function format_result(value)
    if value == nil then return nil end
    if value ~= value then return "NaN" end  -- NaN check
    if value == math.huge then return "Infinity" end
    if value == -math.huge then return "-Infinity" end

    -- Check if integer
    if value == math.floor(value) and math.abs(value) < 1e15 then
        return tostring(math.floor(value))
    end
    -- Very large or very small
    if math.abs(value) >= 1e15 or (math.abs(value) < 1e-6 and value ~= 0) then
        return string.format("%.6g", value)
    end
    return string.format("%.10g", value)
end

local function try_evaluate(expression)
    local result = evaluate(expression)
    if result then return format_result(result) end
    return nil
end

-- ── Button Grid Definition ───────────────────────────────────────────────────

local TYPE_DIGIT = "digit"
local TYPE_OP = "op"
local TYPE_EQUAL = "equal"
local TYPE_CLEAR = "clear"
local TYPE_FUNC = "func"
local TYPE_DEL = "del"
local TYPE_EMPTY = "empty"

local GRID = {
    {{label="C", action="clear", type=TYPE_CLEAR},
     {label="AC", action="allclear", type=TYPE_CLEAR},
     {label="%", action="%", type=TYPE_FUNC},
     {label="(", action="(", type=TYPE_FUNC},
     {label=")", action=")", type=TYPE_FUNC}},
    {{label="7", action="7", type=TYPE_DIGIT},
     {label="8", action="8", type=TYPE_DIGIT},
     {label="9", action="9", type=TYPE_DIGIT},
     {label="\195\183", action="/", type=TYPE_OP},
     {label="DEL", action="del", type=TYPE_DEL}},
    {{label="4", action="4", type=TYPE_DIGIT},
     {label="5", action="5", type=TYPE_DIGIT},
     {label="6", action="6", type=TYPE_DIGIT},
     {label="\195\151", action="*", type=TYPE_OP},
     {label="", action="", type=TYPE_EMPTY}},
    {{label="1", action="1", type=TYPE_DIGIT},
     {label="2", action="2", type=TYPE_DIGIT},
     {label="3", action="3", type=TYPE_DIGIT},
     {label="\226\136\146", action="-", type=TYPE_OP},
     {label="", action="", type=TYPE_EMPTY}},
    {{label="0", action="0", type=TYPE_DIGIT},
     {label=".", action=".", type=TYPE_DIGIT},
     {label="+/\226\136\146", action="negate", type=TYPE_FUNC},
     {label="+", action="+", type=TYPE_OP},
     {label="=", action="equals", type=TYPE_EQUAL}},
}

local GRID_ROWS = #GRID
local GRID_COLS = #GRID[1]
local MAX_HISTORY = 20

-- ── State ────────────────────────────────────────────────────────────────────

local state = {
    screen_stack = {"calc"},
    -- Calc screen
    expression = "",
    result_text = "",
    error_text = "",
    last_result = "",
    cursor_row = 5,
    cursor_col = 1,
    -- History
    history = {},
    history_selected = 1,
    history_scroll = 0,
    history_visible = 9,
}

-- ── Drawing Helpers ──────────────────────────────────────────────────────────

local function draw_header(title)
    screen.draw_gradient_rect(0, 0, 720, 40,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 720, 0, {color=theme.accent})
    screen.draw_text(title, 12, 10, {color=theme.text, size=20, bold=true})
end

local function draw_footer(hints)
    screen.draw_rect(0, 684, 720, 36, {color=theme.bg_header, filled=true})
    screen.draw_line(0, 684, 720, 684, {color=theme.border})
    local x = 10
    for _, h in ipairs(hints) do
        local w = screen.draw_button_hint(h[1], h[2], x, 692, {color=h[3], size=12})
        x = x + w + 14
    end
end

local function get_button_colors(btn_type, is_selected)
    if is_selected then
        if btn_type == TYPE_EQUAL then
            return {60,170,60}, {255,255,255}, theme.accent
        elseif btn_type == TYPE_OP then
            return {50,85,160}, {255,255,255}, theme.accent
        elseif btn_type == TYPE_CLEAR then
            return {160,50,50}, {255,255,255}, theme.accent
        elseif btn_type == TYPE_DEL then
            return {140,80,40}, {255,255,255}, theme.accent
        elseif btn_type == TYPE_FUNC then
            return {55,65,95}, {255,255,255}, theme.accent
        else
            return theme.card_highlight, {255,255,255}, theme.accent
        end
    end
    if btn_type == TYPE_EQUAL then
        return {45,140,45}, {255,255,255}, {55,160,55}
    elseif btn_type == TYPE_OP then
        return {40,65,130}, {180,210,255}, {50,80,155}
    elseif btn_type == TYPE_CLEAR then
        return {130,40,40}, {255,180,180}, {155,50,50}
    elseif btn_type == TYPE_DEL then
        return {110,65,30}, {255,200,150}, {135,80,40}
    elseif btn_type == TYPE_FUNC then
        return {38,48,72}, {160,190,230}, {50,62,90}
    else
        return theme.card_bg, theme.text, theme.border
    end
end

-- ── Calc Logic ───────────────────────────────────────────────────────────────

local function update_preview()
    if state.expression == "" then
        state.result_text = ""
        state.error_text = ""
        return
    end
    local preview = try_evaluate(state.expression)
    if preview then
        state.result_text = preview
        state.error_text = ""
    else
        state.result_text = ""
    end
end

local function do_evaluate()
    if state.expression == "" then return end
    local result = evaluate(state.expression)
    if result then
        local result_str = format_result(result)
        state.result_text = result_str
        state.error_text = ""
        -- Add to history
        if #state.history == 0 or state.history[1].expr ~= state.expression or state.history[1].result ~= result_str then
            table.insert(state.history, 1, {expr=state.expression, result=result_str})
            if #state.history > MAX_HISTORY then
                state.history[#state.history] = nil
            end
            storage.save("history", {entries=state.history})
        end
        state.last_result = result_str
        state.expression = result_str
    else
        -- Check division by zero
        if state.expression:match("/0") then
            state.error_text = "Cannot divide by zero"
        else
            state.error_text = "Invalid expression"
        end
        state.result_text = ""
    end
end

local function clear_last_entry()
    if state.expression == "" then return end
    local ops = {["+"] = true, ["-"] = true, ["*"] = true, ["/"] = true, ["("] = true}
    local i = #state.expression
    while i >= 1 and ops[state.expression:sub(i, i)] do
        i = i - 1
    end
    while i >= 1 and not ops[state.expression:sub(i, i)] do
        i = i - 1
    end
    if i >= 1 then
        state.expression = state.expression:sub(1, i)
    else
        state.expression = ""
    end
end

local function toggle_negate()
    local expr = state.expression
    if expr == "" then
        state.expression = "-"
        return
    end
    local i = #expr
    while i >= 1 and (expr:sub(i, i):match("%d") or expr:sub(i, i) == ".") do
        i = i - 1
    end
    if i >= 1 and expr:sub(i, i) == "-" then
        if i == 1 or (i > 1 and ({["+"] = true, ["-"] = true, ["*"] = true, ["/"] = true, ["("] = true})[expr:sub(i - 1, i - 1)]) then
            state.expression = expr:sub(1, i - 1) .. expr:sub(i + 1)
            return
        end
    end
    if i >= 1 and ({["+"] = true, ["-"] = true, ["*"] = true, ["/"] = true, ["%"] = true, ["("] = true})[expr:sub(i, i)] then
        state.expression = expr:sub(1, i) .. "-" .. expr:sub(i + 1)
    elseif i < 1 then
        if expr:sub(1, 1) == "-" then
            state.expression = expr:sub(2)
        else
            state.expression = "-" .. expr
        end
    else
        state.expression = state.expression .. "-"
    end
end

local function press_button()
    local btn = GRID[state.cursor_row][state.cursor_col]
    if btn.type == TYPE_EMPTY then return end

    state.error_text = ""
    local action = btn.action

    if action == "clear" then
        clear_last_entry()
    elseif action == "allclear" then
        state.expression = ""
        state.result_text = ""
        state.error_text = ""
    elseif action == "del" then
        if #state.expression > 0 then
            state.expression = state.expression:sub(1, -2)
        end
    elseif action == "negate" then
        toggle_negate()
    elseif action == "equals" then
        do_evaluate()
        return
    else
        state.expression = state.expression .. action
    end

    update_preview()
end

local function move_cursor(drow, dcol)
    local new_row = state.cursor_row + drow
    local new_col = state.cursor_col + dcol

    -- Wrap
    if new_row < 1 then new_row = GRID_ROWS
    elseif new_row > GRID_ROWS then new_row = 1 end
    if new_col < 1 then new_col = GRID_COLS
    elseif new_col > GRID_COLS then new_col = 1 end

    -- Skip empty buttons
    local btn = GRID[new_row][new_col]
    if btn.type == TYPE_EMPTY then
        state.cursor_row = new_row
        state.cursor_col = new_col
        if drow ~= 0 then move_cursor(drow, 0) return end
        if dcol ~= 0 then move_cursor(0, dcol) return end
        return
    end
    state.cursor_row = new_row
    state.cursor_col = new_col
end

-- ── Calc Screen Drawing ─────────────────────────────────────────────────────

local function draw_calc_screen()
    draw_header("Calculator")

    -- Display area
    local display_y = 44
    local display_h = 116
    screen.draw_card(12, display_y, 696, display_h, {bg=theme.card_bg, border=theme.border, radius=10, shadow=true})

    -- Format expression
    local display_expr = state.expression
    display_expr = display_expr:gsub("/", " \195\183 ")
    display_expr = display_expr:gsub("%*", " \195\151 ")
    -- Add spaces around + and -
    local formatted = ""
    local raw = state.expression
    local i = 1
    while i <= #raw do
        local ch = raw:sub(i, i)
        if (ch == "+" or ch == "-") and i > 1 and not ({["+"] = true, ["-"] = true, ["*"] = true, ["/"] = true, ["("] = true})[raw:sub(i - 1, i - 1)] then
            formatted = formatted .. " " .. ch .. " "
        else
            formatted = formatted .. ch
        end
        i = i + 1
    end
    display_expr = formatted:gsub("/", " \195\183 "):gsub("%*", " \195\151 ")

    if display_expr == "" then
        display_expr = "0"
    end

    -- Draw expression (right-aligned)
    local expr_color = state.expression == "" and theme.text_dim or theme.text
    local max_expr_w = 672
    local tw = screen.get_text_width(display_expr, 28, true)
    if tw > max_expr_w then
        -- Show rightmost portion
        local expr_x = 24
        screen.draw_text(display_expr, expr_x, display_y + 16, {color=expr_color, size=28, bold=true, max_width=max_expr_w})
    else
        local expr_x = 12 + 696 - 12 - tw
        screen.draw_text(display_expr, expr_x, display_y + 16, {color=expr_color, size=28, bold=true})
    end

    -- Divider
    screen.draw_line(24, display_y + 55, 698, display_y + 55, {color={50, 50, 70}})

    -- Result preview or error
    local preview_y = display_y + 60
    if state.error_text ~= "" then
        local ew = screen.get_text_width(state.error_text, 20, false)
        screen.draw_text(state.error_text, 698 - ew, preview_y, {color=theme.negative, size=20})
    elseif state.result_text ~= "" and state.expression ~= "" then
        local preview_str = "= " .. state.result_text
        local pw = screen.get_text_width(preview_str, 20, false)
        screen.draw_text(preview_str, 698 - pw, preview_y, {color=theme.text_dim, size=20})
    end

    -- Button grid
    local grid_y_start = 168
    local grid_x_start = 10
    local btn_w = 136
    local btn_h = 52
    local gap_x = 5
    local gap_y = 5

    for row = 1, GRID_ROWS do
        for col = 1, GRID_COLS do
            local btn = GRID[row][col]
            if btn.type ~= TYPE_EMPTY then
                local x = grid_x_start + (col - 1) * (btn_w + gap_x)
                local y = grid_y_start + (row - 1) * (btn_h + gap_y)
                local is_selected = (row == state.cursor_row and col == state.cursor_col)

                local bg, text_color, border_color = get_button_colors(btn.type, is_selected)

                -- Selection glow
                if is_selected then
                    screen.draw_rect(x - 2, y - 2, btn_w + 4, btn_h + 4, {color=theme.accent, filled=true, radius=10})
                end

                screen.draw_rect(x, y, btn_w, btn_h, {color=bg, filled=true, radius=8})
                if border_color then
                    screen.draw_rect(x, y, btn_w, btn_h, {color=border_color, filled=false, radius=8})
                end

                -- Label
                local font_size = 20
                if btn.label == "DEL" or btn.label == "AC" or btn.label == "+/\226\136\146" then
                    font_size = 16
                end
                local lw = screen.get_text_width(btn.label, font_size, true)
                local lh = screen.get_line_height(font_size, true)
                screen.draw_text(btn.label, x + (btn_w - lw) / 2, y + (btn_h - lh) / 2, {color=text_color, size=font_size, bold=true})
            end
        end
    end

    draw_footer({
        {"A", "Press", theme.btn_a},
        {"B", "Delete", theme.btn_b},
        {"Y", "History", theme.btn_y},
    })
end

-- ── History Screen Drawing ───────────────────────────────────────────────────

local function draw_history_screen()
    draw_header("History")

    local history = state.history
    if #history == 0 then
        local tw = screen.get_text_width("No calculations yet", 18, false)
        screen.draw_text("No calculations yet", (720 - tw) / 2, 200, {color=theme.text_dim, size=18})
        local hw = screen.get_text_width("Press B to go back", 14, false)
        screen.draw_text("Press B to go back", (720 - hw) / 2, 236, {color=theme.text_dim, size=14})
        draw_footer({{"B", "Back", theme.btn_b}})
        return
    end

    -- Ensure selected is in range
    state.history_selected = math.max(1, math.min(state.history_selected, #history))

    -- Ensure visible
    if state.history_selected < state.history_scroll + 1 then
        state.history_scroll = state.history_selected - 1
    elseif state.history_selected > state.history_scroll + state.history_visible then
        state.history_scroll = state.history_selected - state.history_visible
    end

    local content_y = 50
    local item_h = 62
    local item_gap = 6
    local x_pad = 12
    local card_w = 696

    for i = state.history_scroll + 1, math.min(#history, state.history_scroll + state.history_visible + 1) do
        local entry = history[i]
        local local_idx = i - state.history_scroll - 1
        local y = content_y + local_idx * (item_h + item_gap)

        if y + item_h > 684 then break end

        local is_selected = (i == state.history_selected)

        if is_selected then
            screen.draw_card(x_pad, y, card_w, item_h, {bg=theme.card_highlight, border=theme.accent, radius=8, shadow=true})
        else
            screen.draw_card(x_pad, y, card_w, item_h, {bg=theme.card_bg, border=theme.border, radius=8})
        end

        -- Expression
        local expr_display = entry.expr:gsub("/", " \195\183 "):gsub("%*", " \195\151 ")
        screen.draw_text(expr_display, x_pad + 14, y + 10, {color=theme.text, size=16, max_width=400})

        -- Result (right-aligned)
        local result_str = "= " .. entry.result
        local rw = screen.get_text_width(result_str, 16, true)
        screen.draw_text(result_str, x_pad + card_w - 14 - rw, y + 10, {color=theme.text_accent, size=16, bold=true})

        -- Index number
        local idx_str = "#" .. (#history - i + 1)
        screen.draw_text(idx_str, x_pad + 14, y + 38, {color=theme.text_dim, size=11})
    end

    -- Scroll indicators
    if state.history_scroll > 0 then
        local tw = screen.get_text_width("\226\150\178 more", 14, false)
        screen.draw_text("\226\150\178 more", (720 - tw) / 2, 42, {color=theme.text_dim, size=14})
    end
    if state.history_scroll + state.history_visible < #history then
        local tw = screen.get_text_width("\226\150\188 more", 14, false)
        screen.draw_text("\226\150\188 more", (720 - tw) / 2, 670, {color=theme.text_dim, size=14})
    end

    draw_footer({
        {"A", "Recall", theme.btn_a},
        {"B", "Back", theme.btn_b},
        {"X", "Clear All", theme.btn_x},
    })
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    -- Load history
    local data = storage.load("history")
    if data and data.entries then
        state.history = data.entries
    end
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local current = state.screen_stack[#state.screen_stack]

    if current == "calc" then
        if button == "dpad_up" then
            move_cursor(-1, 0)
        elseif button == "dpad_down" then
            move_cursor(1, 0)
        elseif button == "dpad_left" then
            move_cursor(0, -1)
        elseif button == "dpad_right" then
            move_cursor(0, 1)
        elseif button == "a" then
            press_button()
        elseif button == "b" then
            if #state.expression > 0 then
                state.expression = state.expression:sub(1, -2)
                state.error_text = ""
                update_preview()
            end
        elseif button == "y" then
            state.screen_stack[#state.screen_stack + 1] = "history"
            state.history_selected = math.max(1, math.min(state.history_selected, #state.history))
        end

    elseif current == "history" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
        elseif button == "a" and #state.history > 0 then
            local entry = state.history[state.history_selected]
            if entry then
                state.expression = entry.expr
                state.error_text = ""
                update_preview()
                state.screen_stack[#state.screen_stack] = nil
            end
        elseif button == "x" then
            state.history = {}
            storage.save("history", {entries={}})
            state.history_selected = 1
            state.history_scroll = 0
        elseif button == "dpad_up" then
            state.history_selected = math.max(1, state.history_selected - 1)
        elseif button == "dpad_down" then
            state.history_selected = math.min(#state.history, state.history_selected + 1)
        end
    end
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    local current = state.screen_stack[#state.screen_stack]
    if current == "calc" then
        draw_calc_screen()
    elseif current == "history" then
        draw_history_screen()
    end
end
