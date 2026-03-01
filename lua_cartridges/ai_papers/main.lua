-- AI Papers - Browse top AI research papers from HuggingFace Daily Papers
-- with arxiv paper reader

local API_URL = "https://huggingface.co/api/daily_papers"
local AR5IV_BASE = "https://ar5iv.labs.arxiv.org/html/"

-- ── State ────────────────────────────────────────────────────────────────────

local state = {
    screen_stack = {"list"},
    papers = {},
    cursor = 1,
    loading = false,
    error_msg = "",
    -- Detail state
    detail_paper = nil,
    detail_lines = {},
    detail_scroll = 0,
    detail_needs_layout = false,
    -- Reader state
    reader_lines = {},
    reader_scroll = 0,
    reader_loading = false,
    reader_raw_body = nil,
    reader_needs_layout = false,
    reader_url_to_load = nil,
    -- Deferred initial load
    needs_initial_load = true,
    ready_to_load = false,
}

local ROW_HEIGHT = 72
local CARD_RADIUS = 6

-- ── Utility Functions ────────────────────────────────────────────────────────

local function time_ago(iso_date)
    if not iso_date or iso_date == "" then return "" end
    -- Parse ISO date: "2025-03-01T12:00:00.000Z"
    local y, m, d = iso_date:match("(%d+)-(%d+)-(%d+)")
    if not y then return "" end
    local ts = os.time({year=tonumber(y), month=tonumber(m), day=tonumber(d)})
    local diff = os.time() - ts
    if diff < 0 then return "today" end
    if diff < 86400 then return "today" end
    if diff < 172800 then return "yesterday" end
    return math.floor(diff / 86400) .. "d ago"
end

local function word_wrap(text, max_width, font_size, bold)
    if not text or text == "" then return {""} end
    local words = {}
    for w in text:gmatch("%S+") do
        words[#words + 1] = w
    end
    if #words == 0 then return {""} end

    local lines = {}
    local current = ""
    for _, word in ipairs(words) do
        local test = current == "" and word or (current .. " " .. word)
        local tw = screen.get_text_width(test, font_size, bold or false)
        if tw <= max_width then
            current = test
        else
            if current ~= "" then
                lines[#lines + 1] = current
            end
            if screen.get_text_width(word, font_size, bold or false) > max_width then
                local chunk = ""
                for i = 1, #word do
                    local c = word:sub(i, i)
                    if screen.get_text_width(chunk .. c, font_size, bold or false) > max_width then
                        lines[#lines + 1] = chunk
                        chunk = c
                    else
                        chunk = chunk .. c
                    end
                end
                current = chunk
            else
                current = word
            end
        end
    end
    if current ~= "" then
        lines[#lines + 1] = current
    end
    if #lines == 0 then return {""} end
    return lines
end

local function strip_html_body(body)
    body = body:gsub("<script[^>]*>.-</script>", "")
    body = body:gsub("<style[^>]*>.-</style>", "")
    body = body:gsub("<br%s*/?>", "\n")
    body = body:gsub("<p[^>]*>", "\n\n")
    body = body:gsub("<h[1-6][^>]*>(.-)</h[1-6]>", "\n\n[%1]\n")
    body = body:gsub("<li[^>]*>", "\n  - ")
    -- Convert math elements to readable text
    body = body:gsub("<math[^>]*>(.-)</math>", " [math] ")
    body = body:gsub("<annotation[^>]*>(.-)</annotation>", " %1 ")
    -- Table handling
    body = body:gsub("<tr[^>]*>", "\n")
    body = body:gsub("<td[^>]*>(.-)</td>", " %1 |")
    body = body:gsub("<th[^>]*>(.-)</th>", " %1 |")
    -- Strip remaining tags
    body = body:gsub("<[^>]+>", "")
    -- Decode entities
    body = body:gsub("&amp;", "&")
    body = body:gsub("&lt;", "<")
    body = body:gsub("&gt;", ">")
    body = body:gsub("&quot;", '"')
    body = body:gsub("&#39;", "'")
    body = body:gsub("&nbsp;", " ")
    body = body:gsub("&#x27;", "'")
    body = body:gsub("&#(%d+);", function(n) return string.char(tonumber(n)) end)
    -- Clean up whitespace
    body = body:gsub("\r\n", "\n")
    body = body:gsub("\n%s*\n%s*\n+", "\n\n")
    body = body:gsub("  +", " ")
    return body:match("^%s*(.-)%s*$") or ""
end

-- ── Drawing Helpers ──────────────────────────────────────────────────────────

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

local function draw_loading(msg, y, h)
    local text = msg or "Loading..."
    local tw = screen.get_text_width(text, 16, false)
    screen.draw_text(text, (720 - tw) / 2, y + h / 2 - 8, {color=theme.text_dim, size=16})
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

-- ── API Functions ────────────────────────────────────────────────────────────

local function fetch_papers()
    state.loading = true
    state.error_msg = ""

    local ok, resp = pcall(http.get_cached, API_URL .. "?limit=30", 300)
    if ok and resp.ok then
        local dok, data = pcall(json.decode, resp.body)
        if dok and data then
            state.papers = {}
            for _, entry in ipairs(data) do
                local paper = entry.paper or entry
                local authors = {}
                if paper.authors then
                    for j, a in ipairs(paper.authors) do
                        if j <= 3 then
                            authors[#authors + 1] = a.name or (a.user and a.user.fullname) or "Unknown"
                        end
                    end
                    if #paper.authors > 3 then
                        authors[#authors + 1] = "+" .. (#paper.authors - 3) .. " more"
                    end
                end

                state.papers[#state.papers + 1] = {
                    id = paper.id or "",
                    title = paper.title or "Untitled",
                    authors = authors,
                    authors_str = table.concat(authors, ", "),
                    summary = paper.summary or paper.ai_summary or "",
                    ai_summary = paper.ai_summary or "",
                    upvotes = paper.upvotes or entry.upvotes or 0,
                    published = paper.publishedAt or entry.publishedAt or "",
                    keywords = paper.ai_keywords or {},
                    num_comments = entry.numComments or 0,
                }
            end
            -- Sort by upvotes descending
            table.sort(state.papers, function(a, b) return a.upvotes > b.upvotes end)
        else
            state.error_msg = "Failed to parse papers"
        end
    else
        state.error_msg = "Failed to load papers"
    end
    state.loading = false
end

-- ── Paper List Screen ────────────────────────────────────────────────────────

local function draw_paper_card(paper, y, is_selected, index)
    local card_x = 6
    local card_w = 708
    local card_h = ROW_HEIGHT - 4

    if is_selected then
        screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_highlight, border=theme.accent, radius=CARD_RADIUS})
    else
        screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_bg, radius=CARD_RADIUS})
    end

    -- Upvote count + rank
    local score_x = card_x + 6
    local score_w = 42
    local intensity = math.min(1.0, paper.upvotes / 100)
    local score_color = {
        math.floor(140 + 115 * intensity),
        math.floor(140 - 40 * intensity),
        math.floor(140 - 100 * intensity),
    }

    local score_str = tostring(paper.upvotes)
    local sw = screen.get_text_width(score_str, 15, true)
    screen.draw_text(score_str, score_x + (score_w - sw) / 2, y + 6, {color=score_color, size=15, bold=true})

    -- Rank number
    local rank_str = "#" .. index
    local rw = screen.get_text_width(rank_str, 10, false)
    screen.draw_text(rank_str, score_x + (score_w - rw) / 2, y + 24, {color=theme.text_dim, size=10})

    -- Arrow
    local aw = screen.get_text_width("\226\150\178", 9, false)
    screen.draw_text("\226\150\178", score_x + (score_w - aw) / 2, y + 38, {color=score_color, size=9})

    -- Title (truncated to 2 lines)
    local title_x = card_x + score_w + 12
    local title_max_w = card_w - score_w - 24

    local title = paper.title
    local tw = screen.get_text_width(title, 13, is_selected)
    if tw > title_max_w then
        while #title > 1 and screen.get_text_width(title .. "..", 13, is_selected) > title_max_w do
            title = title:sub(1, -2)
        end
        title = title .. ".."
    end
    screen.draw_text(title, title_x, y + 4, {color=theme.text, size=13, bold=is_selected})

    -- Authors
    local authors = paper.authors_str
    local auth_max_w = title_max_w - 80
    local auth_tw = screen.get_text_width(authors, 11, false)
    if auth_tw > auth_max_w then
        while #authors > 1 and screen.get_text_width(authors .. "..", 11, false) > auth_max_w do
            authors = authors:sub(1, -2)
        end
        authors = authors .. ".."
    end
    screen.draw_text(authors, title_x, y + 22, {color=theme.text_dim, size=11})

    -- Date + arxiv ID
    local meta = paper.id
    if paper.published ~= "" then
        meta = time_ago(paper.published) .. " \194\183 " .. paper.id
    end
    screen.draw_text(meta, title_x, y + 38, {color=theme.text_dim, size=10})

    -- Keywords pills (up to 2)
    local kx = title_x + screen.get_text_width(meta, 10, false) + 8
    for ki = 1, math.min(2, #paper.keywords) do
        if kx + 60 > card_x + card_w - 8 then break end
        local pw = screen.draw_pill(paper.keywords[ki], kx, y + 36,
            40, 40, 60, {text_color=theme.text_dim, size=9})
        kx = kx + pw + 4
    end

    -- Comment count
    if paper.num_comments > 0 then
        local cstr = tostring(paper.num_comments)
        screen.draw_pill(cstr, card_x + card_w - 42, y + (card_h - 18) / 2,
            60, 60, 80, {text_color=theme.text_dim, size=11})
    end
end

local function draw_paper_list()
    local n = #state.papers

    draw_header("AI Papers",
        n > 0 and ("HuggingFace Daily \194\183 " .. n) or nil,
        n > 0 and {180, 100, 255} or nil)

    local content_y = 42
    local footer_y = 684
    local content_h = footer_y - content_y

    if state.loading then
        draw_loading("Loading papers from HuggingFace...", content_y, content_h)
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
            draw_paper_card(state.papers[idx], y, idx == state.cursor, idx)
            y = y + ROW_HEIGHT
        end

        draw_scroll_indicator(content_y, content_h, state.cursor, n, visible)
    elseif state.error_msg ~= "" then
        local ew = screen.get_text_width(state.error_msg, 14, false)
        screen.draw_text(state.error_msg, (720 - ew) / 2, content_y + content_h / 2 - 8, {color=theme.negative, size=14})
    end

    draw_footer({
        {"A", "Details", theme.btn_a},
        {"X", "Refresh", theme.btn_x},
        {"\226\134\145\226\134\147", "Navigate", theme.btn_l},
    })
end

-- ── Paper Detail Screen ──────────────────────────────────────────────────────

local function layout_detail()
    state.detail_lines = {}
    local lines = state.detail_lines
    local paper = state.detail_paper
    if not paper then return end

    local max_w = 680

    -- Title
    local title_lines = word_wrap(paper.title, max_w, 16, true)
    for _, line in ipairs(title_lines) do
        lines[#lines + 1] = {text=line, color=theme.text, size=16, bold=true}
    end
    lines[#lines + 1] = {text="", color=theme.text, size=14}

    -- Authors
    local auth_lines = word_wrap(paper.authors_str, max_w, 13, false)
    for _, line in ipairs(auth_lines) do
        lines[#lines + 1] = {text=line, color=theme.text_accent, size=13}
    end

    -- Meta
    local meta = paper.upvotes .. " upvotes"
    if paper.published ~= "" then
        meta = meta .. " \194\183 " .. time_ago(paper.published)
    end
    meta = meta .. " \194\183 arxiv:" .. paper.id
    lines[#lines + 1] = {text=meta, color=theme.text_dim, size=12}
    lines[#lines + 1] = {text="", color=theme.text, size=14}

    -- Keywords
    if #paper.keywords > 0 then
        local kw_str = "Keywords: " .. table.concat(paper.keywords, ", ")
        local kw_lines = word_wrap(kw_str, max_w, 12, false)
        for _, line in ipairs(kw_lines) do
            lines[#lines + 1] = {text=line, color={180, 100, 255}, size=12}
        end
        lines[#lines + 1] = {text="", color=theme.text, size=14}
    end

    -- AI Summary (if available and different from full summary)
    if paper.ai_summary and paper.ai_summary ~= "" and paper.ai_summary ~= paper.summary then
        lines[#lines + 1] = {text="AI Summary", color=theme.accent, size=14, bold=true}
        local ai_lines = word_wrap(paper.ai_summary, max_w, 13, false)
        for _, line in ipairs(ai_lines) do
            lines[#lines + 1] = {text=line, color=theme.text, size=13}
        end
        lines[#lines + 1] = {text="", color=theme.text, size=14}
    end

    -- Abstract separator
    local sep = string.rep("\226\148\128", 30) .. " Abstract " .. string.rep("\226\148\128", 15)
    lines[#lines + 1] = {text=sep, color=theme.text_dim, size=12}
    lines[#lines + 1] = {text="", color=theme.text, size=14}

    -- Full abstract
    if paper.summary and paper.summary ~= "" then
        for paragraph in paper.summary:gmatch("[^\n]+") do
            local wrapped = word_wrap(paragraph, max_w, 14, false)
            for _, wl in ipairs(wrapped) do
                lines[#lines + 1] = {text=wl, color=theme.text, size=14}
            end
            lines[#lines + 1] = {text="", color=theme.text, size=14}
        end
    else
        lines[#lines + 1] = {text="No abstract available", color=theme.text_dim, size=14}
    end
end

local function draw_paper_detail()
    if state.detail_needs_layout then
        layout_detail()
        state.detail_needs_layout = false
    end

    draw_header("Paper Details", "arxiv:" .. (state.detail_paper and state.detail_paper.id or ""), {180, 100, 255})

    local content_y = 42
    local content_h = 642
    local lh = screen.get_line_height(14, false)

    local lines = state.detail_lines
    local total = #lines
    local visible_lines = math.max(1, math.floor(content_h / lh))
    local max_scroll = math.max(0, total - visible_lines)
    state.detail_scroll = math.min(state.detail_scroll, max_scroll)

    local y = content_y + 6
    for i = state.detail_scroll + 1, math.min(state.detail_scroll + visible_lines, total) do
        local line = lines[i]
        screen.draw_text(line.text, 20, y, {
            color=line.color,
            size=line.size or 14,
            bold=line.bold or false,
        })
        y = y + lh
    end

    -- Scroll indicator
    if total > visible_lines then
        local ind_x = 715
        local bar_top = content_y + 6
        local bar_h = content_h - 12
        screen.draw_line(ind_x, bar_top, ind_x, bar_top + bar_h, {color=theme.border})
        local thumb_h = math.max(8, math.floor(bar_h * visible_lines / total))
        local progress = max_scroll > 0 and (state.detail_scroll / max_scroll) or 0
        local thumb_y = bar_top + math.floor((bar_h - thumb_h) * progress)
        screen.draw_rect(ind_x - 1, thumb_y, 3, thumb_h, {color=theme.text_dim, filled=true, radius=1})
    end

    draw_footer({
        {"B", "Back", theme.btn_b},
        {"X", "Read Paper", theme.btn_x},
        {"\226\134\145\226\134\147", "Scroll", theme.btn_l},
        {"L2/R2", "Page", theme.btn_l},
    })
end

-- ── Reader Screen ────────────────────────────────────────────────────────────

local function layout_reader()
    state.reader_lines = {}
    local body = state.reader_raw_body or ""
    state.reader_raw_body = nil
    local max_w = 680
    for paragraph in body:gmatch("[^\n]+") do
        local trimmed = paragraph:match("^%s*(.-)%s*$") or ""
        if trimmed ~= "" then
            -- Detect section headers (lines in brackets from our h1-h6 conversion)
            if trimmed:match("^%[.+%]$") then
                state.reader_lines[#state.reader_lines + 1] = ""
                state.reader_lines[#state.reader_lines + 1] = trimmed:sub(2, -2)
                state.reader_lines[#state.reader_lines + 1] = string.rep("\226\148\128", 40)
            else
                local wrapped = word_wrap(trimmed, max_w, 14, false)
                for _, wl in ipairs(wrapped) do
                    state.reader_lines[#state.reader_lines + 1] = wl
                end
            end
        end
        state.reader_lines[#state.reader_lines + 1] = ""
    end
end

local function load_reader(url)
    state.reader_lines = {}
    state.reader_scroll = 0
    state.reader_loading = true

    local ok, resp = pcall(http.get, url)
    if ok and resp.ok then
        state.reader_raw_body = strip_html_body(resp.body or "")
        state.reader_needs_layout = true
    else
        state.reader_lines = {"Failed to load paper.", "The ar5iv HTML version may not be available.", "", "Try a different paper."}
    end
    state.reader_loading = false
end

local function draw_reader()
    if state.reader_needs_layout then
        layout_reader()
        state.reader_needs_layout = false
    end

    local title = "ar5iv Reader"
    if state.detail_paper then
        title = state.detail_paper.id
    end
    draw_header(title)

    local content_y = 42
    local content_h = 642
    local lh = screen.get_line_height(14, false)

    if state.reader_loading then
        draw_loading("Loading paper from ar5iv...", content_y, content_h)
        draw_footer({{"B", "Back", theme.btn_b}})
        return
    end

    local lines = state.reader_lines
    local total = #lines
    local visible_lines = math.max(1, math.floor(content_h / lh))
    local max_scroll = math.max(0, total - visible_lines)
    state.reader_scroll = math.min(state.reader_scroll, max_scroll)

    local y = content_y + 6
    for i = state.reader_scroll + 1, math.min(state.reader_scroll + visible_lines, total) do
        local line = lines[i]
        -- Section headers (followed by separator line)
        local is_header = (i + 1 <= total and lines[i + 1] and lines[i + 1]:match("^\226\148\128"))
        local is_sep = line:match("^\226\148\128")
        if is_header then
            screen.draw_text(line, 20, y, {color=theme.accent, size=15, bold=true})
        elseif is_sep then
            screen.draw_text(line, 20, y, {color=theme.text_dim, size=12})
        else
            screen.draw_text(line, 20, y, {color=theme.text, size=14})
        end
        y = y + lh
    end

    -- Scroll indicator
    if total > visible_lines then
        local ind_x = 715
        local bar_top = content_y + 6
        local bar_h = content_h - 12
        screen.draw_line(ind_x, bar_top, ind_x, bar_top + bar_h, {color=theme.border})
        local thumb_h = math.max(8, math.floor(bar_h * visible_lines / total))
        local progress = max_scroll > 0 and (state.reader_scroll / max_scroll) or 0
        local thumb_y = bar_top + math.floor((bar_h - thumb_h) * progress)
        screen.draw_rect(ind_x - 1, thumb_y, 3, thumb_h, {color=theme.text_dim, filled=true, radius=1})
    end

    draw_footer({
        {"B", "Back", theme.btn_b},
        {"\226\134\145\226\134\147", "Scroll", theme.btn_l},
        {"L2/R2", "Page", theme.btn_l},
    })
end

-- ── Lifecycle Callbacks ──────────────────────────────────────────────────────

function on_init()
    state.loading = true
end

function on_update(dt)
    if state.ready_to_load then
        state.ready_to_load = false
        fetch_papers()
    end

    if state.reader_url_to_load then
        local url = state.reader_url_to_load
        state.reader_url_to_load = nil
        load_reader(url)
    end
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local current = state.screen_stack[#state.screen_stack]

    if current == "list" then
        local n = #state.papers

        if button == "dpad_up" then
            state.cursor = math.max(1, state.cursor - 1)
        elseif button == "dpad_down" then
            state.cursor = math.min(math.max(1, n), state.cursor + 1)
        elseif button == "a" and n > 0 and state.cursor <= n then
            state.detail_paper = state.papers[state.cursor]
            state.detail_scroll = 0
            state.detail_needs_layout = true
            state.screen_stack[#state.screen_stack + 1] = "detail"
        elseif button == "x" then
            fetch_papers()
        elseif button == "l2" then
            local visible = math.max(1, math.floor(640 / ROW_HEIGHT))
            state.cursor = math.max(1, state.cursor - visible)
        elseif button == "r2" then
            local visible = math.max(1, math.floor(640 / ROW_HEIGHT))
            state.cursor = math.min(math.max(1, n), state.cursor + visible)
        end

    elseif current == "detail" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
        elseif button == "x" then
            if state.detail_paper and state.detail_paper.id ~= "" then
                state.screen_stack[#state.screen_stack + 1] = "reader"
                state.reader_lines = {}
                state.reader_scroll = 0
                state.reader_loading = true
                state.reader_url_to_load = AR5IV_BASE .. state.detail_paper.id
            end
        elseif button == "dpad_up" then
            state.detail_scroll = math.max(0, state.detail_scroll - 1)
        elseif button == "dpad_down" then
            state.detail_scroll = state.detail_scroll + 1
        elseif button == "l2" then
            state.detail_scroll = math.max(0, state.detail_scroll - 10)
        elseif button == "r2" then
            state.detail_scroll = state.detail_scroll + 10
        end

    elseif current == "reader" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
        elseif button == "dpad_up" then
            state.reader_scroll = math.max(0, state.reader_scroll - 1)
        elseif button == "dpad_down" then
            state.reader_scroll = state.reader_scroll + 1
        elseif button == "l2" then
            state.reader_scroll = math.max(0, state.reader_scroll - 10)
        elseif button == "r2" then
            state.reader_scroll = state.reader_scroll + 10
        end
    end
end

function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)

    if state.needs_initial_load then
        state.needs_initial_load = false
        state.ready_to_load = true
    end

    local current = state.screen_stack[#state.screen_stack]
    if current == "list" then
        draw_paper_list()
    elseif current == "detail" then
        draw_paper_detail()
    elseif current == "reader" then
        draw_reader()
    end
end
