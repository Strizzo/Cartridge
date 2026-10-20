-- Stock Market / quote snapshots, price history and watchlist catalog.
local U = ui.public
local C = U.C

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
local PERIODS = {{"1D","1d"}, {"1W","5d"}, {"1M","1mo"}, {"3M","3mo"}, {"1Y","1y"}}
local PERIOD_INTERVALS = {["1d"]="5m", ["5d"]="15m", ["1mo"]="1d", ["3mo"]="1d", ["1y"]="1wk"}

local state = {
    screen_stack = {"watchlist"},
    tabs = {"Watchlist", "Indices", "Crypto"}, tab_ids = {"watchlist", "indices", "crypto"}, active_tab = 1,
    watchlist = {}, watchlist_revision = 0, quotes = {}, quote_errors = {}, quote_periods = {},
    cursor = 1, list_period_idx = 2, loading = false, loading_tab = nil, loading_period = nil, loading_revision = nil,
    refresh_timer = 0, refresh_interval = 120.0,
    detail_quote = nil, detail_history = nil, detail_period_idx = 2, detail_loading = false, detail_error = "",
    browse_sector_idx = 1, browse_cursor = 1, browse_stocks = {},
    pending = {}, quotes_gen = 0, detail_gen = 0,
}
local ROW_HEIGHT, LIST_VISIBLE = 84, 5

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
    state.watchlist_revision = state.watchlist_revision + 1
    -- Retained snapshots must also respect removals while a refresh is pending.
    if state.quotes.watchlist then
        local retained = {}
        for _, quote in ipairs(state.quotes.watchlist) do
            if is_in_watchlist(quote.symbol) then retained[#retained + 1] = quote end
        end
        state.quotes.watchlist = retained
    end
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

local function chart_url(symbol, range)
    local interval = PERIOD_INTERVALS[range] or "1d"
    return YAHOO_BASE .. symbol .. "?range=" .. range .. "&interval=" .. interval .. "&events="
end

-- Async plumbing: every call goes through http.get_async; on_update drains
-- http.poll() and dispatches to the handler registered here.
local function request(url, handler)
    local ok, id = pcall(http.get_async, url)
    if ok and id then
        state.pending[id] = handler
        return id
    end
    handler(nil)
    return nil
end

local function chart_from_resp(resp)
    if not resp or not resp.ok then return nil end
    local dok, data = pcall(json.decode, resp.body)
    if not dok or type(data) ~= "table" or type(data.chart) ~= "table" then return nil end
    local result = data.chart.result
    if type(result) ~= "table" or type(result[1]) ~= "table" then return nil end
    return result[1]
end

local function quote_from_chart(symbol, chart)
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
        has_week52 = meta.fiftyTwoWeekHigh ~= nil and meta.fiftyTwoWeekLow ~= nil,
        quote_time = meta.regularMarketTime,
        received_at = os.time(),
        currency = meta.currency or "USD",
    }
    return quote, closes
end

local function load_quotes()
    local tab_id = state.tab_ids[state.active_tab]
    local revision = tab_id == "watchlist" and state.watchlist_revision or nil
    if state.loading and state.loading_tab == tab_id and state.loading_period == state.list_period_idx
        and state.loading_revision == revision then return end
    state.loading = true
    state.refresh_timer = 0
    state.loading_period = state.list_period_idx
    state.loading_tab = tab_id
    state.loading_revision = revision
    state.quote_errors[tab_id] = ""
    local symbols = {}
    local source = tab_id == "watchlist" and state.watchlist or (tab_id == "crypto" and DEFAULT_CRYPTO or DEFAULT_INDICES)
    for _, item in ipairs(source) do symbols[#symbols + 1] = item.symbol end
    local range = PERIODS[state.list_period_idx][2]
    state.quotes_gen = state.quotes_gen + 1
    local gen, n = state.quotes_gen, #symbols
    if n == 0 then
        state.quotes[tab_id] = {}
        state.quote_periods[tab_id] = PERIODS[state.list_period_idx][2]
        state.loading = false
        return
    end
    local previous = {}
    for _, quote in ipairs(state.quotes[tab_id] or {}) do previous[quote.symbol] = quote end
    local results, completed, failed = {}, 0, 0
    for i, sym in ipairs(symbols) do
        request(chart_url(sym, range), function(resp)
            if gen ~= state.quotes_gen or (tab_id == "watchlist" and revision ~= state.watchlist_revision) then return end
            local chart = chart_from_resp(resp)
            if chart then
                local quote, closes = quote_from_chart(sym, chart)
                quote.history_range = range
                quote.history = closes
                results[i] = quote
            else
                failed = failed + 1
                if previous[sym] then
                    local cached = {}
                    for key, value in pairs(previous[sym]) do cached[key] = value end
                    cached.stale = true
                    results[i] = cached
                end
            end
            completed = completed + 1
            if completed >= n then
                local quotes = {}
                for k = 1, n do if results[k] then quotes[#quotes + 1] = results[k] end end
                -- Publish quotes and their matching histories as one snapshot batch.
                -- Failed rows retain the original range and series together.
                state.quotes[tab_id] = quotes
                state.quote_periods[tab_id] = range
                state.quote_errors[tab_id] = failed > 0 and (failed .. " quote requests failed") or ""
                state.loading = false
            end
        end)
    end
end

local function load_detail_chart(symbol, range)
    state.detail_loading = true
    state.detail_history = nil
    state.detail_error = ""
    state.detail_gen = state.detail_gen + 1
    local gen = state.detail_gen
    request(chart_url(symbol, range), function(resp)
        if gen ~= state.detail_gen then return end
        local chart = chart_from_resp(resp)
        if chart then
            local quote, closes = quote_from_chart(symbol, chart)
            quote.history_range = range
            quote.history = closes
            state.detail_quote, state.detail_history = quote, closes
            if #closes < 2 then state.detail_error = "Yahoo returned too few price samples." end
        else
            state.detail_error = "The Yahoo price-history request failed."
        end
        state.detail_loading = false
    end)
end

local function period_label(range)
    for _, period in ipairs(PERIODS) do if period[2] == range then return period[1] end end
    return "--"
end

local function price_text(quote, value)
    value = value or quote.price
    if value <= 0 then return "N/A" end
    local prefix = quote.symbol:sub(1, 1) == "^" and "" or (quote.currency == "USD" and "$" or (quote.currency .. " "))
    if value < 0.01 then return prefix .. string.format("%.6f", value) end
    if value < 1 then return prefix .. string.format("%.4f", value) end
    return prefix .. string.format("%.2f", value)
end

local function change_text(quote)
    if quote.price <= 0 then return "Change unavailable" end
    local sign = quote.change >= 0 and "+" or ""
    return sign .. string.format("%.2f", quote.change) .. " / " .. sign .. string.format("%.2f%%", quote.change_pct)
end

local function draw_quote_row(quote, y, selected)
    local fg, secondary = U.row(24, y, 672, ROW_HEIGHT - 6, selected)
    local change_color = quote.change >= 0 and C.green or C.red
    U.display(quote.symbol:gsub("%-USD$", ""), 42, y + 6, 28, fg, 204)
    U.text((quote.stale and "Cached / " or "") .. quote.name, 42, y + 48, 14, secondary, 204)
    local history = quote.history
    if history and #history >= 2 then
        for gy = y + 12, y + 40, 14 do U.rule(266, gy, 144, selected and C.quiet or C.line) end
        U.sparkline(history, 266, y + 12, 144, 31, selected and fg or change_color)
    else
        U.text("No history", 266, y + 22, 14, secondary, 144)
    end
    U.text(period_label(quote.history_range) .. (quote.stale and " / CACHED" or " / HISTORY"), 266, y + 50, 14, secondary, 150)
    U.display(price_text(quote), 438, y + 6, 30, fg, 238)
    U.text(change_text(quote), 438, y + 48, 16, (quote.price > 0 and not selected) and change_color or fg, 238, true)
end

local function draw_watchlist_screen()
    local tab_id = state.tab_ids[state.active_tab]
    local quotes = state.quotes[tab_id] or {}
    local n = #quotes
    local loading = state.loading and state.loading_tab == tab_id
    local error_msg = state.quote_errors[tab_id] or ""
    U.header("Stock Market", "YAHOO FINANCE / QUOTE SNAPSHOTS", loading and "Refreshing" or (error_msg ~= "" and (n > 0 and "Partial / stale" or "Unavailable") or "Snapshots"), error_msg ~= "" and C.gold or C.muted)
    U.tabs(state.tabs, state.active_tab)
    local labels = {}
    for i, period in ipairs(PERIODS) do labels[i] = period[1] end
    U.tabs(labels, state.list_period_idx, 156)
    U.text(error_msg ~= "" and "INCOMPLETE / X RETRY / CACHED VALUES MARKED" or (loading and "REFRESHING / PREVIOUS SNAPSHOTS RETAINED" or "YAHOO / DATA MAY BE DELAYED"), 24, 198, 14, error_msg ~= "" and C.gold or C.muted, 558)
    if n > 0 then
        state.cursor = math.max(1, math.min(state.cursor, n))
        U.text(state.cursor .. " / " .. n, 612, 198, 14, C.muted, 84)
        local start = math.max(1, math.min(state.cursor - 1, math.max(1, n - LIST_VISIBLE + 1)))
        for idx = start, math.min(n, start + LIST_VISIBLE - 1) do
            draw_quote_row(quotes[idx], 218 + (idx - start) * ROW_HEIGHT, idx == state.cursor)
        end
    elseif loading then
        U.empty("Fetching quote snapshots", "Requesting Yahoo prices and history for this list. Quotes may be delayed.")
    elseif error_msg ~= "" then
        U.empty("Quotes unavailable", error_msg .. ". Press X to retry, or Y to browse the watchlist catalog.")
    elseif tab_id == "watchlist" and #state.watchlist == 0 then
        U.empty("Your watchlist is empty", "Press Y to browse sectors, then A to add a symbol. Your list is saved on this device.")
    else
        U.empty("No quotes returned", "Press X to retry these symbols, or L1 / R1 to switch lists.")
    end
    U.footer({{"L1/R1", "List"}, {"L2/R2", "Range"}, {"A", "Quote"}, {"X", error_msg ~= "" and "Retry" or "Refresh"}, {"Y", "Browse"}})
end

local function draw_range_panel(x, y, w, quote, label, low, high, available)
    U.panel(x, y, w, 82, C.panel)
    U.text(label, x + 16, y + 10, 14, C.muted, w - 32, true)
    U.text(available and (price_text(quote, low) .. "  /  " .. price_text(quote, high)) or "Not supplied", x + 16, y + 35, 18, C.paper, w - 32, true)
    U.rect(x + 16, y + 67, w - 32, 3, C.line)
    if available and high > low and quote.price > 0 then
        local progress = math.max(0, math.min(1, (quote.price - low) / (high - low)))
        U.rect(x + 16 + (w - 36) * progress, y + 63, 4, 11, C.red)
    end
end

local function draw_detail_screen()
    local quote = state.detail_quote
    if not quote then
        U.header("Quote", "STOCK MARKET / DETAIL", "Unavailable", C.gold)
        U.empty("No quote available", "Press B to return to the list, then X to refresh quotes.")
        U.footer({{"B", "Back"}})
        return
    end
    local change_color = quote.change >= 0 and C.green or C.red
    U.header(quote.symbol:gsub("%-USD$", ""), quote.name .. " / YAHOO FINANCE", state.detail_loading and "Refreshing" or ((quote.stale or state.detail_error ~= "") and "Cached quote" or "Snapshot"), (quote.stale or state.detail_error ~= "") and C.gold or C.muted)
    U.panel(24, 112, 672, 108, C.paper, C.red)
    U.text(quote.symbol:sub(1, 1) == "^" and "LAST QUOTE / INDEX POINTS" or ("LAST QUOTE / " .. quote.currency), 42, 124, 14, C.black, 358, true)
    U.display(price_text(quote), 42, 146, 44, C.black, 370)
    U.text("CHANGE / " .. period_label(quote.history_range), 438, 124, 14, C.black, 238, true)
    U.text(change_text(quote), 438, 151, 18, C.black, 238, true)
    local stamp = type(quote.quote_time) == "number" and quote.quote_time or quote.received_at
    U.text((type(quote.quote_time) == "number" and "As of " or "Fetched ") .. os.date("%d %b %H:%M", stamp), 42, 197, 14, C.black, 356)
    U.text("May be delayed", 438, 197, 14, C.black, 238)
    local labels = {}
    for i, period in ipairs(PERIODS) do labels[i] = period[1] end
    U.tabs(labels, state.detail_period_idx, 232)
    U.panel(24, 280, 672, 226, C.panel)
    U.text("PRICE HISTORY / " .. PERIODS[state.detail_period_idx][1], 42, 292, 14, C.muted, 620, true)
    local history = state.detail_history
    if state.detail_loading then
        U.display("Fetching history", 42, 352, 32, C.paper, 620)
        U.text("Requesting the selected range from Yahoo.", 42, 405, 16, C.muted, 620)
    elseif state.detail_error ~= "" or not history or #history < 2 then
        U.display("History unavailable", 42, 352, 32, C.paper, 620)
        U.text((state.detail_error ~= "" and state.detail_error or "Too few price samples were returned.") .. " X to retry.", 42, 405, 16, C.muted, 620)
    else
        for gy = 326, 460, 45 do U.rule(42, gy, 636, C.line) end
        U.sparkline(history, 42, 326, 636, 134, change_color)
        local low, high = history[1], history[1]
        for _, value in ipairs(history) do low, high = math.min(low, value), math.max(high, value) end
        U.text("Low " .. price_text(quote, low), 42, 476, 14, C.muted, 310)
        U.text("High " .. price_text(quote, high), 438, 476, 14, C.muted, 238)
    end
    draw_range_panel(24, 518, 328, quote, period_label(quote.history_range) .. " RANGE / LOW & HIGH", quote.low, quote.high, quote.low > 0 and quote.high > 0)
    draw_range_panel(368, 518, 328, quote, "52-WEEK RANGE / LOW & HIGH", quote.week52_low, quote.week52_high, quote.has_week52)
    U.text("Price and change are snapshots. History follows the selected range.", 24, 616, 14, C.muted, 672)
    U.footer({{"B", "Back"}, {"L1/R1", "Range"}, {"X", "Retry"}})
end

local function update_browse_stocks()
    local sector = SECTORS[state.browse_sector_idx]
    state.browse_stocks = get_stocks_by_sector(sector)
    state.browse_cursor = math.min(state.browse_cursor, math.max(1, #state.browse_stocks))
end

local function draw_browse_screen()
    local stocks = state.browse_stocks
    local n = #stocks
    local sector = SECTORS[state.browse_sector_idx]
    U.header("Browse Stocks", "WATCHLIST / SYMBOL CATALOG", #STOCK_UNIVERSE .. " symbols")
    U.panel(24, 112, 672, 72, C.panel, C.red)
    U.display(sector, 42, 120, 34, C.paper, 410)
    U.text(n .. " symbols / " .. state.browse_sector_idx .. " of " .. #SECTORS, 482, 133, 14, C.muted, 196)
    for i = 1, #SECTORS do U.rect(42 + (i - 1) * 80, 174, 72, 3, i == state.browse_sector_idx and C.red or C.line) end
    U.text("SYMBOL / COMPANY", 24, 198, 14, C.muted, 400, true)
    U.text("TIER / WATCHLIST", 482, 198, 14, C.muted, 214, true)
    if n == 0 then
        U.empty("No symbols in this sector", "Press L1 / R1 for another sector, or B to return to your quotes.")
    else
        state.browse_cursor = math.max(1, math.min(state.browse_cursor, n))
        local start = math.max(1, math.min(state.browse_cursor - 1, math.max(1, n - LIST_VISIBLE + 1)))
        for idx = start, math.min(n, start + LIST_VISIBLE - 1) do
            local stock = stocks[idx]
            local y = 218 + (idx - start) * ROW_HEIGHT
            local fg, secondary = U.row(24, y, 672, ROW_HEIGHT - 6, idx == state.browse_cursor)
            U.display(stock.symbol, 42, y + 7, 28, fg, 410)
            U.text(stock.name, 42, y + 48, 16, secondary, 410)
            U.text(stock.tier:upper() .. " CAP", 482, y + 13, 14, secondary, 196, true)
            U.text(is_in_watchlist(stock.symbol) and "IN WATCHLIST" or "A TO ADD", 482, y + 48, 14, is_in_watchlist(stock.symbol) and (idx == state.browse_cursor and fg or C.green) or secondary, 196, true)
        end
    end
    local hints = {{"B", "Back"}, {"L1/R1", "Sector"}, {"D-pad", "Move"}}
    local stock = stocks[state.browse_cursor]
    if stock then hints[#hints + 1] = is_in_watchlist(stock.symbol) and {"X", "Remove"} or {"A", "Add"} end
    U.footer(hints)
end

local function visual_key()
    local tab = state.tab_ids[state.active_tab]
    return table.concat({state.screen_stack[#state.screen_stack], state.active_tab, tostring(state.quotes[tab]),
        state.cursor, state.list_period_idx, tostring(state.loading and state.loading_tab == tab), state.quote_errors[tab] or "",
        tostring(state.detail_quote), tostring(state.detail_history), state.detail_period_idx,
        tostring(state.detail_loading), state.detail_error, state.browse_sector_idx, state.browse_cursor,
        state.watchlist_revision}, "\0")
end

function on_init()
    state.watchlist = load_watchlist()
    update_browse_stocks()
    load_quotes()
    app.request_redraw()
end

function on_update(dt)
    local before = visual_key()
    for _, resp in ipairs(http.poll()) do
        local handler = state.pending[resp.id]
        if handler then state.pending[resp.id] = nil; handler(resp) end
    end
    if state.screen_stack[#state.screen_stack] == "watchlist" then
        state.refresh_timer = state.refresh_timer + dt
        if state.refresh_timer >= state.refresh_interval then
            state.refresh_timer = 0
            if not state.loading and (state.tab_ids[state.active_tab] ~= "watchlist" or #state.watchlist > 0) then load_quotes() end
        end
    end
    local dirty = before ~= visual_key()
    if dirty then app.request_redraw() end
    return dirty
end

local function handle_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local current = state.screen_stack[#state.screen_stack]

    if current == "watchlist" then
        -- Tab switching
        if button == "l1" and action == "press" then
            state.active_tab = state.active_tab > 1 and (state.active_tab - 1) or #state.tabs
            state.cursor = 1
            if not state.quotes[state.tab_ids[state.active_tab]] or state.quote_periods[state.tab_ids[state.active_tab]] ~= PERIODS[state.list_period_idx][2] then
                load_quotes()
            end
            return
        elseif button == "r1" and action == "press" then
            state.active_tab = state.active_tab < #state.tabs and (state.active_tab + 1) or 1
            state.cursor = 1
            if not state.quotes[state.tab_ids[state.active_tab]] or state.quote_periods[state.tab_ids[state.active_tab]] ~= PERIODS[state.list_period_idx][2] then
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
            state.detail_period_idx = state.list_period_idx
            for i, period in ipairs(PERIODS) do
                if period[2] == state.detail_quote.history_range then state.detail_period_idx = i end
            end
            state.detail_history = state.detail_quote.history
            state.detail_error = ""
            state.detail_loading = false
            state.screen_stack[#state.screen_stack + 1] = "detail"
        elseif button == "x" then
            load_quotes()
        elseif button == "y" then
            state.screen_stack[#state.screen_stack + 1] = "browse"
        elseif button == "l2" and action == "press" then
            if state.list_period_idx > 1 then
                state.list_period_idx = state.list_period_idx - 1
                load_quotes()
            end
        elseif button == "r2" and action == "press" then
            if state.list_period_idx < #PERIODS then
                state.list_period_idx = state.list_period_idx + 1
                load_quotes()
            end
        end

    elseif current == "detail" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
            state.detail_gen = state.detail_gen + 1  -- drop an in-flight chart fetch
            state.detail_loading = false
        elseif button == "l1" then
            if state.detail_period_idx > 1 then
                state.detail_period_idx = state.detail_period_idx - 1
                load_detail_chart(state.detail_quote.symbol, PERIODS[state.detail_period_idx][2])
            end
        elseif button == "r1" then
            if state.detail_period_idx < #PERIODS then
                state.detail_period_idx = state.detail_period_idx + 1
                load_detail_chart(state.detail_quote.symbol, PERIODS[state.detail_period_idx][2])
            end
        elseif button == "x" and not state.detail_loading then
            load_detail_chart(state.detail_quote.symbol, PERIODS[state.detail_period_idx][2])
        end

    elseif current == "browse" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
            -- Refresh watchlist if needed
            state.quote_periods["watchlist"] = nil
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
                state.quote_periods["watchlist"] = nil
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
                state.quote_periods["watchlist"] = nil
            end
        end
    end
end

function on_input(button, action)
    local before = visual_key()
    handle_input(button, action)
    if before ~= visual_key() then app.request_redraw() end
end

function on_render()
    local current = state.screen_stack[#state.screen_stack]
    if current == "watchlist" then draw_watchlist_screen()
    elseif current == "detail" then draw_detail_screen()
    elseif current == "browse" then draw_browse_screen() end
end
