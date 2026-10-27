-- CartridgeOS v0.7.0 starter. See docs/app-development.md.
local count = 0
local status = "Press A to start"
local save_failed = false
local MAX_COUNT = 999999

function on_init()
    -- Storage exists because cartridge.json grants the storage permission.
    local saved = storage.load("counter")
    if type(saved) == "table" and type(saved.count) == "number"
        and saved.count >= 0 and saved.count <= MAX_COUNT
        and saved.count == math.floor(saved.count) then
        count = saved.count
        status = "Restored saved count"
    end
    -- A static app needs very little idle work. Input still wakes the loop.
    app.set_idle_fps(1)
end

local function set_count(value)
    if value == count then return end
    count = value
    -- Save only on a change, never in on_update or on_render.
    local ok = pcall(storage.save, "counter", {count = count})
    save_failed = not ok
    status = ok and "Saved locally" or "Save failed; count is in memory"
    app.request_redraw()
end

function on_input(button, action)
    -- Ignore releases and auto-repeat: one increment per physical press.
    if action ~= "press" then return end
    if button == "a" then
        set_count(math.min(count + 1, MAX_COUNT))
    elseif button == "b" then
        set_count(0)
    end
    -- Select is handled by the runtime and exits before reaching this callback.
end

function on_update(dt)
    -- No animation or polling. Returning true here would redraw every update.
    return false
end

function on_render()
    -- All screen.* calls belong in this callback.
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)
    screen.draw_text("Hello, Cartridge", 40, 42, {
        size = 40, bold = true, color = theme.text,
    })
    screen.draw_text("Your first Lua app", 40, 104, {
        size = 26, color = theme.text_dim,
    })
    screen.draw_rect(40, 176, 640, 288, {color = theme.card_bg})
    screen.draw_text("COUNT", 64, 200, {size = 24, color = theme.text_dim})
    screen.draw_text(tostring(count), 64, 244, {
        size = 112, bold = true, color = theme.accent,
    })
    screen.draw_text(status, 64, 412, {
        size = 22, color = save_failed and theme.text_error or theme.text_dim,
    })
    screen.draw_text("A  Count +1       B  Reset", 40, 516, {
        size = 28, color = theme.text,
    })
    screen.draw_text("Select  Exit", 40, 568, {size = 28, color = theme.text})
    screen.draw_text("Keyboard: Z = A, X = B, Space = Select", 40, 648, {
        size = 22, color = theme.text_dim,
    })
end

function on_destroy()
    -- Each change was already saved; this app has no resources to close.
end
