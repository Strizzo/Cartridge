-- Local helpers capture the runtime UI before require("ui") shadows it.
local U = _G.ui.public
local C = U.C
local M = {public = U}

function M.status_color(status)
    if status == "waiting" then return C.gold end
    if status == "thinking" or status == "running" then return C.teal end
    if status == "error" or status == "stale" then return C.red end
    return C.muted
end

function M.type_label(kind)
    return ({claude_code = "Claude Code", interactive_prompt = "Prompt",
        running_process = "Process", idle_shell = "Shell", stale = "Stale"})[kind] or "Session"
end

function M.lines(value, x, y, width, size, limit, color, bold)
    for i, line in ipairs(U.wrap(value or "", width, size, limit, bold)) do
        U.text(line, x, y + (i - 1) * (size + 6), size, color, width, bold)
    end
end

function M.scroll(y, height, first, total, visible)
    if total <= visible then return end
    local thumb = math.max(16, height * visible / total)
    local progress = math.max(0, math.min(1, (first - 1) / (total - visible)))
    U.rect(704, y, 3, height, C.line)
    U.rect(704, y + (height - thumb) * progress, 3, thumb, C.red)
end

return M
