
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
    check(update_app(10) == false, "Calculator idle is clean")
    check(evaluate("2^3^2") == 512, "right associative powers")
    check(near(evaluate("sqrt(81)+sin(pi/2)"), 10), "radians and functions")
    check(near(evaluate("1250*15%"), 187.5), "percent semantics")
    check(evaluate("1/0") == nil and evaluate("1+") == nil, "invalid expressions remain invalid")
    press("a")
    check(state.expression == "0", "default selected key")
    press("b")
    check(state.expression == "", "delete binding")
    state.expression = "(1200-175)*1.18"
    state.cursor_row, state.cursor_col = 5, 5
    press("a")
    check(#state.history == 1 and state.history[1].result == "1209.5", "evaluate and save")
    check(saved.history.entries[1].expr == "(1200-175)*1.18", "compatible history shape")
    press("y"); press("a")
    check(#state.screen_stack == 1 and state.expression == "(1200-175)*1.18", "history recall binding")
    press("y"); press("x")
    check(#state.history == 0 and #saved.history.entries == 0, "clear history binding")
    press("dpad_down")
    check(state.history_selected == 1, "empty history selection remains valid")
    local before = redraws
    press("dpad_down")
    check(redraws == before, "empty history navigation is clean")

    print("ASSIGNED HARNESS PASSED: " .. assertions .. " assertions")
end

on_init()
