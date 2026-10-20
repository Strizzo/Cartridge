-- Hacker News / public reading desk. Network requests remain asynchronous.
local U = ui.public
local C = U.C
local BASE_URL = "https://hacker-news.firebaseio.com/v0"
local MAX_COMMENTS, MAX_STORIES = 20, 30
local ROW_HEIGHT, LIST_VISIBLE = 92, 5
local DOC_Y, DOC_BOTTOM = 164, 630

local state = {
    screen_stack = {"list"},
    tabs = {"Top", "New", "Best"}, tab_ids = {"top", "new", "best"}, active_tab = 1,
    stories = {}, cursor = 1, loading = false, loading_tab = nil, error_msg = "",
    list_errors = {},
    detail_story = nil, detail_comments = {}, detail_lines = {}, detail_scroll = 0,
    detail_max_scroll = 0, detail_loading = false, detail_needs_layout = false, detail_error = "",
    reader_lines = {}, reader_scroll = 0, reader_max_scroll = 0, reader_loading = false,
    reader_domain = "", reader_url = "", reader_error = "", reader_raw_body = nil,
    reader_needs_layout = false,
    pending = {}, list_gen = 0, detail_gen = 0, reader_gen = 0,
}

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

-- All font measurement and prepared layouts run only from on_render.
local function add_text(lines, value, size, color, bold, indent)
    indent = indent or 0
    for _, text in ipairs(U.wrap(value, 620 - indent, size, 1024, bold)) do
        lines[#lines + 1] = {text=text, size=size, color=color, bold=bold,
            indent=indent, height=size >= 20 and 31 or (size >= 18 and 27 or 23)}
    end
end

local function gap(lines, height)
    lines[#lines + 1] = {text="", height=height or 14}
end

local function section(lines, label)
    gap(lines, 12)
    lines[#lines + 1] = {text=label, size=14, color=C.red, bold=true, height=34, rule=true}
end

local function draw_document(lines, scroll, label)
    U.panel(U.X, U.TOP, U.W, U.BOTTOM - U.TOP, C.panel)
    U.text(label, 44, 124, 14, C.muted, 456, true)
    U.rule(44, 152, 628, C.line)
    local used, first = 0, #lines + 1
    for i = #lines, 1, -1 do
        if used + lines[i].height > DOC_BOTTOM - DOC_Y then break end
        used, first = used + lines[i].height, i
    end
    local max_scroll = math.max(0, first - 1)
    scroll = math.max(0, math.min(scroll, max_scroll))
    local y = DOC_Y
    for i = scroll + 1, #lines do
        local line = lines[i]
        if y + line.height > DOC_BOTTOM then break end
        if line.rule then U.rule(44, y + 27, 620, C.line) end
        if line.text ~= "" then
            local indent = line.indent or 0
            if indent > 0 then U.rect(44 + indent - 10, y, 2, line.height, C.line) end
            U.text(line.text, 44 + indent, y, line.size or 18, line.color or C.paper, 620 - indent, line.bold)
        end
        y = y + line.height
    end
    if max_scroll > 0 then
        U.text(string.format("%d%%", math.floor(scroll / max_scroll * 100)), 610, 124, 14, C.muted, 60)
        U.rect(684, DOC_Y, 3, DOC_BOTTOM - DOC_Y, C.line)
        local thumb = math.max(24, (DOC_BOTTOM - DOC_Y) / math.max(1, #lines) * (first > 1 and (#lines - first + 1) or #lines))
        U.rect(684, DOC_Y + (DOC_BOTTOM - DOC_Y - thumb) * scroll / max_scroll, 3, thumb, C.red)
    end
    return scroll, max_scroll
end

local function request(url, handler)
    local ok, id = pcall(http.get_async, url)
    if ok and id then
        state.pending[id] = handler
        return id
    end
    handler(nil)
    return nil
end

local function decode(resp)
    if not resp or not resp.ok then return nil end
    local ok, data = pcall(json.decode, resp.body)
    if ok then return data end
    return nil
end

local function item_url(item_id)
    return BASE_URL .. "/item/" .. item_id .. ".json"
end

local function story_from_item(item)
    return {
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

-- Fetch a list of item ids in one batch; calls done(items) with a table
-- indexed like `ids` (missing/failed entries are nil) once all responded.
-- `is_live()` lets a superseded load drop its late responses.
local function fetch_items(ids, is_live, done)
    local n = #ids
    if n == 0 then done({}) return end
    local items, completed = {}, 0
    for i = 1, n do
        request(item_url(ids[i]), function(resp)
            if not is_live() then return end
            items[i] = decode(resp)
            completed = completed + 1
            if completed >= n then done(items) end
        end)
    end
end

local function load_stories()
    local tab_id = state.tab_ids[state.active_tab]
    if state.loading and state.loading_tab == tab_id then return end
    state.loading = true
    state.error_msg = ""
    state.loading_tab = tab_id
    state.list_errors[tab_id] = ""
    state.list_gen = state.list_gen + 1
    local gen = state.list_gen
    local function live() return gen == state.list_gen end
    request(BASE_URL .. "/" .. tab_id .. "stories.json", function(resp)
        if not live() then return end
        local all_ids = decode(resp)
        if type(all_ids) ~= "table" or #all_ids == 0 then
            state.error_msg = "Hacker News did not return a story list."
            state.list_errors[tab_id] = state.error_msg
            state.loading = false
            return
        end
        local ids = {}
        for i = 1, math.min(MAX_STORIES, #all_ids) do ids[i] = all_ids[i] end
        fetch_items(ids, live, function(items)
            local stories = {}
            for i = 1, #ids do
                if type(items[i]) == "table" and not items[i].deleted and not items[i].dead then
                    local story = story_from_item(items[i])
                    story.rank = i
                    stories[#stories + 1] = story
                end
            end
            if #stories > 0 then
                state.stories[tab_id] = stories
                if #stories < #ids then state.list_errors[tab_id] = "Some stories could not be loaded." end
            else
                state.list_errors[tab_id] = "Story requests failed. Press X to retry."
            end
            state.loading = false
        end)
    end)
end

local function draw_story_card(story, y, selected, index)
    local fg, secondary = U.row(U.X, y, U.W, ROW_HEIGHT - 6, selected)
    U.display(string.format("%02d", story.rank or index), 40, y + 8, 30, fg, 54)
    U.text(tostring(story.score) .. " pts", 40, y + 53, 14, selected and fg or C.muted, 66, true)
    U.rect(104, y + 12, 1, 62, selected and C.quiet or C.line)
    if not story.title_layout then story.title_layout = U.wrap(story.title, 558, 18, 2, true) end
    for i, line in ipairs(story.title_layout) do
        U.text(line, 120, y + 9 + (i - 1) * 24, 18, fg, 558, true)
    end
    local domain = get_domain(story.url)
    local meta = story.by .. " / " .. time_ago(story.time) .. " / " .. story.descendants .. " comments"
    if domain ~= "" then meta = meta .. " / " .. domain end
    U.text(meta, 120, y + 62, 14, secondary, 558)
end

local function draw_story_list()
    local tab_id = state.tab_ids[state.active_tab]
    local stories = state.stories[tab_id] or {}
    local n = #stories
    local loading = state.loading and state.loading_tab == tab_id
    local error_msg = state.list_errors[tab_id] or ""
    local status = loading and "Refreshing" or (error_msg ~= "" and "Load issue" or (n > 0 and (n .. " stories") or "No stories"))
    U.header("Hacker News", "NEWS / TECHNOLOGY / DISCUSSION", status, error_msg ~= "" and C.gold or C.muted)
    U.tabs(state.tabs, state.active_tab)
    U.text(error_msg ~= "" and (n > 0 and "INCOMPLETE OR CACHED / X TO RETRY" or "STORY FEED") or "RANK / SCORE     /     FRONT PAGE", 24, 156, 14, C.muted, 550, true)
    if n > 0 then
        state.cursor = math.max(1, math.min(state.cursor, n))
        U.text(state.cursor .. " / " .. n, 612, 156, 14, C.muted, 84)
        local start = math.max(1, math.min(state.cursor - 1, math.max(1, n - LIST_VISIBLE + 1)))
        for idx = start, math.min(n, start + LIST_VISIBLE - 1) do
            draw_story_card(stories[idx], 176 + (idx - start) * ROW_HEIGHT, idx == state.cursor, idx)
        end
    elseif loading then
        U.empty("Fetching stories", "Loading stories from Hacker News. Press L1 / R1 to choose another feed.")
    elseif error_msg ~= "" then
        U.empty("Stories unavailable", error_msg .. " Press X to retry the feed.")
    else
        U.empty("No stories returned", "Press X to refresh this feed, or L1 / R1 to try another feed.")
    end
    U.footer({{"L1/R1", "Feed"}, {"A", "Open"}, {"X", error_msg ~= "" and "Retry" or "Refresh"}, {"D-pad", "Move"}, {"L2/R2", "Page"}})
end

local function layout_detail()
    local lines, story = {}, state.detail_story
    state.detail_lines = lines
    if not story then return end
    add_text(lines, story.title, 22, C.paper, true)
    gap(lines)
    add_text(lines, story.score .. " points / " .. story.by .. " / " .. time_ago(story.time), 14, C.muted)
    local domain = get_domain(story.url)
    if domain ~= "" then add_text(lines, domain, 14, C.teal) end
    if story.text and story.text ~= "" then
        section(lines, "STORY")
        for paragraph in strip_html(story.text):gmatch("[^\n]+") do
            add_text(lines, paragraph, 18, C.paper)
            gap(lines)
        end
    end
    section(lines, "COMMENTS / " .. story.descendants .. " TOTAL / FIRST " .. MAX_COMMENTS .. " TOP-LEVEL ONLY")
    if state.detail_loading then
        add_text(lines, "Fetching comments. The story is ready to read.", 16, C.muted)
    elseif state.detail_error ~= "" then
        add_text(lines, state.detail_error .. " Press A to retry comments.", 16, C.gold)
        gap(lines)
    elseif #state.detail_comments == 0 then
        add_text(lines, story.descendants > 0 and "No readable comments in this sample." or "No comments yet.", 16, C.muted)
    end
    for _, comment in ipairs(state.detail_comments) do
        gap(lines, 8)
        add_text(lines, comment.by .. " / " .. time_ago(comment.time), 14, C.red, true, 12)
        for paragraph in comment.text:gmatch("[^\n]+") do
            add_text(lines, paragraph, 18, C.paper, false, 12)
            gap(lines, 10)
        end
        if comment.text == "" then add_text(lines, "No comment text.", 16, C.muted, false, 12) end
        gap(lines, 14)
    end
end

local function load_story_detail(story)
    state.detail_story = story
    state.detail_comments = {}
    state.detail_scroll, state.detail_max_scroll = 0, 0
    state.detail_lines = {}
    state.detail_error = ""
    state.detail_needs_layout = true
    state.detail_gen = state.detail_gen + 1
    local gen = state.detail_gen
    local function live() return gen == state.detail_gen end
    local ids = {}
    for i = 1, math.min(MAX_COMMENTS, #(story.kids or {})) do ids[i] = story.kids[i] end
    state.detail_loading = #ids > 0
    if #ids == 0 then return end
    fetch_items(ids, live, function(items)
        local comments, failed = {}, 0
        for i = 1, #ids do
            local item = items[i]
            if type(item) ~= "table" then failed = failed + 1 end
            if type(item) == "table" and not item.deleted and not item.dead then
                comments[#comments + 1] = {
                    id=item.id or 0, by=item.by or "[deleted]", text=strip_html(item.text or ""),
                    time=item.time or 0, kids=item.kids or {}, parent=item.parent or 0, depth=0,
                }
            end
        end
        state.detail_comments = comments
        state.detail_error = failed > 0 and (failed .. " comment requests failed; this sample is incomplete.") or ""
        state.detail_loading = false
        state.detail_needs_layout = true
    end)
end

local function draw_story_detail()
    U.header("Discussion", "HACKER NEWS / STORY & COMMENTS", state.detail_loading and "Loading comments" or (#state.detail_comments .. " loaded"), state.detail_error ~= "" and C.gold or C.muted)
    if state.detail_needs_layout then layout_detail(); state.detail_needs_layout = false end
    state.detail_scroll, state.detail_max_scroll = draw_document(state.detail_lines, state.detail_scroll, "STORY / TOP-LEVEL DISCUSSION")
    local hints = {{"B", "Back"}, {"D-pad", "Scroll"}, {"L2/R2", "Page"}}
    if state.detail_story and state.detail_story.url ~= "" then hints[#hints + 1] = {"X", "Article"} end
    if state.detail_error ~= "" and not state.detail_loading then hints[#hints + 1] = {"A", "Retry"} end
    U.footer(hints)
end

local function strip_html_body(body)
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
    return body:match("^%s*(.-)%s*$") or ""
end

local function layout_reader()
    state.reader_lines = {}
    local body = state.reader_raw_body or ""
    state.reader_raw_body = nil
    for paragraph in body:gmatch("[^\n]+") do
        local trimmed = paragraph:match("^%s*(.-)%s*$") or ""
        if trimmed ~= "" then
            add_text(state.reader_lines, trimmed, 18, C.paper)
            gap(state.reader_lines)
        end
    end
end

local function load_reader(url)
    state.reader_lines = {}
    state.reader_scroll, state.reader_max_scroll = 0, 0
    state.reader_loading = true
    state.reader_url, state.reader_domain = url, get_domain(url)
    state.reader_error = ""
    state.reader_raw_body = nil
    state.reader_needs_layout = false
    state.reader_gen = state.reader_gen + 1
    local gen = state.reader_gen
    request(url, function(resp)
        if gen ~= state.reader_gen then return end
        if resp and resp.ok then
            state.reader_raw_body = strip_html_body(resp.body or "")
            state.reader_needs_layout = true
        else
            state.reader_error = "The article request failed. Press A to retry, or B for the discussion."
        end
        state.reader_loading = false
    end)
end

local function draw_reader()
    U.header("Article", "HACKER NEWS / " .. state.reader_domain, state.reader_loading and "Loading" or (state.reader_error ~= "" and "Unavailable" or "Text view"), state.reader_error ~= "" and C.gold or C.muted)
    if state.reader_needs_layout then layout_reader(); state.reader_needs_layout = false end
    if state.reader_loading then
        U.empty("Fetching article", "Retrieving the linked page. Press B to return to the discussion.")
    elseif state.reader_error ~= "" then
        U.empty("Article unavailable", state.reader_error)
    elseif #state.reader_lines == 0 then
        U.empty("No readable text", "This page returned no plain text. Press A to retry, or B to return to the discussion.")
    else
        state.reader_scroll, state.reader_max_scroll = draw_document(state.reader_lines, state.reader_scroll, "PLAIN TEXT / " .. state.reader_domain)
    end
    local hints = {{"B", "Back"}, {"D-pad", "Scroll"}, {"L2/R2", "Page"}}
    if not state.reader_loading and (state.reader_error ~= "" or #state.reader_lines == 0) then hints[#hints + 1] = {"A", "Retry"} end
    U.footer(hints)
end

local function visual_key()
    local tab = state.tab_ids[state.active_tab]
    return table.concat({state.screen_stack[#state.screen_stack], state.active_tab, state.cursor,
        tostring(state.stories[tab]), tostring(state.loading and state.loading_tab == tab), state.list_errors[tab] or "",
        tostring(state.detail_story), tostring(state.detail_comments), state.detail_scroll,
        tostring(state.detail_loading), state.detail_error, tostring(state.detail_needs_layout),
        tostring(state.reader_lines), state.reader_scroll, tostring(state.reader_loading), state.reader_error,
        tostring(state.reader_needs_layout)}, "\0")
end

function on_init()
    load_stories()
    app.request_redraw()
end

function on_update(dt)
    local before = visual_key()
    for _, resp in ipairs(http.poll()) do
        local handler = state.pending[resp.id]
        if handler then state.pending[resp.id] = nil; handler(resp) end
    end
    local dirty = before ~= visual_key()
    if dirty then app.request_redraw() end
    return dirty
end

local function handle_input(button, action)
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
            local visible = math.max(1, LIST_VISIBLE)
            state.cursor = math.max(1, state.cursor - visible)
        elseif button == "r2" then
            local visible = math.max(1, LIST_VISIBLE)
            state.cursor = math.min(math.max(1, n), state.cursor + visible)
        end

    elseif current == "detail" then
        if button == "b" then
            state.screen_stack[#state.screen_stack] = nil
            state.detail_gen = state.detail_gen + 1  -- drop in-flight comment fetches
            state.detail_loading = false
        elseif button == "a" and state.detail_error ~= "" and not state.detail_loading then
            load_story_detail(state.detail_story)
        elseif button == "x" then
            if state.detail_story and state.detail_story.url ~= "" then
                state.screen_stack[#state.screen_stack + 1] = "reader"
                load_reader(state.detail_story.url)
            end
        elseif button == "dpad_up" then
            state.detail_scroll = math.max(0, state.detail_scroll - 1)
        elseif button == "dpad_down" then
            state.detail_scroll = math.min(state.detail_max_scroll, state.detail_scroll + 1)
        elseif button == "l2" then
            state.detail_scroll = math.max(0, state.detail_scroll - 10)
        elseif button == "r2" then
            state.detail_scroll = math.min(state.detail_max_scroll, state.detail_scroll + 10)
        end

    elseif current == "reader" then
        if button == "a" and not state.reader_loading and (state.reader_error ~= "" or #state.reader_lines == 0) then
            load_reader(state.reader_url)
        elseif button == "b" then
            state.screen_stack[#state.screen_stack] = nil
            state.reader_gen = state.reader_gen + 1  -- drop an in-flight article fetch
            state.reader_loading = false
        elseif button == "dpad_up" then
            state.reader_scroll = math.max(0, state.reader_scroll - 1)
        elseif button == "dpad_down" then
            state.reader_scroll = math.min(state.reader_max_scroll, state.reader_scroll + 1)
        elseif button == "l2" then
            state.reader_scroll = math.max(0, state.reader_scroll - 10)
        elseif button == "r2" then
            state.reader_scroll = math.min(state.reader_max_scroll, state.reader_scroll + 10)
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
    if current == "list" then draw_story_list()
    elseif current == "detail" then draw_story_detail()
    elseif current == "reader" then draw_reader() end
end
