-- Hello World cartridge: a minimal example covering the lifecycle,
-- drawing primitives, input, and audio. Read this top-to-bottom to
-- learn the CartridgeOS Lua API.

local state = {
    x = 360,
    y = 360,
    color_idx = 1,
    presses = 0,
}

local PALETTE = {
    {255, 100, 100},  -- red
    {100, 255, 100},  -- green
    {100, 200, 255},  -- blue
    {255, 220, 100},  -- yellow
    {220, 100, 255},  -- magenta
}

-- on_init() is called once at startup.
function on_init()
    print("Hello World cartridge starting!")
end

-- on_input(button, action) fires for every button event.
-- See docs/lua-api.md for the full button list.
function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local step = 20
    if button == "dpad_up" then    state.y = state.y - step end
    if button == "dpad_down" then  state.y = state.y + step end
    if button == "dpad_left" then  state.x = state.x - step end
    if button == "dpad_right" then state.x = state.x + step end

    if button == "a" then
        state.presses = state.presses + 1
        state.color_idx = (state.color_idx % #PALETTE) + 1
        if audio then audio.beep(880, 100) end
    end

    -- Clamp to screen bounds
    state.x = math.max(40, math.min(SCREEN_WIDTH - 40, state.x))
    state.y = math.max(80, math.min(SCREEN_HEIGHT - 80, state.y))
end

-- on_update(dt) runs every frame with delta time. Use for animations.
-- This cartridge has no animation, so it's empty.
function on_update(dt)
end

-- on_render() runs every frame, after on_update. ONLY place you can
-- call screen.* functions.
function on_render()
    -- Always start with a clear if you want a known background.
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    -- Title bar
    screen.draw_rect(0, 0, SCREEN_WIDTH, 50, {
        color = theme.bg_header, filled = true,
    })
    screen.draw_text("Hello World", 16, 14, {
        color = theme.accent, size = 22, bold = true,
    })

    -- Status text
    local status = string.format("Presses: %d", state.presses)
    screen.draw_text(status, SCREEN_WIDTH - 200, 18, {
        color = theme.text_dim, size = 14,
    })

    -- The "player" — draw a circle at state.x, state.y in the current color
    local c = PALETTE[state.color_idx]
    screen.draw_circle(state.x, state.y, 30, c[1], c[2], c[3])

    -- A border around the playfield
    screen.draw_rect(20, 60, SCREEN_WIDTH - 40, SCREEN_HEIGHT - 120, {
        color = theme.card_border, filled = false, radius = 8,
    })

    -- Footer hints
    local fy = SCREEN_HEIGHT - 50
    screen.draw_rect(0, fy, SCREEN_WIDTH, 50, {color = theme.bg_header, filled = true})
    screen.draw_button_hint("D-pad", "Move", 16, fy + 16, {color = theme.btn_l})
    screen.draw_button_hint("A", "Color + beep", 200, fy + 16, {color = theme.btn_a})
    screen.draw_button_hint("Select", "Quit", 480, fy + 16, {color = theme.btn_b})
end

-- on_destroy() runs when the cartridge exits. Clean up anything that
-- needs explicit teardown (e.g. ssh.close()).
function on_destroy()
    print("Hello World cartridge exiting. Goodbye!")
end
