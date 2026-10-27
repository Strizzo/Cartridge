-- Compatibility page only; CartridgeOS 0.8 routes runtime="browser" to the
-- built-in native browser. Older runtimes never execute arbitrary native code.
function on_render()
    screen.clear(theme.bg.r, theme.bg.g, theme.bg.b)
    ui.public.header("BROWSER", "RUNTIME UPDATE REQUIRED", "")
    screen.draw_text("Install CartridgeOS 0.8 or newer to use Browser.", 24, 180,
        {color=theme.text, size=18, max_width=672})
end
