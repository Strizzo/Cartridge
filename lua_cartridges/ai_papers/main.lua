-- AI Papers / research index and reading desk.
local U = ui.public
local C = U.C
local API_URL = "https://huggingface.co/api/daily_papers"
local AR5IV_BASE = "https://ar5iv.labs.arxiv.org/html/"
local ROW_HEIGHT, LIST_VISIBLE = 116, 4
local DOC_Y, DOC_BOTTOM = 164, 630

local state = {
    screen_stack = {"list"}, papers = {}, cursor = 1, loading = false, error_msg = "",
    detail_paper = nil, detail_lines = {}, detail_scroll = 0, detail_max_scroll = 0, detail_needs_layout = false,
    reader_lines = {}, reader_scroll = 0, reader_max_scroll = 0, reader_loading = false,
    reader_raw_body = nil, reader_needs_layout = false, reader_generation = 0, reader_error = "",
}

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
    body = body:gsub("&#(%d+);", function(n)
        local code = tonumber(n)
        if code and code <= 0x10FFFF and not (code >= 0xD800 and code <= 0xDFFF) then return utf8.char(code) end
        return "?"
    end)
    -- Clean up whitespace
    body = body:gsub("\r\n", "\n")
    body = body:gsub("\n%s*\n%s*\n+", "\n\n")
    body = body:gsub("  +", " ")
    return body:match("^%s*(.-)%s*$") or ""
end

-- Cached line records keep long abstracts and readers out of idle updates.
local function add_text(lines, value, size, color, bold)
    for _, text in ipairs(U.wrap(value, 620, size, 1024, bold)) do
        lines[#lines + 1] = {text=text, size=size, color=color, bold=bold,
            height=size >= 20 and 31 or (size >= 18 and 27 or 23)}
    end
end

local function gap(lines, height)
    lines[#lines + 1] = {text="", height=height or 14}
end

local function section(lines, label)
    gap(lines)
    lines[#lines + 1] = {text=label, size=14, color=C.red, bold=true, height=34, rule=true}
end

local function draw_document(lines, scroll, label)
    U.panel(U.X, U.TOP, U.W, U.BOTTOM - U.TOP, C.panel)
    U.text(label, 44, 124, 14, C.muted, 540, true)
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
        if line.text ~= "" then U.text(line.text, 44, y, line.size or 18, line.color or C.paper, 620, line.bold) end
        y = y + line.height
    end
    if max_scroll > 0 then
        U.text(string.format("%d%%", math.floor(scroll / max_scroll * 100)), 610, 124, 14, C.muted, 60)
        U.rect(684, DOC_Y, 3, DOC_BOTTOM - DOC_Y, C.line)
        local thumb = math.max(24, (DOC_BOTTOM - DOC_Y) / math.max(1, #lines) * (#lines - first + 1))
        U.rect(684, DOC_Y + (DOC_BOTTOM - DOC_Y - thumb) * scroll / max_scroll, 3, thumb, C.red)
    end
    return scroll, max_scroll
end

local pending = {}
local function request(url, handler)
    local ok, id = pcall(http.get_async, url)
    if ok and id then pending[id] = handler
    else handler({ok=false, status=0, body="Request could not be started.", elapsed_ms=0}) end
end

local function fetch_papers()
    if state.loading then return end
    state.loading = true
    state.error_msg = ""

    request(API_URL .. "?limit=30", function(resp)
    if resp and resp.ok then
        local dok, data = pcall(json.decode, resp.body)
        if dok and type(data) == "table" then
            state.papers = {}
            for i = 1, math.min(30, #data) do
                local entry = data[i]
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
                    summary_is_ai = paper.summary == nil and paper.ai_summary ~= nil,
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
    state.cursor = math.max(1, math.min(state.cursor, #state.papers))
    end)
end

local function draw_paper_card(paper, y, selected, index)
    local fg, secondary = U.row(U.X, y, U.W, ROW_HEIGHT - 6, selected)
    U.display(string.format("%02d", index), 40, y + 12, 28, fg, 48)
    U.text(tostring(paper.upvotes), 40, y + 49, 18, fg, 52, true)
    U.text("votes", 40, y + 78, 14, secondary, 50)
    U.rect(88, y + 14, 1, 80, selected and C.quiet or C.line)
    local category = #paper.keywords > 0 and table.concat(paper.keywords, " / ", 1, math.min(2, #paper.keywords)) or "RESEARCH PAPER"
    if paper.num_comments > 0 then category = category .. " / " .. paper.num_comments .. " comments" end
    U.text(category, 104, y + 9, 14, selected and fg or C.teal, 574, true)
    if not paper.title_layout then paper.title_layout = U.wrap(paper.title, 574, 18, 2, true) end
    for i, line in ipairs(paper.title_layout) do U.text(line, 104, y + 31 + (i - 1) * 24, 18, fg, 574, true) end
    U.text(paper.authors_str ~= "" and paper.authors_str or "Authors unavailable", 104, y + 84, 14, secondary, 456)
    U.text(time_ago(paper.published), 580, y + 84, 14, secondary, 98)
end

local function draw_paper_list()
    local n = #state.papers
    local status = state.loading and "Refreshing" or (state.error_msg ~= "" and (n > 0 and "Cached feed" or "Unavailable") or (n .. " papers"))
    U.header("AI Papers", "RESEARCH / HUGGING FACE DAILY", status, state.error_msg ~= "" and C.gold or C.muted)
    U.panel(24, 112, 672, 50, C.panel)
    U.display("Community ranking", 40, 119, 24, C.paper, 370)
    U.text(state.error_msg ~= "" and "X TO RETRY" or "BY UPVOTES", 526, 130, 14, state.error_msg ~= "" and C.gold or C.muted, 154, true)
    if n > 0 then
        state.cursor = math.max(1, math.min(state.cursor, n))
        local start = math.max(1, math.min(state.cursor - 1, math.max(1, n - LIST_VISIBLE + 1)))
        for idx = start, math.min(n, start + LIST_VISIBLE - 1) do
            draw_paper_card(state.papers[idx], 174 + (idx - start) * ROW_HEIGHT, idx == state.cursor, idx)
        end
    elseif state.loading then
        U.empty("Fetching daily papers", "Loading up to 30 papers from Hugging Face. The feed is ranked by community upvotes.")
    elseif state.error_msg ~= "" then
        U.empty("Papers unavailable", state.error_msg .. ". Press X to retry the daily feed.")
    else
        U.empty("No papers returned", "The daily feed returned no entries. Press X to refresh it.")
    end
    U.footer({{"A", "Details"}, {"X", state.error_msg ~= "" and "Retry" or "Refresh"}, {"D-pad", "Move"}, {"L2/R2", "Page"}})
end

local function layout_detail()
    local lines, paper = {}, state.detail_paper
    state.detail_lines = lines
    if not paper then return end
    add_text(lines, paper.title, 22, C.paper, true)
    gap(lines)
    add_text(lines, paper.authors_str ~= "" and paper.authors_str or "Authors unavailable", 16, C.teal)
    add_text(lines, "arXiv:" .. paper.id .. " / " .. paper.upvotes .. " upvotes / " .. paper.num_comments .. " comments", 14, C.muted)
    if paper.published ~= "" then add_text(lines, "Published " .. paper.published:sub(1, 10), 14, C.muted) end
    if #paper.keywords > 0 then
        section(lines, "TOPICS")
        add_text(lines, table.concat(paper.keywords, " / "), 14, C.teal)
    end
    if paper.ai_summary and paper.ai_summary ~= "" and paper.ai_summary ~= paper.summary then
        section(lines, "AI SUMMARY / PROVIDED BY THE FEED")
        add_text(lines, paper.ai_summary, 18, C.paper)
    end
    section(lines, paper.summary_is_ai and "AI SUMMARY / ABSTRACT NOT PROVIDED" or "ABSTRACT")
    if paper.summary and paper.summary ~= "" then
        for paragraph in paper.summary:gmatch("[^\n]+") do
            add_text(lines, paragraph, 18, C.paper)
            gap(lines)
        end
    else
        add_text(lines, "The feed did not provide an abstract. Press X to try the full paper.", 18, C.muted)
    end
end

local function draw_paper_detail()
    U.header("Paper Details", "AI PAPERS / ABSTRACT & AUTHORS", state.detail_paper and ("arXiv:" .. state.detail_paper.id) or nil)
    if state.detail_needs_layout then layout_detail(); state.detail_needs_layout = false end
    state.detail_scroll, state.detail_max_scroll = draw_document(state.detail_lines, state.detail_scroll, "RESEARCH NOTE / FULL ABSTRACT")
    local hints = {{"B", "Back"}, {"D-pad", "Scroll"}, {"L2/R2", "Page"}}
    if state.detail_paper and state.detail_paper.id ~= "" then hints[#hints + 1] = {"X", "Paper"} end
    U.footer(hints)
end

local function layout_reader()
    state.reader_lines = {}
    local body = state.reader_raw_body or ""
    state.reader_raw_body = nil
    for paragraph in body:gmatch("[^\n]+") do
        local trimmed = paragraph:match("^%s*(.-)%s*$") or ""
        if trimmed ~= "" then
            if trimmed:match("^%[.+%]$") then
                section(state.reader_lines, "SECTION")
                add_text(state.reader_lines, trimmed:sub(2, -2), 20, C.paper, true)
            else
                add_text(state.reader_lines, trimmed, 18, C.paper)
            end
            gap(state.reader_lines)
        end
    end
end

local function load_reader(url)
    state.reader_generation = state.reader_generation + 1
    local generation = state.reader_generation
    state.reader_lines = {}
    state.reader_scroll, state.reader_max_scroll = 0, 0
    state.reader_loading = true
    state.reader_error = ""
    state.reader_needs_layout = false
    state.reader_raw_body = nil
    request(url, function(resp)
        if generation ~= state.reader_generation then return end
        if resp and resp.ok then
            state.reader_raw_body = strip_html_body(resp.body or "")
            state.reader_needs_layout = true
        else
            state.reader_error = "The ar5iv text version may be unavailable. Press A to retry, or B to return to the abstract."
        end
        state.reader_loading = false
    end)
end

local function draw_reader()
    U.header("Paper Reader", "AR5IV / " .. (state.detail_paper and state.detail_paper.id or ""), state.reader_loading and "Loading" or (state.reader_error ~= "" and "Unavailable" or "Text view"), state.reader_error ~= "" and C.gold or C.muted)
    if state.reader_needs_layout then layout_reader(); state.reader_needs_layout = false end
    if state.reader_loading then
        U.empty("Fetching paper text", "Loading the ar5iv HTML edition. Press B to return to the abstract.")
    elseif state.reader_error ~= "" then
        U.empty("Paper text unavailable", state.reader_error)
    elseif #state.reader_lines == 0 then
        U.empty("No readable paper text", "The HTML edition returned no plain text. Press A to retry, or B for the abstract.")
    else
        state.reader_scroll, state.reader_max_scroll = draw_document(state.reader_lines, state.reader_scroll, "TEXT VIEW / MATH & TABLES SIMPLIFIED")
    end
    local hints = {{"B", "Back"}, {"D-pad", "Scroll"}, {"L2/R2", "Page"}}
    if not state.reader_loading and (state.reader_error ~= "" or #state.reader_lines == 0) then hints[#hints + 1] = {"A", "Retry"} end
    U.footer(hints)
end

local function visual_key()
    return table.concat({state.screen_stack[#state.screen_stack], tostring(state.papers), state.cursor,
        tostring(state.loading), state.error_msg, tostring(state.detail_paper), state.detail_scroll,
        tostring(state.detail_needs_layout), tostring(state.reader_lines), state.reader_scroll,
        tostring(state.reader_loading), state.reader_error, tostring(state.reader_needs_layout)}, "\0")
end

function on_init()
    fetch_papers()
    app.request_redraw()
end

function on_update(dt)
    local before = visual_key()
    for _, response in ipairs(http.poll()) do
        local handler = pending[response.id]
        if handler then pending[response.id] = nil; handler(response) end
    end
    local dirty = before ~= visual_key()
    if dirty then app.request_redraw() end
    return dirty
end

local function handle_input(button, action)
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
            local visible = math.max(1, LIST_VISIBLE)
            state.cursor = math.max(1, state.cursor - visible)
        elseif button == "r2" then
            local visible = math.max(1, LIST_VISIBLE)
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
                load_reader(AR5IV_BASE .. state.detail_paper.id)
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
            load_reader(AR5IV_BASE .. state.detail_paper.id)
        elseif button == "b" then
            state.reader_generation = state.reader_generation + 1
            state.reader_loading = false
            state.screen_stack[#state.screen_stack] = nil
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
    if current == "list" then draw_paper_list()
    elseif current == "detail" then draw_paper_detail()
    elseif current == "reader" then draw_reader() end
end
