-- Perf bench cartridge: draws a deliberately heavy synthetic frame so
-- `perf-bench app lua_cartridges/bench` exercises the whole Lua draw path.
--
-- Per frame: a gradient header, ~40 cards with shadows, ~150 draw_text
-- calls (a third of them with strings that change every frame), 8
-- sparklines of 130 points, a footer with button hints. A highlight
-- cycles across the cards on a timer. on_update always returns true so
-- dirty rendering never skips a frame -- the bench measures rendering.

local COLS, ROWS = 5, 8            -- 40 cards
local CARD_W, CARD_H = 136, 70
local GAP = 5
local GRID_X, GRID_Y = 10, 50
local SPARK_POINTS = 130
local SPARK_ROWS = 8               -- one sparkline per row (8 total)

local state = {
    t = 0,
    frame = 0,
    highlight = 1,
    sparks = {},
    values = {},
}

local LABELS = {
    "CPU", "RAM", "NET", "DISK", "TEMP", "BATT", "LOAD", "RX", "TX", "IO",
}

function on_init()
    -- 30fps while idle so the highlight timer looks alive if run visibly.
    app.set_idle_fps(30)
    for s = 1, SPARK_ROWS do
        local series = {}
        for i = 1, SPARK_POINTS do
            series[i] = 50 + 40 * math.sin(i / (6 + s)) + (i * s) % 11
        end
        state.sparks[s] = series
    end
    for i = 1, COLS * ROWS do
        state.values[i] = (i * 37) % 100
    end
end

function on_update(dt)
    state.t = state.t + dt
    state.frame = state.frame + 1
    -- Cycle the highlighted card every 100ms.
    state.highlight = (math.floor(state.t / 0.1) % (COLS * ROWS)) + 1
    -- Rotate every series by one sample so sparklines move.
    for s = 1, SPARK_ROWS do
        local series = state.sparks[s]
        local first = table.remove(series, 1)
        series[#series + 1] = first
    end
    -- Bump every third value so a third of the card texts change per frame.
    for i = 1, COLS * ROWS, 3 do
        state.values[i] = (state.values[i] + 1) % 1000
    end
    return true
end

local function draw_header()
    screen.draw_gradient_rect(0, 0, 720, 40,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 720, 0, {color = theme.accent})
    screen.draw_text("Perf Bench", 12, 10, {color = theme.text, size = 20, bold = true})
    local right = string.format("frame %d  t=%.1fs", state.frame, state.t)
    local rw = screen.get_text_width(right, 12, false)
    screen.draw_text(right, 704 - rw, 14, {color = theme.text_dim, size = 12})
    screen.draw_pill("LIVE", 160, 12, theme.positive.r, theme.positive.g, theme.positive.b,
        {text_color = {20, 20, 30}, size = 10})
end

local function draw_cards()
    local idx = 0
    for row = 0, ROWS - 1 do
        for col = 0, COLS - 1 do
            idx = idx + 1
            local x = GRID_X + col * (CARD_W + GAP)
            local y = GRID_Y + row * (CARD_H + GAP)
            local selected = (idx == state.highlight)
            if selected then
                screen.draw_card(x, y, CARD_W, CARD_H,
                    {bg = theme.card_highlight, border = theme.accent, radius = 6, shadow = true})
            else
                screen.draw_card(x, y, CARD_W, CARD_H,
                    {bg = theme.card_bg, radius = 6, shadow = true})
            end

            local label = LABELS[(idx % #LABELS) + 1] .. " " .. idx
            screen.draw_text(label, x + 8, y + 6, {color = theme.text_dim, size = 11})
            -- Values on every third card change each frame (cache misses).
            local value = string.format("%d%%", state.values[idx])
            screen.draw_text(value, x + 8, y + 22, {color = theme.text, size = 16, bold = true})
            screen.draw_text("avg 42 / max 97", x + 8, y + 46,
                {color = theme.text_dim, size = 10, max_width = CARD_W - 16})

            if col == COLS - 1 and row < SPARK_ROWS then
                -- 8 sparklines of 130 points, one per row, in the last column.
                screen.draw_sparkline(state.sparks[row + 1], x + 70, y + 20, 60, 44,
                    {color = theme.accent, baseline_color = theme.border})
            elseif col == 2 then
                screen.draw_progress_bar(x + 70, y + 30, 58, 6, (state.values[idx] % 100) / 100, {})
            end
        end
    end
end

local function draw_ticker()
    -- 24 small changing strings along one row.
    local y = GRID_Y + ROWS * (CARD_H + GAP) + 4
    local x = 10
    for i = 1, 24 do
        local s = string.format("%02d:%03d", i, (state.frame * i) % 1000)
        screen.draw_text(s, x, y, {color = theme.text_accent, size = 10})
        x = x + 29
    end
end

local function draw_footer()
    screen.draw_rect(0, 684, 720, 36, {color = theme.bg_header, filled = true})
    screen.draw_line(0, 684, 720, 684, {color = theme.border})
    local x = 10
    local hints = {
        {"A", "Select", theme.btn_a},
        {"B", "Back", theme.btn_b},
        {"X", "Refresh", theme.btn_x},
        {"L1/R1", "Tab", theme.btn_l},
    }
    for _, h in ipairs(hints) do
        local w = screen.draw_button_hint(h[1], h[2], x, 692, {color = h[3], size = 12})
        x = x + w + 14
    end
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)
    draw_header()
    draw_cards()
    draw_ticker()
    draw_footer()
end
