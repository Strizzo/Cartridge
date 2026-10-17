-- Todo for CartridgeOS (Lua)
-- Task register with priorities and compatible local persistence.

local U = ui.public
local C = U.C
local PRIORITIES = {"high", "medium", "low"}
local PRIORITY_COLORS = {high = C.red, medium = C.gold, low = C.teal}
local PRIORITY_LABELS = {high = "HIGH", medium = "MED", low = "LOW"}
local VISIBLE_TASKS = 5

local state = {
    screen = "list",
    tasks = {},
    cursor = 0,
    scroll = 0,
    list_tab = 1,
    add_text = "",
    add_priority = 2,
    add_cursor = 0, -- 0=text, 1=priority, 2=confirm
    add_error = "",
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

local function save_tasks()
    storage.save("tasks", state.tasks)
    storage.save("stats", state.stats)
end

local function load_tasks()
    local tasks = storage.load("tasks")
    if tasks then state.tasks = tasks end
    local stats = storage.load("stats")
    if stats then
        state.stats.completed = stats.completed or 0
        state.stats.total_ever = stats.total_ever or #state.tasks
    end
end

local function done_count()
    local count = 0
    for _, task in ipairs(state.tasks) do
        if task.done then count = count + 1 end
    end
    return count
end

local function visible_tasks()
    local tasks = {}
    for _, task in ipairs(state.tasks) do
        if (state.list_tab == 2) == (task.done == true) then tasks[#tasks + 1] = task end
    end
    return tasks
end

local function draw_task_list()
    local tasks = visible_tasks()
    local count = #tasks
    local completed = done_count()
    local open = #state.tasks - completed
    U.header("Todo", "TASKS / DAILY REGISTER", #state.tasks .. " STORED", C.muted)
    U.panel(24, 112, 440, 88, C.panel, C.red)
    U.display(tostring(open), 44, 118, 56, C.paper, 120)
    U.text("TASKS TO DO", 180, 130, 18, C.paper, 260, true)
    U.text(count > VISIBLE_TASKS and ((state.cursor + 1) .. " / " .. count .. " in this view") or "One task at a time.", 180, 164, 14, C.muted, 260)
    U.panel(480, 112, 216, 88, C.panel)
    U.text("COMPLETED", 496, 124, 14, C.muted, 184, true)
    U.display(tostring(completed), 496, 145, 38, C.teal, 184)
    U.tabs({"Open / " .. open, "Completed / " .. completed}, state.list_tab, 212)

    if count == 0 then
        if state.list_tab == 2 then
            U.empty("Nothing completed yet", "L1 returns to Open. Select a task and press X to finish it.", 292)
        elseif completed > 0 then
            U.empty("All tasks completed", "A starts a new task. R1 opens the completed register.", 292)
        else
            U.empty("A clean page", "Press A to write your first task. Choose its priority, then save it.", 292)
        end
    else
        state.cursor = math.max(0, math.min(state.cursor, count - 1))
        state.scroll = math.max(0, math.min(state.scroll, count - VISIBLE_TASKS))
        if state.cursor < state.scroll then state.scroll = state.cursor end
        if state.cursor >= state.scroll + VISIBLE_TASKS then state.scroll = state.cursor - VISIBLE_TASKS + 1 end
        for index = state.scroll + 1, math.min(count, state.scroll + VISIBLE_TASKS) do
            local task = tasks[index]
            local y = 260 + (index - state.scroll - 1) * 78
            local selected = index - 1 == state.cursor
            local fg, secondary = U.row(24, y, 672, 70, selected)
            local priority = PRIORITY_COLORS[task.priority] or C.gold
            -- Priority ink sits on graphite even when the task row is selected.
            U.rect(36, y + 10, 62, 50, C.panel2)
            U.text(PRIORITY_LABELS[task.priority] or "MED", 44, y + 15, 14, priority, 50, true)
            U.text(task.done and "DONE" or string.format("%02d", index), 44, y + 37, 14,
                task.done and C.teal or C.paper, 50)
            local lines = U.wrap(task.text, 480, 18, 2, selected)
            for i, line in ipairs(lines) do
                U.text(line, 112, y + 11 + (i - 1) * 23, 18, fg, 480, selected)
            end
            local stamp = task.created and os.date("%d %b", task.created) or ""
            U.text(stamp, 608, y + 13, 14, secondary, 72)
            U.text(task.done and "done" or "open", 608, y + 39, 14, secondary, 72)
        end
    end
    U.footer({{"A", "Add"}, {"X", state.list_tab == 1 and "Done" or "Reopen"},
        {"Y", "Delete"}, {"L1/R1", "View"}, {"D-PAD", "Task"}})
end

local function draw_add_screen()
    U.header("New task", "TASKS / WRITE AN ENTRY", "LOCAL STORAGE", C.muted)
    U.panel(24, 112, 672, 118, C.panel, state.add_cursor == 0 and C.red or nil)
    U.text("TASK TEXT", 44, 126, 14, C.muted, 480, true)
    U.text(#state.add_text .. " / 60", 596, 126, 14, C.muted, 80)
    local value = state.add_text ~= "" and state.add_text or "Choose letters on the keyboard below."
    local lines = U.wrap(value, 632, 18, 2)
    for i, line in ipairs(lines) do
        U.text(line, 44, 151 + (i - 1) * 23, 18, state.add_text == "" and C.muted or C.paper, 632)
    end
    U.text(state.add_error ~= "" and state.add_error or "L1 / R1 moves between text, priority and save.",
        44, 208, 14, state.add_error ~= "" and C.red or C.muted, 632)
    U.tabs({"01 / Type", "02 / Priority", "03 / Save"}, state.add_cursor + 1, 246)

    U.panel(24, 298, 672, 54, C.panel, state.add_cursor == 1 and C.red or nil)
    U.text("PRIORITY", 44, 316, 14, C.muted, 150, true)
    for i, priority in ipairs(PRIORITIES) do
        local x = 224 + (i - 1) * 156
        local active = i == state.add_priority
        U.rect(x, 306, 144, 38, active and PRIORITY_COLORS[priority] or C.panel2)
        U.text(PRIORITY_LABELS[priority], x + 16, 315, 16, active and C.black or C.muted, 112, true)
    end

    U.text("KEYBOARD / A enters the selected key", 24, 366, 14, C.muted, 672, true)
    for row, keys in ipairs(KB_ROWS) do
        for col, key in ipairs(keys) do
            local x = 24 + (col - 1) * 68
            local y = 390 + (row - 1) * 50
            local selected = state.add_cursor == 0 and state.kb_row == row - 1 and state.kb_col == col - 1
            U.rect(x, y, 60, 42, selected and C.paper or C.panel2)
            if selected then U.rect(x, y + 40, 60, 2, C.red) end
            local label = key == " " and "SPC" or (key == "<" and "DEL" or key)
            local size = #label == 1 and 18 or 14
            local width = screen.get_text_width(label, size, true)
            U.text(label, x + (60 - width) / 2, y + 10, size, selected and C.black or C.paper, 48, true)
        end
    end
    local fg, secondary = U.row(24, 596, 672, 54, state.add_cursor == 2)
    U.display("ADD TASK", 44, 603, 30, fg, 184)
    U.text(state.add_cursor == 2 and "A saves this entry" or "R1 moves to Save", 252, 614, 18, secondary, 424)
    U.footer({{"A", state.add_cursor == 2 and "Save" or "Select"}, {"B", "Cancel"},
        {"L1/R1", "Section"}, {"D-PAD", state.add_cursor == 1 and "Priority" or "Key"}})
end

local function handle_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end
    if state.screen == "list" then
        local tasks = visible_tasks()
        local count = #tasks
        if button == "a" and action == "press" then
            state.screen = "add"
            state.add_text, state.add_error = "", ""
            state.add_priority, state.add_cursor = 2, 0
            state.kb_row, state.kb_col = 0, 0
        elseif button == "x" and count > 0 and action == "press" then
            local task = tasks[state.cursor + 1]
            if task then
                task.done = not task.done
                state.stats.completed = math.max(0, state.stats.completed + (task.done and 1 or -1))
                save_tasks()
                state.cursor = math.max(0, math.min(state.cursor, count - 2))
            end
        elseif button == "y" and count > 0 and action == "press" then
            local task = tasks[state.cursor + 1]
            if task then
                for i, stored in ipairs(state.tasks) do
                    if stored == task then table.remove(state.tasks, i); break end
                end
                save_tasks()
                state.cursor = math.max(0, math.min(state.cursor, count - 2))
            end
        elseif button == "l1" or button == "r1" then
            local tab = button == "l1" and 1 or 2
            if state.list_tab ~= tab then state.list_tab, state.cursor, state.scroll = tab, 0, 0 end
        elseif button == "dpad_up" then
            state.cursor = math.max(0, state.cursor - 1)
        elseif button == "dpad_down" then
            state.cursor = math.min(math.max(0, count - 1), state.cursor + 1)
        end
    elseif state.screen == "add" then
        if button == "b" then state.screen = "list"; return end
        if button == "l1" then state.add_cursor = math.max(0, state.add_cursor - 1); return end
        if button == "r1" then state.add_cursor = math.min(2, state.add_cursor + 1); return end
        if state.add_cursor == 0 then
            if button == "dpad_up" then state.kb_row = math.max(0, state.kb_row - 1)
            elseif button == "dpad_down" then state.kb_row = math.min(#KB_ROWS - 1, state.kb_row + 1)
            elseif button == "dpad_left" then state.kb_col = math.max(0, state.kb_col - 1)
            elseif button == "dpad_right" then state.kb_col = math.min(#KB_ROWS[state.kb_row + 1] - 1, state.kb_col + 1)
            elseif button == "a" then
                local key = KB_ROWS[state.kb_row + 1][state.kb_col + 1]
                if key == "<" then state.add_text = state.add_text:sub(1, -2)
                elseif #state.add_text < 60 then state.add_text = state.add_text .. key end
                state.add_error = ""
            end
        elseif state.add_cursor == 1 then
            if button == "dpad_left" or button == "a" then
                state.add_priority = (state.add_priority - 2) % #PRIORITIES + 1
            elseif button == "dpad_right" then state.add_priority = state.add_priority % #PRIORITIES + 1 end
        elseif state.add_cursor == 2 and button == "a" and action == "press" then
            local text = state.add_text:match("^%s*(.-)%s*$")
            if text == "" then state.add_error = "Enter a task before saving. L1 twice returns to text."; return end
            state.tasks[#state.tasks + 1] = {text = text, priority = PRIORITIES[state.add_priority], done = false, created = os.time()}
            state.stats.total_ever = state.stats.total_ever + 1
            save_tasks()
            state.screen, state.list_tab, state.scroll = "list", 1, 0
            state.cursor = #state.tasks - done_count() - 1
        end
    end
end

local function input_snapshot()
    return table.concat({state.screen, state.list_tab, state.cursor, state.scroll,
        state.add_text, state.add_priority, state.add_cursor, state.add_error,
        state.kb_row, state.kb_col, #state.tasks, done_count()}, "\0")
end

function on_input(button, action)
    local before = input_snapshot()
    handle_input(button, action)
    if before ~= input_snapshot() then app.request_redraw() end
end

function on_init()
    load_tasks()
end

function on_update(dt)
    return false
end

function on_render()
    if state.screen == "list" then draw_task_list()
    elseif state.screen == "add" then draw_add_screen() end
end
