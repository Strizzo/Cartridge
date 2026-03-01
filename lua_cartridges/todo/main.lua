-- Todo for CartridgeOS (Lua)
-- Task manager with priorities and persistence

local PRIORITIES = {"high", "medium", "low"}
local PRIORITY_COLORS = {
    high   = {239, 68, 68},
    medium = {251, 191, 36},
    low    = {100, 220, 140},
}
local PRIORITY_LABELS = {high = "HIGH", medium = "MED", low = "LOW"}

local state = {
    screen = "list",  -- "list" or "add"
    tasks = {},
    cursor = 0,
    scroll = 0,
    -- Add screen
    add_text = "",
    add_priority = 2,  -- index into PRIORITIES (1=high,2=med,3=low)
    add_cursor = 0,    -- 0=text, 1=priority, 2=confirm
    -- Keyboard
    kb_active = false,
    kb_row = 0,
    kb_col = 0,
    stats = {completed = 0, total_ever = 0},
}

local KB_ROWS = {
    {"A","B","C","D","E","F","G","H","I","J"},
    {"K","L","M","N","O","P","Q","R","S","T"},
    {"U","V","W","X","Y","Z","1","2","3","4"},
    {"5","6","7","8","9","0"," ",".","-","<"},
}

-- ── Persistence ──────────────────────────────────────────────────────────────

local function save_tasks()
    storage.save("tasks", state.tasks)
    storage.save("stats", state.stats)
end

local function load_tasks()
    local tasks = storage.load("tasks")
    if tasks then state.tasks = tasks end
    local stats = storage.load("stats")
    if stats then state.stats = stats end
end

-- ── Drawing Helpers ──────────────────────────────────────────────────────────

local function draw_header(title, right_text)
    screen.draw_gradient_rect(0, 0, 720, 40,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 720, 0, {color=theme.accent})
    screen.draw_text(title, 12, 10, {color=theme.text, size=20, bold=true})
    if right_text then
        local rw = screen.get_text_width(right_text, 12, false)
        screen.draw_text(right_text, 704 - rw, 14, {color=theme.text_dim, size=12})
    end
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

-- ── List Screen ──────────────────────────────────────────────────────────────

local function active_tasks()
    local result = {}
    for _, t in ipairs(state.tasks) do
        if not t.done then result[#result + 1] = t end
    end
    return result
end

local function done_count()
    local c = 0
    for _, t in ipairs(state.tasks) do
        if t.done then c = c + 1 end
    end
    return c
end

local function draw_task_list()
    local tasks = active_tasks()
    local n = #tasks
    local dc = done_count()

    draw_header("Todo", n .. " active, " .. dc .. " done")

    local y = 48
    local row_h = 56
    local content_h = 684 - y
    local visible = math.floor(content_h / row_h)

    if n == 0 then
        local msg = "No tasks yet. Press A to add one."
        local tw = screen.get_text_width(msg, 16, false)
        screen.draw_text(msg, (720 - tw) / 2, 340, {color=theme.text_dim, size=16})
    else
        state.cursor = math.max(0, math.min(state.cursor, n - 1))
        local start = math.max(0, math.min(state.scroll, n - visible))
        if state.cursor < start then start = state.cursor end
        if state.cursor >= start + visible then start = state.cursor - visible + 1 end
        state.scroll = math.max(0, start)

        for vi = 0, visible - 1 do
            local idx = start + vi + 1
            if idx > n then break end
            local task = tasks[idx]
            local is_sel = (idx - 1 == state.cursor)
            local cy = y + vi * row_h

            local bg = is_sel and theme.card_highlight or theme.card_bg
            local border = is_sel and theme.accent or theme.card_border
            screen.draw_card(12, cy, 696, row_h - 4, {bg=bg, border=border, radius=6})

            -- Priority color strip
            local pc = PRIORITY_COLORS[task.priority] or PRIORITY_COLORS.medium
            screen.draw_rect(12, cy + 4, 4, row_h - 12, {color=pc, filled=true, radius=2})

            -- Priority pill
            local pl = PRIORITY_LABELS[task.priority] or "MED"
            screen.draw_pill(pl, 24, cy + 6, pc[1], pc[2], pc[3], {text_color={20,20,30}, size=10})

            -- Task text
            local max_w = 580
            local text = task.text
            local tw = screen.get_text_width(text, 14, is_sel)
            if tw > max_w then
                while #text > 1 and screen.get_text_width(text .. "..", 14, is_sel) > max_w do
                    text = text:sub(1, -2)
                end
                text = text .. ".."
            end
            screen.draw_text(text, 24, cy + 26, {color=theme.text, size=14, bold=is_sel})

            -- Created time
            if task.created then
                local age = os.time() - task.created
                local age_str
                if age < 3600 then age_str = math.floor(age/60) .. "m"
                elseif age < 86400 then age_str = math.floor(age/3600) .. "h"
                else age_str = math.floor(age/86400) .. "d"
                end
                local aw = screen.get_text_width(age_str, 11, false)
                screen.draw_text(age_str, 696 - aw, cy + 30, {color=theme.text_dim, size=11})
            end
        end

        -- Scroll indicator
        if n > visible then
            local ind_x = 715
            local bar_top = y + 4
            local bar_h = content_h - 8
            screen.draw_line(ind_x, bar_top, ind_x, bar_top + bar_h, {color=theme.border})
            local thumb_h = math.max(8, math.floor(bar_h * visible / n))
            local progress = n > 1 and (state.cursor / (n - 1)) or 0
            local thumb_y = bar_top + math.floor((bar_h - thumb_h) * progress)
            screen.draw_rect(ind_x - 1, thumb_y, 3, thumb_h, {color=theme.text_dim, filled=true, radius=1})
        end
    end

    draw_footer({
        {"A", "Add", theme.btn_a},
        {"X", "Done", theme.btn_x},
        {"Y", "Delete", theme.btn_y},
        {"\226\134\145\226\134\147", "Navigate", theme.btn_l},
    })
end

-- ── Add Screen with Keyboard ─────────────────────────────────────────────────

local function draw_add_screen()
    draw_header("New Task")

    local y = 52

    -- Text input field
    screen.draw_card(12, y, 696, 50, {bg=theme.card_bg, border=state.add_cursor == 0 and theme.accent or theme.card_border, radius=6})
    screen.draw_text("TASK", 24, y + 6, {color=theme.text_dim, size=11, bold=true})
    local display_text = state.add_text ~= "" and state.add_text or "Type with keyboard below..."
    local text_color = state.add_text ~= "" and theme.text or theme.text_dim
    screen.draw_text(display_text, 24, y + 24, {color=text_color, size=14, max_width=660})
    -- Cursor blink
    if state.add_cursor == 0 then
        local cw = screen.get_text_width(state.add_text, 14, false)
        screen.draw_rect(24 + cw, y + 24, 2, 16, {color=theme.accent, filled=true})
    end
    y = y + 58

    -- Priority selector
    screen.draw_card(12, y, 696, 44, {bg=theme.card_bg, border=state.add_cursor == 1 and theme.accent or theme.card_border, radius=6})
    screen.draw_text("PRIORITY", 24, y + 6, {color=theme.text_dim, size=11, bold=true})
    local px = 24
    for i, p in ipairs(PRIORITIES) do
        local pc = PRIORITY_COLORS[p]
        local is_active = (i == state.add_priority)
        if is_active then
            screen.draw_pill(PRIORITY_LABELS[p], px, y + 22, pc[1], pc[2], pc[3], {text_color={20,20,30}, size=12})
        else
            screen.draw_pill(PRIORITY_LABELS[p], px, y + 22, 40, 40, 60, {text_color=theme.text_dim, size=12})
        end
        px = px + screen.get_text_width(PRIORITY_LABELS[p], 12, true) + 24
    end
    y = y + 52

    -- On-screen keyboard
    local kb_x = 30
    local kb_y = y + 4
    local key_w = 60
    local key_h = 42
    local gap = 6

    for ri, row in ipairs(KB_ROWS) do
        for ci, key in ipairs(row) do
            local kx = kb_x + (ci - 1) * (key_w + gap)
            local ky = kb_y + (ri - 1) * (key_h + gap)
            local is_sel = (state.kb_row == ri - 1 and state.kb_col == ci - 1 and state.add_cursor == 0)

            local bg = is_sel and theme.accent or theme.card_bg
            local tc = is_sel and {20, 20, 30} or theme.text

            screen.draw_rect(kx, ky, key_w, key_h, {color=bg, filled=true, radius=4})
            if not is_sel then
                screen.draw_rect(kx, ky, key_w, key_h, {color=theme.card_border, filled=false, radius=4})
            end

            local label = key
            if key == " " then label = "SPC"
            elseif key == "<" then label = "DEL"
            end
            local lw = screen.get_text_width(label, 14, is_sel)
            screen.draw_text(label, kx + (key_w - lw) / 2, ky + 12, {color=tc, size=14, bold=is_sel})
        end
    end

    -- Confirm button
    y = kb_y + #KB_ROWS * (key_h + gap) + 8
    local confirm_sel = (state.add_cursor == 2)
    local cbg = confirm_sel and theme.accent or theme.card_bg
    local ctc = confirm_sel and {20, 20, 30} or theme.text
    screen.draw_rect(240, y, 240, 44, {color=cbg, filled=true, radius=6})
    if not confirm_sel then
        screen.draw_rect(240, y, 240, 44, {color=theme.card_border, filled=false, radius=6})
    end
    local cl = "ADD TASK"
    local clw = screen.get_text_width(cl, 16, true)
    screen.draw_text(cl, 240 + (240 - clw) / 2, y + 12, {color=ctc, size=16, bold=true})

    draw_footer({
        {"A", "Select", theme.btn_a},
        {"B", "Cancel", theme.btn_b},
        {"L1/R1", "Section", theme.btn_l},
    })
end

-- ── Input ────────────────────────────────────────────────────────────────────

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    if state.screen == "list" then
        local tasks = active_tasks()
        local n = #tasks

        if button == "a" then
            state.screen = "add"
            state.add_text = ""
            state.add_priority = 2
            state.add_cursor = 0
            state.kb_row = 0
            state.kb_col = 0
        elseif button == "x" and n > 0 then
            -- Mark as done
            local task = tasks[state.cursor + 1]
            if task then
                task.done = true
                state.stats.completed = state.stats.completed + 1
                save_tasks()
                if state.cursor >= n - 1 and state.cursor > 0 then
                    state.cursor = state.cursor - 1
                end
            end
        elseif button == "y" and n > 0 then
            -- Delete
            local task = tasks[state.cursor + 1]
            if task then
                for i, t in ipairs(state.tasks) do
                    if t == task then
                        table.remove(state.tasks, i)
                        break
                    end
                end
                save_tasks()
                if state.cursor >= #active_tasks() and state.cursor > 0 then
                    state.cursor = state.cursor - 1
                end
            end
        elseif button == "dpad_up" then
            state.cursor = math.max(0, state.cursor - 1)
        elseif button == "dpad_down" then
            state.cursor = math.min(math.max(0, n - 1), state.cursor + 1)
        end

    elseif state.screen == "add" then
        if button == "b" then
            state.screen = "list"
            return
        end

        if button == "l1" then
            state.add_cursor = math.max(0, state.add_cursor - 1)
            return
        elseif button == "r1" then
            state.add_cursor = math.min(2, state.add_cursor + 1)
            return
        end

        if state.add_cursor == 0 then
            -- Keyboard navigation
            if button == "dpad_up" then
                state.kb_row = math.max(0, state.kb_row - 1)
            elseif button == "dpad_down" then
                state.kb_row = math.min(#KB_ROWS - 1, state.kb_row + 1)
            elseif button == "dpad_left" then
                state.kb_col = math.max(0, state.kb_col - 1)
            elseif button == "dpad_right" then
                state.kb_col = math.min(#KB_ROWS[state.kb_row + 1] - 1, state.kb_col + 1)
            elseif button == "a" then
                local key = KB_ROWS[state.kb_row + 1][state.kb_col + 1]
                if key == "<" then
                    -- Backspace
                    if #state.add_text > 0 then
                        state.add_text = state.add_text:sub(1, -2)
                    end
                else
                    if #state.add_text < 60 then
                        state.add_text = state.add_text .. key
                    end
                end
            end
        elseif state.add_cursor == 1 then
            -- Priority
            if button == "dpad_left" or button == "a" then
                state.add_priority = state.add_priority > 1 and (state.add_priority - 1) or #PRIORITIES
            elseif button == "dpad_right" then
                state.add_priority = state.add_priority < #PRIORITIES and (state.add_priority + 1) or 1
            end
        elseif state.add_cursor == 2 then
            -- Confirm
            if button == "a" and state.add_text ~= "" then
                state.tasks[#state.tasks + 1] = {
                    text = state.add_text,
                    priority = PRIORITIES[state.add_priority],
                    done = false,
                    created = os.time(),
                }
                state.stats.total_ever = state.stats.total_ever + 1
                save_tasks()
                state.screen = "list"
                state.cursor = #active_tasks() - 1
            end
        end
    end
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    load_tasks()
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    if state.screen == "list" then
        draw_task_list()
    elseif state.screen == "add" then
        draw_add_screen()
    end
end
