-- Calculator App for Cartridge OS (Lua)
-- Safe expression evaluator with history

local U = ui.public
local C = U.C

-- ── Expression Evaluator (Recursive Descent Parser) ──────────────────────────

local FUNCTIONS = {
    sin = math.sin, cos = math.cos, tan = math.tan,
    sqrt = math.sqrt, ln = function(x) return math.log(x) end,
}
local CONSTANTS = {
    pi = math.pi, e = math.exp(1),
}

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
        elseif ch:match("%a") then
            local start = i
            while i <= #text and text:sub(i, i):match("%a") do
                i = i + 1
            end
            local word = text:sub(start, i - 1)
            if FUNCTIONS[word] then
                tokens[#tokens + 1] = {kind="FUNC", value=word}
            elseif CONSTANTS[word] then
                tokens[#tokens + 1] = {kind="NUMBER", value=CONSTANTS[word]}
            else
                return nil, "Unknown: " .. word
            end
        elseif ch == "+" then tokens[#tokens + 1] = {kind="PLUS"}; i = i + 1
        elseif ch == "-" then tokens[#tokens + 1] = {kind="MINUS"}; i = i + 1
        elseif ch == "*" then tokens[#tokens + 1] = {kind="MUL"}; i = i + 1
        elseif ch == "/" then tokens[#tokens + 1] = {kind="DIV"}; i = i + 1
        elseif ch == "^" then tokens[#tokens + 1] = {kind="POWER"}; i = i + 1
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

    local expr, term, power, unary, postfix, primary

    primary = function()
        local tok = current()
        if tok.kind == "FUNC" then
            local fn = FUNCTIONS[tok.value]
            pos = pos + 1
            local _, err = consume("LPAREN")
            if err then return nil end
            local val = expr()
            if val == nil then return nil end
            _, err = consume("RPAREN")
            if err then return nil end
            return fn(val)
        elseif tok.kind == "NUMBER" then
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

    -- Right-associative: 2^3^2 = 2^(3^2) = 512
    power = function()
        local base = unary()
        if base == nil then return nil end
        if match("POWER") then
            local exp_val = power()
            if exp_val == nil then return nil end
            return base ^ exp_val
        end
        return base
    end

    term = function()
        local left = power()
        if left == nil then return nil end
        while true do
            local tok = match("MUL", "DIV")
            if not tok then break end
            local right = power()
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
     {label="^", action="^", type=TYPE_OP}},
    {{label="1", action="1", type=TYPE_DIGIT},
     {label="2", action="2", type=TYPE_DIGIT},
     {label="3", action="3", type=TYPE_DIGIT},
     {label="\226\136\146", action="-", type=TYPE_OP},
     {label="\226\136\154", action="sqrt(", type=TYPE_FUNC}},
    {{label="0", action="0", type=TYPE_DIGIT},
     {label=".", action=".", type=TYPE_DIGIT},
     {label="+/\226\136\146", action="negate", type=TYPE_FUNC},
     {label="+", action="+", type=TYPE_OP},
     {label="=", action="equals", type=TYPE_EQUAL}},
    {{label="sin", action="sin(", type=TYPE_FUNC},
     {label="cos", action="cos(", type=TYPE_FUNC},
     {label="tan", action="tan(", type=TYPE_FUNC},
     {label="ln", action="ln(", type=TYPE_FUNC},
     {label="\207\128", action="pi", type=TYPE_FUNC}},
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
    history_visible = 6,
}

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
    local ops = {["+"] = true, ["-"] = true, ["*"] = true, ["/"] = true, ["("] = true, ["^"] = true}
    local i = #state.expression
    while i >= 1 and ops[state.expression:sub(i, i)] do
        i = i - 1
    end
    while i >= 1 and not ops[state.expression:sub(i, i)] do
        i = i - 1
    end
    -- Also consume a preceding function name (e.g. "sin(")
    if i >= 1 and state.expression:sub(i, i) == "(" then
        local before = i - 1
        while before >= 1 and state.expression:sub(before, before):match("%a") do
            before = before - 1
        end
        if before < i - 1 then i = before end
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
    if i >= 1 and ({["+"] = true, ["-"] = true, ["*"] = true, ["/"] = true, ["%"] = true, ["("] = true, ["^"] = true})[expr:sub(i, i)] then
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

-- ── Instrument and History ──────────────────────────────────────────────────

-- Expressions are ASCII internally. Keep the editable end visible without
-- shrinking the body text or changing the stored expression.
local function expression_tail(expression, width)
    local value = expression == "" and "Enter an expression" or expression
    if screen.get_text_width(value, 18, false) <= width then return value end
    while #value > 1 and screen.get_text_width("..." .. value, 18, false) > width do
        value = value:sub(2)
    end
    return "..." .. value
end

local function draw_calc_screen()
    U.header("Calculator", "CALCULATION / INSTRUMENT", "LOCAL HISTORY", C.muted)
    U.panel(24, 112, 672, 150, C.paper, C.red)
    U.text("EXPRESSION", 44, 126, 14, C.black, 136, true)
    U.text(expression_tail(state.expression, 476), 200, 126, 18, C.black, 476)
    U.rule(44, 157, 632, C.quiet)

    if state.error_text ~= "" then
        U.display("CHECK INPUT", 44, 172, 40, C.red, 632)
        U.text(state.error_text .. ". B edits; AC clears.", 44, 226, 16, C.black, 632)
    else
        local value = state.result_text ~= "" and state.result_text or "0"
        U.display(value, 44, 166, 76, C.black, 632)
        local note = state.expression == "" and "D-pad chooses a key. A enters it."
            or (state.result_text == "" and "Finish the expression, then choose =."
            or "RESULT / choose = to save to history")
        U.text(note, 44, 238, 14, C.black, 632)
    end

    U.text("KEYPAD", 24, 274, 14, C.muted, 120, true)
    U.text("C entry / AC all / functions in radians", 184, 274, 14, C.muted, 512)
    for row = 1, GRID_ROWS do
        for col = 1, GRID_COLS do
            local btn = GRID[row][col]
            local x = 24 + (col - 1) * 136
            local y = 300 + (row - 1) * 58
            local selected = row == state.cursor_row and col == state.cursor_col
            local bg, fg = C.panel, C.paper
            if btn.type == TYPE_DIGIT then bg = C.panel2
            elseif btn.type == TYPE_EQUAL then bg, fg = C.red, C.black
            elseif btn.type == TYPE_CLEAR or btn.type == TYPE_DEL then fg = C.red
            elseif btn.type == TYPE_OP then fg = C.gold
            else fg = C.muted end
            if selected then bg, fg = C.paper, C.black end
            U.rect(x, y, 128, 52, bg)
            if selected then U.rect(x, y, 4, 52, C.red) end
            local label = btn.label
            if btn.action == "sqrt(" then label = "SQRT"
            elseif btn.action == "pi" then label = "PI"
            elseif btn.action == "negate" then label = "+/-" end
            local size = (btn.type == TYPE_DIGIT or btn.type == TYPE_OP or btn.type == TYPE_EQUAL)
                and 26 or 18
            -- Keep math symbols in the body font; the display face is for readouts.
            local width = screen.get_text_width(label, size, true)
            U.text(label, x + (128 - width) / 2, y + (52 - screen.get_line_height(size, true)) / 2,
                size, fg, 108, true)
        end
    end
    U.footer({{"A", "Enter"}, {"B", "Delete"}, {"Y", "History"}, {"D-PAD", "Key"}})
end

local function draw_history_screen()
    U.header("Calculator", "CALCULATION / ARCHIVE", #state.history .. " SAVED", C.muted)
    U.tabs({"Instrument", "History"}, 2)
    local history = state.history
    if #history == 0 then
        U.empty("No calculations saved", "B returns to the keypad. Enter an expression and choose = to save it.")
        U.footer({{"B", "Back"}})
        return
    end
    state.history_selected = math.max(1, math.min(state.history_selected, #history))
    if state.history_selected < state.history_scroll + 1 then
        state.history_scroll = state.history_selected - 1
    elseif state.history_selected > state.history_scroll + state.history_visible then
        state.history_scroll = state.history_selected - state.history_visible
    end
    state.history_scroll = math.max(0, math.min(state.history_scroll, #history - state.history_visible))
    U.text("EXPRESSION / RESULT", 24, 158, 14, C.muted, 440, true)
    U.text(state.history_selected .. " / " .. #history, 600, 158, 14, C.muted, 96, true)
    for i = state.history_scroll + 1, math.min(#history, state.history_scroll + state.history_visible) do
        local entry = history[i]
        local y = 182 + (i - state.history_scroll - 1) * 78
        local fg, secondary = U.row(24, y, 672, 72, i == state.history_selected)
        U.text(string.format("%02d", #history - i + 1), 40, y + 12, 14, secondary, 38, true)
        U.text(expression_tail(entry.expr, 592), 88, y + 9, 18, secondary, 592)
        U.display("= " .. entry.result, 88, y + 32, 28, fg, 592)
    end
    U.footer({{"A", "Recall"}, {"B", "Back"}, {"X", "Clear all"}, {"D-PAD", "Scroll"}})
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    -- Load history
    local data = storage.load("history")
    if data and data.entries then
        state.history = data.entries
    end
end

local function handle_input(button, action)
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
            state.history_selected = math.max(1, math.min(#state.history, state.history_selected + 1))
        end
    end
end

local function input_snapshot()
    return table.concat({#state.screen_stack, state.cursor_row, state.cursor_col,
        state.expression, state.result_text, state.error_text, #state.history,
        state.history_selected, state.history_scroll}, "\0")
end

function on_input(button, action)
    local before = input_snapshot()
    handle_input(button, action)
    if before ~= input_snapshot() then app.request_redraw() end
end

function on_update(dt)
    return false
end

function on_render()
    local current = state.screen_stack[#state.screen_stack]
    if current == "calc" then
        draw_calc_screen()
    elseif current == "history" then
        draw_history_screen()
    end
end
