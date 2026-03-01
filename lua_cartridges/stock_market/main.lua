-- Stock Market Viewer for Cartridge OS (Lua)
-- Track stocks, crypto, indices, and your watchlist

-- ── Data ─────────────────────────────────────────────────────────────────────

local DEFAULT_WATCHLIST = {
    {symbol="AAPL", name="Apple"},
    {symbol="GOOGL", name="Alphabet"},
    {symbol="MSFT", name="Microsoft"},
    {symbol="AMZN", name="Amazon"},
    {symbol="NVDA", name="NVIDIA"},
    {symbol="TSLA", name="Tesla"},
    {symbol="META", name="Meta"},
}

local DEFAULT_INDICES = {
    {symbol="^GSPC", name="S&P 500"},
    {symbol="^IXIC", name="NASDAQ"},
    {symbol="^DJI", name="Dow Jones"},
    {symbol="^RUT", name="Russell 2000"},
}

local DEFAULT_CRYPTO = {
    {symbol="BTC-USD", name="Bitcoin"},
    {symbol="ETH-USD", name="Ethereum"},
    {symbol="SOL-USD", name="Solana"},
    {symbol="XRP-USD", name="XRP"},
    {symbol="ADA-USD", name="Cardano"},
    {symbol="DOGE-USD", name="Dogecoin"},
    {symbol="AVAX-USD", name="Avalanche"},
    {symbol="DOT-USD", name="Polkadot"},
}

local STOCK_UNIVERSE = {
    {symbol="AAPL", name="Apple", sector="Technology", tier="mega"},
    {symbol="MSFT", name="Microsoft", sector="Technology", tier="mega"},
    {symbol="GOOGL", name="Alphabet", sector="Technology", tier="mega"},
    {symbol="AMZN", name="Amazon", sector="Technology", tier="mega"},
    {symbol="NVDA", name="NVIDIA", sector="Technology", tier="mega"},
    {symbol="META", name="Meta Platforms", sector="Technology", tier="mega"},
    {symbol="TSLA", name="Tesla", sector="Technology", tier="mega"},
    {symbol="AVGO", name="Broadcom", sector="Technology", tier="mega"},
    {symbol="ORCL", name="Oracle", sector="Technology", tier="large"},
    {symbol="CRM", name="Salesforce", sector="Technology", tier="large"},
    {symbol="AMD", name="AMD", sector="Technology", tier="large"},
    {symbol="INTC", name="Intel", sector="Technology", tier="large"},
    {symbol="JPM", name="JPMorgan Chase", sector="Finance", tier="mega"},
    {symbol="V", name="Visa", sector="Finance", tier="mega"},
    {symbol="MA", name="Mastercard", sector="Finance", tier="mega"},
    {symbol="BAC", name="Bank of America", sector="Finance", tier="large"},
    {symbol="GS", name="Goldman Sachs", sector="Finance", tier="large"},
    {symbol="UNH", name="UnitedHealth", sector="Healthcare", tier="mega"},
    {symbol="JNJ", name="Johnson & Johnson", sector="Healthcare", tier="mega"},
    {symbol="LLY", name="Eli Lilly", sector="Healthcare", tier="mega"},
    {symbol="ABBV", name="AbbVie", sector="Healthcare", tier="large"},
    {symbol="MRK", name="Merck", sector="Healthcare", tier="large"},
    {symbol="XOM", name="Exxon Mobil", sector="Energy", tier="mega"},
    {symbol="CVX", name="Chevron", sector="Energy", tier="mega"},
    {symbol="COP", name="ConocoPhillips", sector="Energy", tier="large"},
    {symbol="WMT", name="Walmart", sector="Consumer", tier="mega"},
    {symbol="PG", name="Procter & Gamble", sector="Consumer", tier="mega"},
    {symbol="KO", name="Coca-Cola", sector="Consumer", tier="mega"},
    {symbol="COST", name="Costco", sector="Consumer", tier="mega"},
    {symbol="HD", name="Home Depot", sector="Consumer", tier="mega"},
    {symbol="CAT", name="Caterpillar", sector="Industrial", tier="large"},
    {symbol="HON", name="Honeywell", sector="Industrial", tier="large"},
    {symbol="BA", name="Boeing", sector="Industrial", tier="large"},
    {symbol="UPS", name="UPS", sector="Industrial", tier="large"},
    {symbol="NFLX", name="Netflix", sector="Communication", tier="mega"},
    {symbol="DIS", name="Walt Disney", sector="Communication", tier="large"},
    {symbol="T", name="AT&T", sector="Communication", tier="large"},
    {symbol="BTC-USD", name="Bitcoin", sector="Crypto", tier="mega"},
    {symbol="ETH-USD", name="Ethereum", sector="Crypto", tier="mega"},
    {symbol="SOL-USD", name="Solana", sector="Crypto", tier="mega"},
    {symbol="XRP-USD", name="XRP", sector="Crypto", tier="mega"},
    {symbol="ADA-USD", name="Cardano", sector="Crypto", tier="large"},
    {symbol="DOGE-USD", name="Dogecoin", sector="Crypto", tier="large"},
}

local SECTORS = {"Technology", "Finance", "Healthcare", "Energy", "Consumer", "Industrial", "Communication", "Crypto"}
local SECTOR_LABELS = {
    Technology="Tech", Finance="Finance", Healthcare="Health", Energy="Energy",
    Consumer="Consumer", Industrial="Industry", Communication="Comm", Crypto="Crypto",
}
local TIER_COLORS = {
    mega={255, 200, 60}, large={100, 180, 255}, mid={140, 140, 160},
}
local PERIODS = {{"1D","1d"}, {"1W","5d"}, {"1M","1mo"}, {"3M","3mo"}, {"1Y","1y"}}
local PERIOD_INTERVALS = {["1d"]="5m", ["5d"]="15m", ["1mo"]="1d", ["3mo"]="1d", ["1y"]="1wk"}

-- ── State ────────────────────────────────────────────────────────────────────

local state = {
    screen_stack = {"watchlist"},
    -- Watchlist screen
    tabs = {"Watchlist", "Indices", "Crypto"},
    tab_ids = {"watchlist", "indices", "crypto"},
    active_tab = 1,
    watchlist = {},
    quotes = {},         -- tab_id -> list of quotes
    sparklines = {},     -- symbol -> list of prices
    cursor = 1,
    list_period_idx = 2, -- default to 1W
    loading = false,
    refresh_timer = 0,
    refresh_interval = 120.0,
    -- Detail screen
    detail_quote = nil,
    detail_history = nil,
    detail_period_idx = 2,
    detail_loading = false,
    -- Browse screen
    browse_sector_idx = 1,
    browse_cursor = 1,
    browse_stocks = {},
    needs_initial_load = false,
    ready_to_load = false,
}

local CARD_RADIUS = 6
local ROW_HEIGHT = 56

-- ── Helpers ──────────────────────────────────────────────────────────────────

local function draw_header(title, right_text, right_color)
    screen.draw_gradient_rect(0, 0, 720, 40,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 720, 0, {color=theme.accent})
    screen.draw_text(title, 12, 10, {color=theme.text, size=20, bold=true})
    if right_text then
        local rc = right_color or theme.text_dim
        local rw = screen.get_text_width(right_text, 12, false)
        screen.draw_text(right_text, 704 - rw, 14, {color=rc, size=12})
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

local function draw_tab_bar(tabs, active_idx, y)
    screen.draw_rect(0, y, 720, 30, {color=theme.bg_header, filled=true})
    local tx = 10
    for i, label in ipairs(tabs) do
        local is_active = (i == active_idx)
        local tw = screen.get_text_width(label, 12, is_active)
        local tab_w = tw + 16
        if is_active then
            screen.draw_rect(tx, y + 4, tab_w, 22, {color=theme.accent, filled=true, radius=4})
            screen.draw_text(label, tx + 8, y + 7, {color={20, 20, 30}, size=12, bold=true})
        else
            screen.draw_rect(tx, y + 4, tab_w, 22, {color=theme.card_bg, filled=true, radius=4})
            screen.draw_text(label, tx + 8, y + 7, {color=theme.text_dim, size=12})
        end
        tx = tx + tab_w + 6
    end
    screen.draw_line(0, y + 30, 720, y + 30, {color=theme.border})
end

local function draw_scroll_indicator(y_start, height, cursor, total, visible)
    if total <= visible then return end
    local ind_x = 715
    local bar_top = y_start + 4
    local bar_h = height - 8
    screen.draw_line(ind_x, bar_top, ind_x, bar_top + bar_h, {color=theme.border})
    local thumb_h = math.max(8, math.floor(bar_h * visible / total))
    local progress = (cursor - 1) / math.max(1, total - 1)
    local thumb_y = bar_top + math.floor((bar_h - thumb_h) * progress)
    screen.draw_rect(ind_x - 1, thumb_y, 3, thumb_h, {color=theme.text_dim, filled=true, radius=1})
end

local function is_in_watchlist(symbol)
    for _, w in ipairs(state.watchlist) do
        if w.symbol == symbol then return true end
    end
    return false
end

local function get_name_for_symbol(symbol)
    for _, w in ipairs(state.watchlist) do
        if w.symbol == symbol then return w.name end
    end
    for _, w in ipairs(DEFAULT_INDICES) do
        if w.symbol == symbol then return w.name end
    end
    for _, w in ipairs(DEFAULT_CRYPTO) do
        if w.symbol == symbol then return w.name end
    end
    return symbol
end

local function get_stocks_by_sector(sector)
    local result = {}
    for _, s in ipairs(STOCK_UNIVERSE) do
        if s.sector == sector then
            result[#result + 1] = s
        end
    end
    return result
end

local function save_watchlist()
    local items = {}
    for _, w in ipairs(state.watchlist) do
        items[#items + 1] = {symbol=w.symbol, name=w.name}
    end
    storage.save("watchlist", {items=items})
end

local function load_watchlist()
    local data = storage.load("watchlist")
    if data and data.items then
        local items = {}
        for _, i in ipairs(data.items) do
            items[#items + 1] = {symbol=i.symbol, name=i.name}
        end
        return items
    end
    -- Return copy of defaults
    local items = {}
    for _, w in ipairs(DEFAULT_WATCHLIST) do
        items[#items + 1] = {symbol=w.symbol, name=w.name}
    end
    return items
end

-- ── Yahoo Finance API ──────────────────────────────────────────────────────

local YAHOO_BASE = "https://query1.finance.yahoo.com/v8/finance/chart/"

local function fetch_chart(symbol, range)
    local interval = PERIOD_INTERVALS[range] or "1d"
    local url = YAHOO_BASE .. symbol .. "?range=" .. range .. "&interval=" .. interval .. "&events="
    local ok, resp = pcall(http.get_cached, url, 120)
    if not ok or not resp.ok then return nil end
    local dok, data = pcall(json.decode, resp.body)
    if not dok or not data then return nil end
    local result = data.chart and data.chart.result
    if not result or #result == 0 then return nil end
    return result[1]
end

local function fetch_quote_from_chart(symbol, range)
    local chart = fetch_chart(symbol, range)
    if not chart then return nil, {} end

    local meta = chart.meta or {}
    local price = meta.regularMarketPrice or 0
    local prev_close = meta.chartPreviousClose or meta.previousClose or price
    local change = price - prev_close
    local change_pct = prev_close > 0 and (change / prev_close * 100) or 0

    local closes = {}
    local indicators = chart.indicators
    if indicators and indicators.quote and #indicators.quote > 0 then
        local raw = indicators.quote[1].close or {}
        for _, v in ipairs(raw) do
            if v then closes[#closes + 1] = v end
        end
    end

    local high, low = price, price
    for _, c in ipairs(closes) do
        if c > high then high = c end
        if c < low then low = c end
    end

    local quote = {
        symbol = symbol,
        name = get_name_for_symbol(symbol),
        price = math.floor(price * 100) / 100,
        change = math.floor(change * 100) / 100,
        change_pct = math.floor(change_pct * 100) / 100,
        high = math.floor(high * 100) / 100,
        low = math.floor(low * 100) / 100,
        week52_high = math.floor((meta.fiftyTwoWeekHigh or high) * 100) / 100,
        week52_low = math.floor((meta.fiftyTwoWeekLow or low) * 100) / 100,
    }
    return quote, closes
end

-- ── Load Data ────────────────────────────────────────────────────────────────

local function load_quotes()
    state.loading = true
    local tab_id = state.tab_ids[state.active_tab]
    local symbols = {}

    if tab_id == "watchlist" then
        for _, w in ipairs(state.watchlist) do
            symbols[#symbols + 1] = w.symbol
        end
    elseif tab_id == "crypto" then
        for _, w in ipairs(DEFAULT_CRYPTO) do
            symbols[#symbols + 1] = w.symbol
        end
    else
        for _, w in ipairs(DEFAULT_INDICES) do
            symbols[#symbols + 1] = w.symbol
        end
    end

    local range = PERIODS[state.list_period_idx][2]
    local quotes = {}
    for _, sym in ipairs(symbols) do
        local q, closes = fetch_quote_from_chart(sym, range)
        if q then
            quotes[#quotes + 1] = q
            state.sparklines[sym] = closes
        end
    end
    state.quotes[tab_id] = quotes
    state.loading = false
end

-- ── Watchlist Screen Drawing ─────────────────────────────────────────────────

local function draw_quote_row(q, y, is_selected)
    local card_x = 6
    local card_w = 708
    local card_h = ROW_HEIGHT - 4

    local is_positive = q.change >= 0
    local direction_color = is_positive and theme.positive or theme.negative

    -- Card background
    if is_selected then
        screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_highlight, border=theme.accent, radius=CARD_RADIUS})
    else
        screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_bg, radius=CARD_RADIUS})
    end

    -- Colored left border strip
    screen.draw_rect(card_x, y + 4, 3, card_h - 8, {color=direction_color, filled=true, radius=1})

    -- Symbol (strip -USD suffix for crypto)
    local display_symbol = q.symbol:gsub("%-USD$", "")
    screen.draw_text(display_symbol, card_x + 14, y + 6, {color=theme.text, size=15, bold=true})

    -- Name
    screen.draw_text(q.name, card_x + 14, y + 26, {color=theme.text_dim, size=11})

    -- Sparkline
    local spark_data = state.sparklines[q.symbol]
    if spark_data and #spark_data >= 2 then
        screen.draw_sparkline(spark_data, card_x + 180, y + 8, 100, card_h - 16, {color=direction_color})
    end

    -- Price
    local price_str
    if q.price == 0 then
        price_str = "N/A"
    elseif q.price < 0.01 then
        price_str = string.format("$%.6f", q.price)
    elseif q.price < 1 then
        price_str = string.format("$%.4f", q.price)
    else
        price_str = string.format("$%.2f", q.price)
    end
    local pw = screen.get_text_width(price_str, 15, true)
    screen.draw_text(price_str, card_x + card_w - 14 - pw, y + 4, {color=theme.text, size=15, bold=true})

    -- Change
    local arrow = is_positive and "\226\150\178" or "\226\150\188"
    local sign = is_positive and "+" or ""
    local change_str
    if q.price == 0 then
        change_str = "---"
    else
        change_str = arrow .. " " .. sign .. string.format("%.2f", q.change) .. " (" .. sign .. string.format("%.1f", q.change_pct) .. "%)"
    end
    local cw = screen.get_text_width(change_str, 12, false)
    screen.draw_text(change_str, card_x + card_w - 14 - cw, y + 28, {color=direction_color, size=12})
end

local function draw_watchlist_screen()
    local tab_id = state.tab_ids[state.active_tab]
    local quotes = state.quotes[tab_id] or {}
    local n = #quotes

    draw_header("Stock Market",
        state.loading and "Loading..." or "Live",
        state.loading and theme.text_dim or {100, 220, 100})
    draw_tab_bar(state.tabs, state.active_tab, 40)

    local content_y = 72
    local footer_y = 684
    local content_h = footer_y - content_y

    if state.loading then
        local tw = screen.get_text_width("Fetching quotes...", 16, false)
        screen.draw_text("Fetching quotes...", (720 - tw) / 2, content_y + content_h / 2 - 8, {color=theme.text_dim, size=16})
    elseif n > 0 then
        local visible = math.max(1, math.floor(content_h / ROW_HEIGHT))
        state.cursor = math.max(1, math.min(state.cursor, n))

        local start_idx
        if n <= visible then
            start_idx = 1
        else
            start_idx = math.max(1, math.min(state.cursor - 1, n - visible + 1))
        end
        local end_idx = math.min(start_idx + visible - 1, n)

        local y = content_y + 2
        for idx = start_idx, end_idx do
            draw_quote_row(quotes[idx], y, idx == state.cursor)
            y = y + ROW_HEIGHT
        end

        draw_scroll_indicator(content_y, content_h, state.cursor, n, visible)
    end

    -- Period pills at bottom of list area
    local period_y = footer_y - 28
    screen.draw_rect(0, period_y, 720, 28, {color=theme.bg_header, filled=true})
    screen.draw_line(0, period_y, 720, period_y, {color=theme.border})
    local px = 10
    for i, p in ipairs(PERIODS) do
        local label = p[1]
        local is_active = (i == state.list_period_idx)
        local tw = screen.get_text_width(label, 11, is_active)
        local tab_w = tw + 12
        if is_active then
            screen.draw_rect(px, period_y + 3, tab_w, 20, {color=theme.accent, filled=true, radius=4})
            screen.draw_text(label, px + 6, period_y + 5, {color={20, 20, 30}, size=11, bold=true})
        else
            screen.draw_rect(px, period_y + 3, tab_w, 20, {color=theme.card_bg, filled=true, radius=4})
            screen.draw_text(label, px + 6, period_y + 5, {color=theme.text_dim, size=11})
        end
        px = px + tab_w + 4
    end
    local hint_str = "L2/R2 period"
    local hw = screen.get_text_width(hint_str, 10, false)
    screen.draw_text(hint_str, 710 - hw, period_y + 8, {color=theme.text_dim, size=10})

    draw_footer({
        {"L1/R1", "Tab", theme.btn_l},
        {"A", "Detail", theme.btn_a},
        {"X", "Refresh", theme.btn_x},
        {"Y", "Browse", theme.btn_y},
    })
end

-- ── Detail Screen Drawing ────────────────────────────────────────────────────

local function draw_detail_screen()
    local q = state.detail_quote
    if not q then
        draw_header("Stock")
        screen.draw_text("No data", 20, 200, {color=theme.text_dim, size=16})
        draw_footer({{"B", "Back", theme.btn_b}})
        return
    end

    local is_positive = q.change >= 0
    local change_color = is_positive and theme.positive or theme.negative
    local sign = is_positive and "+" or ""

    -- Header with gradient
    screen.draw_gradient_rect(0, 0, 720, 70,
        theme.header_gradient_top.r, theme.header_gradient_top.g, theme.header_gradient_top.b,
        theme.header_gradient_bottom.r, theme.header_gradient_bottom.g, theme.header_gradient_bottom.b)
    screen.draw_line(0, 0, 720, 0, {color=theme.accent})
    screen.draw_line(0, 70, 720, 70, {color=theme.border})

    -- Symbol and name
    screen.draw_text(q.symbol, 20, 8, {color=theme.text, size=24, bold=true})
    screen.draw_text(q.name, 20, 38, {color=theme.text_dim, size=14})

    -- Price (top right)
    local price_str = q.price > 0 and string.format("$%.2f", q.price) or "N/A"
    local pw = screen.get_text_width(price_str, 24, true)
    screen.draw_text(price_str, 700 - pw, 8, {color=theme.text, size=24, bold=true})

    -- Change (below price)
    local arrow = is_positive and "\226\150\178" or "\226\150\188"
    local change_str = q.price > 0 and (arrow .. " " .. sign .. string.format("%.2f", q.change) .. " (" .. sign .. string.format("%.1f", q.change_pct) .. "%)") or "---"
    local cw = screen.get_text_width(change_str, 14, false)
    screen.draw_text(change_str, 700 - cw, 40, {color=change_color, size=14})

    -- Period tabs
    local y = 78
    local tab_x = 20
    for i, period in ipairs(PERIODS) do
        local is_active = (i == state.detail_period_idx)
        local label = period[1]
        local tw = screen.get_text_width(label, 12, is_active)
        local tab_w = tw + 16
        if is_active then
            screen.draw_rect(tab_x, y, tab_w, 22, {color=theme.accent, filled=true, radius=4})
            screen.draw_text(label, tab_x + 8, y + 3, {color={20, 20, 30}, size=12, bold=true})
        else
            screen.draw_rect(tab_x, y, tab_w, 22, {color=theme.card_bg, filled=true, radius=4})
            screen.draw_text(label, tab_x + 8, y + 3, {color=theme.text_dim, size=12})
        end
        tab_x = tab_x + tab_w + 6
    end

    -- L1/R1 hint
    local hw = screen.get_text_width("L1/R1 switch", 10, false)
    screen.draw_text("L1/R1 switch", 700 - hw, y + 5, {color=theme.text_dim, size=10})

    -- Chart area
    local chart_y = 108
    local chart_h = 180
    screen.draw_card(8, chart_y, 704, chart_h, {bg=theme.card_bg, border=theme.border, radius=8, shadow=true})

    local spark_data = state.sparklines[q.symbol]
    if spark_data and #spark_data >= 2 then
        screen.draw_sparkline(spark_data, 16, chart_y + 8, 688, chart_h - 16, {color=change_color})
    else
        local tw = screen.get_text_width("No chart data", 13, false)
        screen.draw_text("No chart data", (720 - tw) / 2, chart_y + chart_h / 2 - 8, {color=theme.text_dim, size=13})
    end

    -- Stats cards
    local stats_y = chart_y + chart_h + 12

    -- Day range card
    screen.draw_card(8, stats_y, 344, 60, {bg=theme.card_bg, border=theme.border, radius=6})
    screen.draw_text("Day Range", 18, stats_y + 6, {color=theme.text_dim, size=11})
    local low_str = q.low > 0 and string.format("$%.2f", q.low) or "---"
    local high_str = q.high > 0 and string.format("$%.2f", q.high) or "---"
    screen.draw_text(low_str, 18, stats_y + 22, {color=theme.text, size=12})
    local hsw = screen.get_text_width(high_str, 12, false)
    screen.draw_text(high_str, 342 - hsw, stats_y + 22, {color=theme.text, size=12})
    if q.low > 0 and q.high > q.low and q.price > 0 then
        local progress = math.max(0, math.min(1, (q.price - q.low) / (q.high - q.low)))
        screen.draw_progress_bar(18, stats_y + 42, 324, 6, progress, {})
    end

    -- 52-week range card
    screen.draw_card(368, stats_y, 344, 60, {bg=theme.card_bg, border=theme.border, radius=6})
    screen.draw_text("52-Week Range", 378, stats_y + 6, {color=theme.text_dim, size=11})
    local w52_low = q.week52_low > 0 and string.format("$%.2f", q.week52_low) or "---"
    local w52_high = q.week52_high > 0 and string.format("$%.2f", q.week52_high) or "---"
    screen.draw_text(w52_low, 378, stats_y + 22, {color=theme.text, size=12})
    local w52hw = screen.get_text_width(w52_high, 12, false)
    screen.draw_text(w52_high, 702 - w52hw, stats_y + 22, {color=theme.text, size=12})
    if q.week52_low > 0 and q.week52_high > q.week52_low and q.price > 0 then
        local progress = math.max(0, math.min(1, (q.price - q.week52_low) / (q.week52_high - q.week52_low)))
        screen.draw_progress_bar(378, stats_y + 42, 324, 6, progress, {})
    end

    draw_footer({
        {"B", "Back", theme.btn_b},
        {"L1/R1", "Period", theme.btn_l},
    })
end

-- ── Browse Screen Drawing ────────────────────────────────────────────────────

local function update_browse_stocks()
    local sector = SECTORS[state.browse_sector_idx]
    state.browse_stocks = get_stocks_by_sector(sector)
    state.browse_cursor = math.min(state.browse_cursor, math.max(1, #state.browse_stocks))
end

local function draw_browse_screen()
    draw_header("Browse Stocks",
        #STOCK_UNIVERSE .. " stocks", theme.text_dim)

    -- Sector tabs
    local y = 44
    local tab_x = 10
    for i, sector in ipairs(SECTORS) do
        local label = SECTOR_LABELS[sector] or sector
        local is_active = (i == state.browse_sector_idx)
        local tw = screen.get_text_width(label, 11, is_active)
        local tab_w = tw + 12
        if is_active then
            screen.draw_rect(tab_x, y, tab_w, 20, {color=theme.accent, filled=true, radius=4})
            screen.draw_text(label, tab_x + 6, y + 3, {color={20, 20, 30}, size=11, bold=true})
        else
            screen.draw_rect(tab_x, y, tab_w, 20, {color=theme.card_bg, filled=true, radius=4})
            screen.draw_text(label, tab_x + 6, y + 3, {color=theme.text_dim, size=11})
        end
        tab_x = tab_x + tab_w + 4
    end

    -- Stock list
    local list_y = 70
    local list_h = 614
    local stocks = state.browse_stocks
    local n = #stocks
    local browse_row_h = 44

    if n == 0 then
        local tw = screen.get_text_width("No stocks in this sector", 14, false)
        screen.draw_text("No stocks in this sector", (720 - tw) / 2, list_y + 40, {color=theme.text_dim, size=14})
    else
        local visible = math.max(1, math.floor(list_h / browse_row_h))
        state.browse_cursor = math.max(1, math.min(state.browse_cursor, n))

        local start_idx
        if n <= visible then
            start_idx = 1
        else
            start_idx = math.max(1, math.min(state.browse_cursor - 1, n - visible + 1))
        end
        local end_idx = math.min(start_idx + visible - 1, n)

        local cy = list_y + 2
        for idx = start_idx, end_idx do
            local stock = stocks[idx]
            local is_selected = (idx == state.browse_cursor)
            local in_wl = is_in_watchlist(stock.symbol)

            local card_x = 6
            local card_w = 708
            local card_h = browse_row_h - 4

            if is_selected then
                screen.draw_card(card_x, cy, card_w, card_h, {bg=theme.card_highlight, border=theme.accent, radius=CARD_RADIUS})
            else
                screen.draw_card(card_x, cy, card_w, card_h, {bg=theme.card_bg, radius=CARD_RADIUS})
            end

            -- Symbol
            screen.draw_text(stock.symbol, card_x + 12, cy + 4, {color=theme.text, size=15, bold=true})
            -- Name
            screen.draw_text(stock.name, card_x + 12, cy + 24, {color=theme.text_dim, size=12})

            -- Tier badge
            local tier_color = TIER_COLORS[stock.tier] or {140, 140, 160}
            screen.draw_pill(stock.tier:upper(), card_x + card_w - 140, cy + (card_h - 16) / 2,
                tier_color[1], tier_color[2], tier_color[3], {text_color={20,20,30}, size=10})

            -- Watchlist status
            if in_wl then
                screen.draw_pill("IN LIST", card_x + card_w - 70, cy + (card_h - 16) / 2,
                    theme.positive.r, theme.positive.g, theme.positive.b, {text_color={20,20,30}, size=10})
            end

            cy = cy + browse_row_h
        end

        draw_scroll_indicator(list_y, list_h, state.browse_cursor, n, visible)
    end

    -- Footer
    local hints = {
        {"B", "Back", theme.btn_b},
        {"L1/R1", "Sector", theme.btn_l},
    }
    if n > 0 then
        local stock = stocks[state.browse_cursor]
        if stock then
            if is_in_watchlist(stock.symbol) then
                hints[#hints + 1] = {"X", "Remove", theme.btn_x}
            else
                hints[#hints + 1] = {"A", "Add", theme.btn_a}
            end
        end
    end
    draw_footer(hints)
end

-- ── Lifecycle ────────────────────────────────────────────────────────────────

function on_init()
    state.watchlist = load_watchlist()
    update_browse_stocks()
    state.loading = true
    state.needs_initial_load = true
end

function on_update(dt)
    if state.ready_to_load then
        state.ready_to_load = false
        load_quotes()
    end

    if state.screen_stack[#state.screen_stack] == "watchlist" then
        state.refresh_timer = state.refresh_timer + dt
        if state.refresh_timer >= state.refresh_interval then
            state.refresh_timer = 0
            load_quotes()
        end
    end
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local current = state.screen_stack[#state.screen_stack]

    if current == "watchlist" then
        -- Tab switching
        if button == "l1" and action == "press" then
            state.active_tab = state.active_tab > 1 and (state.active_tab - 1) or #state.tabs
            state.cursor = 1
            if not state.quotes[state.tab_ids[state.active_tab]] then
                load_quotes()
            end
            return
        elseif button == "r1" and action == "press" then
            state.active_tab = state.active_tab < #state.tabs and (state.active_tab + 1) or 1
            state.cursor = 1
            if not state.quotes[state.tab_ids[state.active_tab]] then
                load_quotes()
            end
            return
        end

        local tab_id = state.tab_ids[state.active_tab]
        local quotes = state.quotes[tab_id] or {}
        local n = #quotes

        if button == "dpad_up" then
            state.cursor = math.max(1, state.cursor - 1)
        elseif button == "dpad_down" then
            state.cursor = math.min(math.max(1, n), state.cursor + 1)
        elseif button == "a" and n > 0 and state.cursor <= n then
            state.detail_quote = quotes[state.cursor]
            state.detail_period_idx = 2
            state.screen_stack[#state.screen_stack + 1] = "detail"
        elseif button == "x" then
            state.quotes = {}
            state.sparklines = {}
            load_quotes()
        elseif button == "y" then
            state.screen_stack[#state.screen_stack + 1] = "browse"
        elseif button == "l2" and action == "press" then
            state.list_period_idx = math.max(1, state.list_period_idx - 1)
            state.quotes = {}
            state.sparklines = {}
            load_quotes()
        elseif button == "r2" and action == "press" then
            state.list_period_idx = math.min(#PERIODS, state.list_period_idx + 1)
            state.quotes = {}
            state.sparklines = {}
            load_quotes()
        end

    elseif current == "detail" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
        elseif button == "l1" then
            state.detail_period_idx = math.max(1, state.detail_period_idx - 1)
            -- Refresh chart for new period
            local range = PERIODS[state.detail_period_idx][2]
            local q, closes = fetch_quote_from_chart(state.detail_quote.symbol, range)
            if q then
                state.detail_quote = q
                state.sparklines[q.symbol] = closes
            end
        elseif button == "r1" then
            state.detail_period_idx = math.min(#PERIODS, state.detail_period_idx + 1)
            local range = PERIODS[state.detail_period_idx][2]
            local q, closes = fetch_quote_from_chart(state.detail_quote.symbol, range)
            if q then
                state.detail_quote = q
                state.sparklines[q.symbol] = closes
            end
        end

    elseif current == "browse" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
            -- Refresh watchlist if needed
            state.quotes["watchlist"] = nil
            load_quotes()
        elseif button == "l1" then
            state.browse_sector_idx = state.browse_sector_idx > 1 and (state.browse_sector_idx - 1) or #SECTORS
            state.browse_cursor = 1
            update_browse_stocks()
        elseif button == "r1" then
            state.browse_sector_idx = state.browse_sector_idx < #SECTORS and (state.browse_sector_idx + 1) or 1
            state.browse_cursor = 1
            update_browse_stocks()
        elseif button == "dpad_up" then
            state.browse_cursor = math.max(1, state.browse_cursor - 1)
        elseif button == "dpad_down" then
            state.browse_cursor = math.min(math.max(1, #state.browse_stocks), state.browse_cursor + 1)
        elseif button == "a" and #state.browse_stocks > 0 then
            local stock = state.browse_stocks[state.browse_cursor]
            if stock and not is_in_watchlist(stock.symbol) then
                state.watchlist[#state.watchlist + 1] = {symbol=stock.symbol, name=stock.name}
                save_watchlist()
                state.quotes["watchlist"] = nil
            end
        elseif button == "x" and #state.browse_stocks > 0 then
            local stock = state.browse_stocks[state.browse_cursor]
            if stock and is_in_watchlist(stock.symbol) then
                local new_wl = {}
                for _, w in ipairs(state.watchlist) do
                    if w.symbol ~= stock.symbol then
                        new_wl[#new_wl + 1] = w
                    end
                end
                state.watchlist = new_wl
                save_watchlist()
                state.quotes["watchlist"] = nil
            end
        end
    end
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    -- After first frame renders, allow on_update to fetch data
    if state.needs_initial_load then
        state.needs_initial_load = false
        state.ready_to_load = true
    end

    local current = state.screen_stack[#state.screen_stack]
    if current == "watchlist" then
        draw_watchlist_screen()
    elseif current == "detail" then
        draw_detail_screen()
    elseif current == "browse" then
        draw_browse_screen()
    end
end
