
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
    check(update_app(10) == false and state.remaining == 1500, "idle must be clean")
    check(format_time() == "25:00", "initial duration")
    press("a")
    check(state.running and state.session_start ~= nil and redraws == 1, "start")
    check(update_app(0.125) == false and near(state.remaining, 1499.875), "subsecond tick is clean")
    check(update_app(0.875) == true and format_time() == "24:59", "second boundary redraw")
    press("a")
    local remaining = state.remaining
    check(not state.running and update_app(10) == false and state.remaining == remaining, "paused timer does not tick")
    press("x")
    check(state.remaining == 1500 and state.session_start == nil and state.session_elapsed == 0, "reset discards partial session")
    local before = redraws
    press("x")
    press("dpad_left")
    input_app("a", "release")
    check(redraws == before, "no-op input is clean")

    reset()
    press("y")
    check(state.phase == PHASE_SHORT_BREAK and state.total_completed == 0 and state.work_count == 0 and #state.sessions == 0, "unstarted work skip is not a completion")
    press("y")
    press("a")
    update_app(61.8)
    press("y")
    check(state.phase == PHASE_SHORT_BREAK and state.total_completed == 0 and state.work_count == 0, "partial skip does not count toward long rest")
    check(#state.sessions == 1 and state.sessions[1].completed == false and state.sessions[1].duration_min == 1, "skipped history captures actual time")
    check(saved.stats.completed == 0 and saved.stats.sessions[1].completed == false, "skip persistence shape")

    reset()
    set_duration(PHASE_WORK, 15)
    press("a")
    check(update_app(2000) == true, "phase completion is dirty")
    check(state.phase == PHASE_SHORT_BREAK and not state.running and state.remaining == 300, "completion waits at next phase")
    check(state.total_completed == 1 and state.total_focus_seconds == 900 and state.work_count == 1, "large delta cannot overcount focus")
    check(state.sessions[1].duration_min == 15 and state.sessions[1].completed == true, "completed duration")
    for i = 2, 4 do press("y"); press("a"); update_app(2000) end
    check(state.phase == PHASE_LONG_BREAK and state.work_count == 0 and state.total_completed == 4, "four completed work phases earn long rest")
    check(state.remaining == 900 and not state.running, "long rest uses 15 minute default")
    press("y")
    check(state.phase == PHASE_WORK and state.total_completed == 4, "rest skip leaves totals unchanged")

    reset()
    press("a")
    update_app(60)
    set_duration(PHASE_WORK, 30)
    check(state.phase_duration == 1500 and state.remaining == 1440 and state.running, "running duration is fixed")
    update_app(2000)
    check(state.sessions[1].duration_min == 25 and state.total_focus_seconds == 1500, "running settings cannot change logged duration")
    press("y")
    check(state.phase_duration == 1800 and state.remaining == 1800, "next work adopts new duration")
    press("a"); update_app(30); press("a")
    set_duration(PHASE_WORK, 20)
    check(state.remaining == 1200 and state.session_start == nil and state.session_elapsed == 0, "paused settings reset partial session")

    reset()
    press("a"); press("r1")
    check(update_app(2) == false and near(state.remaining, 1498), "hidden timer ticks without redrawing stats")
    press("b"); press("l1")
    check(state.settings_indices[3] == 2, "long rest default selection matches actual duration")
    check(update_app(2) == false and near(state.remaining, 1496), "hidden timer ticks without redrawing settings")
    press("dpad_right")
    check(state.durations[PHASE_WORK] == 1800 and state.phase_duration == 1500, "public settings controller path")
    press("b"); press("a"); press("x")
    check(not state.running and state.remaining == 1800, "pause and reset adopt selected duration")

    reset()
    set_duration(PHASE_WORK, 15)
    press("a")
    for i = 1, 27001 do update_app(1 / 30) end
    check(state.total_focus_seconds == 900 and state.sessions[1].duration_min == 15, "fractional steps preserve complete minutes")

    print("ASSIGNED HARNESS PASSED: " .. assertions .. " assertions")
end

on_init()
