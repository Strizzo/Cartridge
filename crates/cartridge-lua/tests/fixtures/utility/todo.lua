
local function copy(value)
    if type(value) ~= "table" then return value end
    local result = {}
    for key, item in pairs(value) do result[key] = copy(item) end
    return result
end
local baseline = copy(state)
local saved, redraws, assertions = {}, 0, 0
storage.save = function(key, value) saved[key] = copy(value) end
app.request_redraw = function() redraws = redraws + 1 end
if audio then audio.beep = function() end end
local update_app, input_app = on_update, on_input
local function reset()
    for key in pairs(state) do state[key] = nil end
    for key, value in pairs(baseline) do state[key] = copy(value) end
    saved, redraws = {}, 0
end
local function check(condition, message)
    assertions = assertions + 1
    assert(condition, message)
end
local function press(button) input_app(button, "press") end
local function near(a, b) return math.abs(a - b) < 0.00001 end
function on_init()

    reset()
    check(update_app(10) == false, "Todo idle is clean")
    press("a")
    check(state.screen == "add" and state.add_cursor == 0, "open keyboard")
    press("a"); press("dpad_right"); press("a")
    check(state.add_text == "AB", "keyboard enters selected keys")
    press("r1"); press("dpad_left")
    check(state.add_priority == 1, "priority controller")
    press("r1"); press("a")
    check(state.screen == "list" and #state.tasks == 1 and state.tasks[1].text == "AB", "save controller path")
    check(state.tasks[1].priority == "high" and state.tasks[1].done == false and state.tasks[1].created ~= nil, "saved task shape")
    check(saved.tasks[1].text == "AB" and saved.stats.total_ever == 1, "compatible persistence keys")
    press("x")
    check(state.tasks[1].done and #visible_tasks() == 0 and state.stats.completed == 1, "complete selected task")
    input_app("x", "repeat")
    check(state.stats.completed == 1, "repeat does not complete another task")
    press("r1")
    check(#visible_tasks() == 1 and state.list_tab == 2, "completed register")
    press("x")
    check(not state.tasks[1].done and #visible_tasks() == 0 and state.stats.completed == 0, "reopen selected task")
    press("l1")
    check(#visible_tasks() == 1 and state.tasks[1].priority == "high", "reopen preserves priority")
    press("y")
    check(#state.tasks == 0 and #saved.tasks == 0, "delete persistence")

    reset()
    press("a"); press("r1"); press("r1"); press("a")
    check(state.screen == "add" and state.add_error ~= "" and #state.tasks == 0, "blank entry shows repair action")
    state.add_text = "   "
    press("a")
    check(#state.tasks == 0, "spaces alone are not a task")
    state.add_text = "  A REAL TASK  "
    press("a")
    check(state.tasks[1].text == "A REAL TASK", "trim whitespace before saving")
    press("a")
    for i = 1, 65 do input_app("a", "repeat") end
    check(#state.add_text == 60, "keyboard respects existing length limit")
    state.kb_row, state.kb_col = 3, 9
    press("a")
    check(#state.add_text == 59, "DEL removes one character")
    press("b")
    check(state.screen == "list" and #state.tasks == 1, "cancel does not persist draft")
    local before = redraws
    press("dpad_up")
    input_app("x", "release")
    check(redraws == before, "boundary and release are clean")

    print("ASSIGNED HARNESS PASSED: " .. assertions .. " assertions")
end

on_init()
