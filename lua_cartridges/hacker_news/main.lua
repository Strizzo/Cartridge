-- Hacker News Client for Cartridge OS (Lua)
-- Browse top stories, new stories, best stories, and comments

local BASE_URL = "https://hacker-news.firebaseio.com/v0"

-- ── State ────────────────────────────────────────────────────────────────────

local state = {
    screen_stack = {"list"},  -- "list", "detail", "reader"
    -- Story list state
    tabs = {"Top", "New", "Best"},
    tab_ids = {"top", "new", "best"},
    active_tab = 1,
    stories = {},       -- tab_id -> list of stories
    cursor = 1,
    loading = false,
    error_msg = "",
    -- Story detail state
    detail_story = nil,
    detail_comments = {},
    detail_lines = {},  -- pre-laid-out lines for scrolling
    detail_scroll = 0,
    detail_loading = false,
    detail_needs_layout = false,
    -- Reader state
    reader_lines = {},
    reader_scroll = 0,
    reader_loading = false,
    reader_domain = "",
}

local ROW_HEIGHT = 64
local CARD_RADIUS = 6
local DEPTH_COLORS = {
    {100, 180, 255},
    {180, 100, 255},
    {100, 220, 140},
    {255, 180, 60},
    {255, 100, 140},
    {100, 220, 220},
}

-- ── Utility Functions ────────────────────────────────────────────────────────

local function time_ago(timestamp)
    local now = os.time()
    local diff = now - timestamp
    if diff < 60 then return "just now" end
    if diff < 3600 then return math.floor(diff / 60) .. "m ago" end
    if diff < 86400 then return math.floor(diff / 3600) .. "h ago" end
    return math.floor(diff / 86400) .. "d ago"
end

local function get_domain(url)
    if not url or url == "" then return "" end
    local domain = url:match("^https?://([^/]+)")
    return domain or ""
end

local function strip_html(text)
    if not text then return "" end
    text = text:gsub("<br%s*/?>", "\n")
    text = text:gsub("<p>", "\n\n")
    text = text:gsub("<[^>]+>", "")
    text = text:gsub("&amp;", "&")
    text = text:gsub("&lt;", "<")
    text = text:gsub("&gt;", ">")
    text = text:gsub("&quot;", '"')
    text = text:gsub("&#39;", "'")
    text = text:gsub("&nbsp;", " ")
    return text:match("^%s*(.-)%s*$") or ""
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
            -- Check if single word is too wide
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

local function fetch_story_ids(tab_id, callback)
    local url_map = {
        top = BASE_URL .. "/topstories.json",
        new = BASE_URL .. "/newstories.json",
        best = BASE_URL .. "/beststories.json",
    }
    local ok, resp = pcall(http.get_cached, url_map[tab_id], 120)
    if ok and resp.ok then
        local dok, ids = pcall(json.decode, resp.body)
        if dok and ids then
            local result = {}
            for i = 1, math.min(30, #ids) do
                result[#result + 1] = ids[i]
            end
            return result
        end
    end
    return {}
end

local function fetch_item(item_id)
    local ok, resp = pcall(http.get_cached, BASE_URL .. "/item/" .. item_id .. ".json", 300)
    if ok and resp.ok then
        local dok, item = pcall(json.decode, resp.body)
        if dok then return item end
    end
    return nil
end

local function fetch_stories(ids)
    local stories = {}
    for _, id in ipairs(ids) do
        local item = fetch_item(id)
        if item then
            stories[#stories + 1] = {
                id = item.id or 0,
                title = item.title or "",
                url = item.url or "",
                score = item.score or 0,
                by = item.by or "",
                time = item.time or 0,
                descendants = item.descendants or 0,
                kids = item.kids or {},
                text = item.text or "",
            }
        end
    end
    return stories
end

local function fetch_comments(comment_ids, depth, max_depth)
    depth = depth or 0
    max_depth = max_depth or 2
    if depth > max_depth or not comment_ids or #comment_ids == 0 then
        return {}
    end

    local comments = {}
    local batch_size = math.min(20, #comment_ids)
    for i = 1, batch_size do
        local item = fetch_item(comment_ids[i])
        if item and not item.deleted and not item.dead then
            local comment = {
                id = item.id or 0,
                by = item.by or "[deleted]",
                text = strip_html(item.text or ""),
                time = item.time or 0,
                kids = item.kids or {},
                parent = item.parent or 0,
                depth = depth,
            }
            comments[#comments + 1] = comment

            if #comment.kids > 0 and depth < max_depth then
                local children = fetch_comments(comment.kids, depth + 1, max_depth)
                for _, child in ipairs(children) do
                    comments[#comments + 1] = child
                end
            end
        end
    end
    return comments
end

-- ── Story List Screen ────────────────────────────────────────────────────────

local function load_stories()
    state.loading = true
    state.error_msg = ""
    local tab_id = state.tab_ids[state.active_tab]

    local ids = fetch_story_ids(tab_id)
    if #ids > 0 then
        local stories = fetch_stories(ids)
        state.stories[tab_id] = stories
    else
        state.error_msg = "Failed to load stories"
    end
    state.loading = false
end

local function draw_story_card(story, y, is_selected)
    local card_x = 6
    local card_w = 708
    local card_h = ROW_HEIGHT - 4

    -- Card background
    if is_selected then
        screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_highlight, border=theme.accent, radius=CARD_RADIUS})
    else
        screen.draw_card(card_x, y, card_w, card_h, {bg=theme.card_bg, radius=CARD_RADIUS})
    end

    -- Score area
    local score_x = card_x + 8
    local score_w = 46
    local score_intensity = math.min(1.0, story.score / 500)
    local score_color = {
        math.floor(140 + 115 * score_intensity),
        math.floor(140 - 60 * score_intensity),
        math.floor(140 - 100 * score_intensity),
    }

    local score_str = story.score < 10000 and tostring(story.score) or (math.floor(story.score / 1000) .. "k")
    local sw = screen.get_text_width(score_str, 16, true)
    screen.draw_text(score_str, score_x + (score_w - sw) / 2, y + 4, {color=score_color, size=16, bold=true})

    -- Arrow
    local aw = screen.get_text_width("\226\150\178", 10, false)
    screen.draw_text("\226\150\178", score_x + (score_w - aw) / 2, y + 22, {color=score_color, size=10})

    -- Title
    local title_x = card_x + score_w + 14
    local title_max_w = card_w - score_w - 80

    local title = story.title
    local tw = screen.get_text_width(title, 14, is_selected)
    if tw > title_max_w then
        while #title > 1 and screen.get_text_width(title .. "..", 14, is_selected) > title_max_w do
            title = title:sub(1, -2)
        end
        title = title .. ".."
    end
    screen.draw_text(title, title_x, y + 4, {color=theme.text, size=14, bold=is_selected})

    -- Story type badges
    local badge_x = title_x
    local badge_y = y + 22
    if story.title:sub(1, 7) == "Ask HN:" then
        local pw = screen.draw_pill("Ask", badge_x, badge_y, 180, 100, 255, {text_color={255,255,255}, size=9})
        badge_x = badge_x + pw + 4
    elseif story.title:sub(1, 8) == "Show HN:" then
        local pw = screen.draw_pill("Show", badge_x, badge_y, 80, 200, 120, {text_color={255,255,255}, size=9})
        badge_x = badge_x + pw + 4
    elseif story.url == "" then
        local pw = screen.draw_pill("Jobs", badge_x, badge_y, 255, 180, 60, {text_color={30,30,30}, size=9})
        badge_x = badge_x + pw + 4
    end

    -- Meta line
    local domain = get_domain(story.url)
    local meta = story.by .. " \194\183 " .. time_ago(story.time)
    if domain ~= "" then
        meta = meta .. " \194\183 " .. domain
    end
    screen.draw_text(meta, badge_x, badge_y, {color=theme.text_dim, size=11})

    -- Comment count badge
    if story.descendants > 0 then
        local comment_str = story.descendants < 1000 and tostring(story.descendants) or (math.floor(story.descendants / 1000) .. "k")
        local badge_bg = is_selected and {70, 70, 100} or {60, 60, 80}
        screen.draw_pill(comment_str, card_x + card_w - 55, y + (card_h - 18) / 2,
            badge_bg[1], badge_bg[2], badge_bg[3], {text_color=theme.text_dim, size=11})
    end
end

local function draw_story_list()
    local tab_id = state.tab_ids[state.active_tab]
    local stories = state.stories[tab_id] or {}
    local n = #stories

    draw_header("Hacker News",
        n > 0 and (n .. " stories") or nil,
        n > 0 and {100, 220, 100} or nil)

    draw_tab_bar(state.tabs, state.active_tab, 40)

    local content_y = 72
    local footer_y = 684
    local content_h = footer_y - content_y

    if state.loading then
        draw_loading("Loading stories...", content_y, content_h)
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
            draw_story_card(stories[idx], y, idx == state.cursor)
            y = y + ROW_HEIGHT
        end

        draw_scroll_indicator(content_y, content_h, state.cursor, n, visible)
    elseif state.error_msg ~= "" then
        local ew = screen.get_text_width(state.error_msg, 14, false)
        screen.draw_text(state.error_msg, (720 - ew) / 2, content_y + content_h / 2 - 8, {color=theme.negative, size=14})
    end

    draw_footer({
        {"L1/R1", "Tab", theme.btn_l},
        {"A", "Open", theme.btn_a},
        {"X", "Refresh", theme.btn_x},
    })
end

-- ── Story Detail Screen ──────────────────────────────────────────────────────

local function layout_detail()
    state.detail_lines = {}
    local lines = state.detail_lines
    local story = state.detail_story
    if not story then return end

    local max_w = 680

    -- Story title
    local title_lines = word_wrap(story.title, max_w, 15, true)
    for _, line in ipairs(title_lines) do
        lines[#lines + 1] = {text=line, color=theme.text, indent=0, is_header=false, depth=0}
    end

    -- Meta info
    local meta = story.score .. " pts \194\183 " .. story.by .. " \194\183 " .. time_ago(story.time) .. " \194\183 " .. story.descendants .. " comments"
    lines[#lines + 1] = {text=meta, color=theme.text_dim, indent=0, is_header=false, depth=0}

    -- URL domain
    local domain = get_domain(story.url)
    if domain ~= "" then
        lines[#lines + 1] = {text=domain, color=theme.text_accent, indent=0, is_header=false, depth=0}
    end

    -- Story text
    if story.text and story.text ~= "" then
        lines[#lines + 1] = {text="", color=theme.text, indent=0, is_header=false, depth=0}
        local clean_text = strip_html(story.text)
        for paragraph in clean_text:gmatch("[^\n]+") do
            local wrapped = word_wrap(paragraph, max_w, 14, false)
            for _, wl in ipairs(wrapped) do
                lines[#lines + 1] = {text=wl, color=theme.text, indent=0, is_header=false, depth=0}
            end
            lines[#lines + 1] = {text="", color=theme.text, indent=0, is_header=false, depth=0}
        end
    end

    -- Separator
    lines[#lines + 1] = {text="", color=theme.text, indent=0, is_header=false, depth=0}
    local sep = string.rep("\226\148\128", 40) .. " Comments " .. string.rep("\226\148\128", 10)
    lines[#lines + 1] = {text=sep, color=theme.text_dim, indent=0, is_header=false, depth=0}
    lines[#lines + 1] = {text="", color=theme.text, indent=0, is_header=false, depth=0}

    -- Comments
    if #state.detail_comments == 0 then
        lines[#lines + 1] = {text="No comments loaded", color=theme.text_dim, indent=0, is_header=false, depth=0}
        return
    end

    for _, c in ipairs(state.detail_comments) do
        local indent = c.depth * 16

        -- Author header
        local header = c.by .. " \194\183 " .. time_ago(c.time)
        local depth_color = DEPTH_COLORS[(c.depth % #DEPTH_COLORS) + 1]
        lines[#lines + 1] = {text=header, color=depth_color, indent=indent, is_header=true, depth=c.depth}

        -- Comment body
        local comment_w = math.max(100, max_w - indent)
        for paragraph in c.text:gmatch("[^\n]+") do
            local wrapped = word_wrap(paragraph, comment_w, 14, false)
            for _, wl in ipairs(wrapped) do
                lines[#lines + 1] = {text=wl, color=theme.text, indent=indent, is_header=false, depth=c.depth}
            end
        end
        if c.text == "" then
            lines[#lines + 1] = {text="", color=theme.text, indent=indent, is_header=false, depth=c.depth}
        end
        lines[#lines + 1] = {text="", color=theme.text, indent=indent, is_header=false, depth=c.depth}
    end
end

local function load_story_detail(story)
    state.detail_story = story
    state.detail_comments = {}
    state.detail_scroll = 0
    state.detail_loading = true
    state.detail_lines = {}
    state.detail_needs_layout = true

    if story.kids and #story.kids > 0 then
        state.detail_comments = fetch_comments(story.kids, 0, 2)
    end

    state.detail_loading = false
end

local function draw_story_detail()
    -- Layout must happen during render (screen.get_text_width needs active screen)
    if state.detail_needs_layout then
        layout_detail()
        state.detail_needs_layout = false
    end
    draw_header("Story", #state.detail_comments .. " comments", {100, 220, 100})

    local content_y = 42
    local content_h = 642
    local lh = screen.get_line_height(14, false)

    if state.detail_loading then
        draw_loading("Loading comments...", content_y, content_h)
        draw_footer({
            {"B", "Back", theme.btn_b},
        })
        return
    end

    local lines = state.detail_lines
    local total = #lines
    local visible_lines = math.max(1, math.floor(content_h / lh))
    local max_scroll = math.max(0, total - visible_lines)
    state.detail_scroll = math.min(state.detail_scroll, max_scroll)

    local y = content_y + 6
    for i = state.detail_scroll + 1, math.min(state.detail_scroll + visible_lines, total) do
        local line = lines[i]
        local x = 10 + line.indent

        -- Draw depth indicator lines
        if line.depth > 0 then
            for d = 0, line.depth - 1 do
                local line_x = 10 + d * 16 + 4
                local dc = DEPTH_COLORS[(d % #DEPTH_COLORS) + 1]
                screen.draw_line(line_x, y, line_x, y + lh, {color=dc})
            end
        end

        -- Text
        local max_w = 700 - line.indent
        local display = line.text
        if display ~= "" and screen.get_text_width(display, 14, line.is_header) > max_w then
            while #display > 1 and screen.get_text_width(display .. "..", 14, line.is_header) > max_w do
                display = display:sub(1, -2)
            end
            display = display .. ".."
        end

        if line.is_header then
            screen.draw_text(display, x, y, {color=line.color, size=13, bold=true})
        else
            screen.draw_text(display, x, y, {color=line.color, size=14})
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
        local progress = max_scroll > 0 and (state.detail_scroll / max_scroll) or 0
        local thumb_y = bar_top + math.floor((bar_h - thumb_h) * progress)
        screen.draw_rect(ind_x - 1, thumb_y, 3, thumb_h, {color=theme.text_dim, filled=true, radius=1})
    end

    local hints = {
        {"B", "Back", theme.btn_b},
    }
    if state.detail_story and state.detail_story.url ~= "" then
        hints[#hints + 1] = {"X", "Read Article", theme.btn_x}
    end
    hints[#hints + 1] = {"\226\134\145\226\134\147", "Scroll", theme.btn_l}
    hints[#hints + 1] = {"L2/R2", "Page", theme.btn_l}
    draw_footer(hints)
end

-- ── Reader Screen ────────────────────────────────────────────────────────────

local function load_reader(url)
    state.reader_lines = {}
    state.reader_scroll = 0
    state.reader_loading = true
    state.reader_domain = get_domain(url)

    local ok, resp = pcall(http.get, url)
    if ok and resp.ok then
        -- Simple HTML to text conversion
        local body = resp.body or ""
        -- Remove script and style blocks
        body = body:gsub("<script[^>]*>.-</script>", "")
        body = body:gsub("<style[^>]*>.-</style>", "")
        -- Convert common elements
        body = body:gsub("<br%s*/?>", "\n")
        body = body:gsub("<p[^>]*>", "\n\n")
        body = body:gsub("<h[1-6][^>]*>(.-)</h[1-6]>", "\n\n%1\n")
        body = body:gsub("<li[^>]*>", "\n  - ")
        -- Strip remaining tags
        body = body:gsub("<[^>]+>", "")
        -- Decode entities
        body = body:gsub("&amp;", "&")
        body = body:gsub("&lt;", "<")
        body = body:gsub("&gt;", ">")
        body = body:gsub("&quot;", '"')
        body = body:gsub("&#39;", "'")
        body = body:gsub("&nbsp;", " ")
        -- Clean up whitespace
        body = body:gsub("\r\n", "\n")
        body = body:gsub("\n%s*\n%s*\n+", "\n\n")
        body = body:match("^%s*(.-)%s*$") or ""

        -- Word wrap into lines
        local max_w = 680
        for paragraph in body:gmatch("[^\n]+") do
            local trimmed = paragraph:match("^%s*(.-)%s*$") or ""
            if trimmed ~= "" then
                local wrapped = word_wrap(trimmed, max_w, 14, false)
                for _, wl in ipairs(wrapped) do
                    state.reader_lines[#state.reader_lines + 1] = wl
                end
            end
            state.reader_lines[#state.reader_lines + 1] = ""
        end
    else
        state.reader_lines = {"Failed to load article."}
    end
    state.reader_loading = false
end

local function draw_reader()
    draw_header(state.reader_domain ~= "" and state.reader_domain or "Article")

    local content_y = 42
    local content_h = 642
    local lh = screen.get_line_height(14, false)

    if state.reader_loading then
        draw_loading("Loading article...", content_y, content_h)
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
        screen.draw_text(lines[i], 20, y, {color=theme.text, size=14})
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
    load_stories()
end

function on_input(button, action)
    if action ~= "press" and action ~= "repeat" then return end

    local current = state.screen_stack[#state.screen_stack]

    if current == "list" then
        -- Tab switching
        if button == "l1" and action == "press" then
            state.active_tab = state.active_tab > 1 and (state.active_tab - 1) or #state.tabs
            state.cursor = 1
            local tab_id = state.tab_ids[state.active_tab]
            if not state.stories[tab_id] then
                load_stories()
            end
            return
        elseif button == "r1" and action == "press" then
            state.active_tab = state.active_tab < #state.tabs and (state.active_tab + 1) or 1
            state.cursor = 1
            local tab_id = state.tab_ids[state.active_tab]
            if not state.stories[tab_id] then
                load_stories()
            end
            return
        end

        local tab_id = state.tab_ids[state.active_tab]
        local stories = state.stories[tab_id] or {}
        local n = #stories

        if button == "dpad_up" then
            state.cursor = math.max(1, state.cursor - 1)
        elseif button == "dpad_down" then
            state.cursor = math.min(math.max(1, n), state.cursor + 1)
        elseif button == "a" and n > 0 and state.cursor <= n then
            state.screen_stack[#state.screen_stack + 1] = "detail"
            load_story_detail(stories[state.cursor])
        elseif button == "x" then
            load_stories()
        elseif button == "l2" then
            local visible = math.max(1, math.floor(610 / ROW_HEIGHT))
            state.cursor = math.max(1, state.cursor - visible)
        elseif button == "r2" then
            local visible = math.max(1, math.floor(610 / ROW_HEIGHT))
            state.cursor = math.min(math.max(1, n), state.cursor + visible)
        end

    elseif current == "detail" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
        elseif button == "x" then
            if state.detail_story and state.detail_story.url ~= "" then
                state.screen_stack[#state.screen_stack + 1] = "reader"
                load_reader(state.detail_story.url)
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

    local current = state.screen_stack[#state.screen_stack]
    if current == "list" then
        draw_story_list()
    elseif current == "detail" then
        draw_story_detail()
    elseif current == "reader" then
        draw_reader()
    end
end
