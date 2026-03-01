use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use cartridge_core::sysinfo::SystemInfo;
use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::ui_constants::*;
use super::{LauncherScreen, ScreenAction, ScreenContext, ScreenId};

// Dashboard layout constants (720x720 square screen)
const HEADER_H: i32 = 36;
const FOOTER_H: i32 = 36;
const PANEL_ROW_Y: i32 = 40;
const PANEL_ROW_H: i32 = 60;
const DOCK_PANEL_Y: i32 = 106;
const DOCK_PANEL_H: i32 = 120;
const DETAIL_ROW_Y: i32 = 234;
const DETAIL_ROW_H: i32 = 200;
const RECENT_ROW_Y: i32 = 442;
const DOCK_ICON_SZ: u32 = 76;
const DOCK_ICON_FOCUS_SZ: u32 = 84;
const RECENT_ICON_SZ: u32 = 32;

/// Focus zone on the home screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HomeZone {
    Dock,
    Recent,
}

pub struct HomeScreen {
    dock_index: i32,
    recent_index: i32,
    zone: HomeZone,
}

impl Default for HomeScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl HomeScreen {
    pub fn new() -> Self {
        Self {
            dock_index: 0,
            recent_index: 0,
            zone: HomeZone::Dock,
        }
    }
}

impl LauncherScreen for HomeScreen {
    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction {
        let installed = ctx.installed_apps();
        let installed_count = installed.len() as i32;
        let recent_count = ctx.recents.len() as i32;

        for ie in events {
            if ie.action != InputAction::Press && ie.action != InputAction::Repeat {
                continue;
            }

            match ie.button {
                Button::DpadLeft => match self.zone {
                    HomeZone::Dock if installed_count > 0 => {
                        self.dock_index = (self.dock_index - 1).max(0);
                    }
                    HomeZone::Recent if recent_count > 0 => {
                        self.recent_index = (self.recent_index - 1).max(0);
                    }
                    _ => {}
                },
                Button::DpadRight => match self.zone {
                    HomeZone::Dock if installed_count > 0 => {
                        self.dock_index = (self.dock_index + 1).min(installed_count - 1);
                    }
                    HomeZone::Recent if recent_count > 0 => {
                        self.recent_index = (self.recent_index + 1).min(recent_count - 1);
                    }
                    _ => {}
                },
                Button::DpadDown => {
                    if self.zone == HomeZone::Dock && recent_count > 0 {
                        self.zone = HomeZone::Recent;
                    }
                }
                Button::DpadUp => {
                    if self.zone == HomeZone::Recent {
                        self.zone = HomeZone::Dock;
                    }
                }
                Button::L1 => {
                    if self.zone == HomeZone::Dock && installed_count > 0 {
                        self.dock_index = (self.dock_index - 5).max(0);
                    }
                }
                Button::R1 => {
                    if self.zone == HomeZone::Dock && installed_count > 0 {
                        self.dock_index = (self.dock_index + 5).min(installed_count - 1);
                    }
                }
                Button::A => {
                    if self.zone == HomeZone::Dock && installed_count > 0 {
                        let apps = ctx.installed_apps();
                        if let Some(app) = apps.get(self.dock_index as usize) {
                            let app_id = app.id.clone();
                            let app_name = app.name.clone();
                            record_recent(ctx, &app_id, &app_name);
                            return ScreenAction::LaunchApp(app_id);
                        }
                    } else if self.zone == HomeZone::Recent && recent_count > 0
                        && let Some(recent) = ctx.recents.get(self.recent_index as usize)
                    {
                        let app_id = recent.app_id.clone();
                        let app_name = recent.name.clone();
                        if ctx.installed.is_installed(&app_id) {
                            record_recent(ctx, &app_id, &app_name);
                            return ScreenAction::LaunchApp(app_id);
                        }
                    }
                }
                Button::Y => {
                    return ScreenAction::Push(ScreenId::Store);
                }
                Button::X => {
                    if self.zone == HomeZone::Dock && installed_count > 0 {
                        let apps = ctx.installed_apps();
                        if let Some(app) = apps.get(self.dock_index as usize) {
                            let app_id = app.id.clone();
                            if let Some(installer) = &ctx.installer {
                                log::info!("Removing {} from disk...", app_id);
                                match installer.remove(&app_id) {
                                    Ok(()) => log::info!("Removed {} from disk", app_id),
                                    Err(e) => log::warn!("Disk removal failed: {e}"),
                                }
                            }
                            ctx.installed.remove(&app_id);
                            ctx.save_installed();
                            let new_count = ctx.installed_apps().len() as i32;
                            if self.dock_index >= new_count && new_count > 0 {
                                self.dock_index = new_count - 1;
                            } else if new_count == 0 {
                                self.dock_index = 0;
                            }
                        }
                    }
                }
                Button::Start => {
                    return ScreenAction::Push(ScreenId::Settings);
                }
                Button::Select => {
                    return ScreenAction::ShowOverlay;
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;
        let installed_apps = ctx.installed_apps();
        let sysinfo = &ctx.sysinfo;

        // ===== HEADER =====
        // Semi-transparent header bar (grid bleeds through)
        screen.draw_rect(
            Rect::new(0, 0, SCREEN_WIDTH, HEADER_H as u32),
            Some(Color::RGBA(14, 14, 20, 220)),
            true,
            0,
            None,
        );
        // Top glow line
        screen.draw_glow_line(0, 0, SCREEN_WIDTH as i32 - 1, Color::RGBA(100, 180, 255, 80), 3, 1);
        // Title with glow
        screen.draw_text_glow(
            "Cartridge",
            12,
            6,
            theme.accent,
            theme.glow_primary,
            20,
            true,
            None,
        );
        // Status indicators on right side of header
        let mut hx = SCREEN_WIDTH as i32 - 12;

        // Battery indicator
        if sysinfo.battery_percent >= 0 {
            let pct = sysinfo.battery_percent;
            let bat_str = if sysinfo.battery_charging {
                format!("{}%+", pct)
            } else {
                format!("{}%", pct)
            };
            let bat_color = if pct > 50 {
                theme.positive
            } else if pct > 20 {
                theme.text_warning
            } else {
                theme.negative
            };
            let bw = screen.get_text_width(&bat_str, 11, false);
            screen.draw_text(&bat_str, hx - bw as i32, 10, Some(bat_color), 11, false, None);
            hx -= bw as i32 + 8;
        }

        // WiFi signal bars (4 ascending bars)
        let bars = sysinfo.wifi_bars();
        let bar_base_y = 22;
        for i in 0..4u8 {
            let bar_h = 4 + i as i32 * 3;
            let bar_x = hx - (4 - i as i32) * 5;
            let color = if i < bars {
                if bars >= 3 { theme.positive } else { theme.text_warning }
            } else {
                Color::RGBA(50, 50, 70, 120)
            };
            screen.draw_rect(
                Rect::new(bar_x, bar_base_y - bar_h, 3, bar_h as u32),
                Some(color),
                true,
                0,
                None,
            );
        }
        hx -= 26;

        // WiFi SSID or "No WiFi"
        let wifi_str = match &sysinfo.wifi_ssid {
            Some(ssid) => ssid.clone(),
            None => "No WiFi".to_string(),
        };
        let ww = screen.get_text_width(&wifi_str, 11, false);
        let wifi_color = if sysinfo.wifi_ssid.is_some() { theme.text } else { theme.text_dim };
        screen.draw_text(&wifi_str, hx - ww as i32, 10, Some(wifi_color), 11, false, None);
        hx -= ww as i32 + 12;

        // Hostname
        let hw = screen.get_text_width(&sysinfo.hostname, 11, false);
        screen.draw_text(&sysinfo.hostname, hx - hw as i32, 10, Some(theme.text_dim), 11, false, None);

        // ===== SYSTEM PANELS ROW =====
        draw_system_panels(screen, sysinfo);

        if installed_apps.is_empty() {
            // Empty state
            let msg = "No cartridges installed.";
            let w = screen.get_text_width(msg, 16, false);
            screen.draw_text(
                msg,
                (SCREEN_WIDTH as i32 - w as i32) / 2,
                200,
                Some(theme.text_dim),
                16,
                false,
                None,
            );
            let hint = "Press Y to browse the store.";
            let hw = screen.get_text_width(hint, 13, false);
            screen.draw_text(
                hint,
                (SCREEN_WIDTH as i32 - hw as i32) / 2,
                226,
                Some(theme.text_accent),
                13,
                false,
                None,
            );
        } else {
            // ===== APP DOCK PANEL =====
            draw_dock_panel(screen, &installed_apps, self.dock_index, self.zone);

            // ===== SPLIT DETAIL ROW =====
            draw_detail_row(screen, ctx, &installed_apps, self.dock_index, sysinfo);

            // ===== RECENT STRIP =====
            draw_recent_strip(screen, ctx, self.zone, self.recent_index);
        }

        // ===== PROCESS PANEL =====
        draw_process_panel(screen, sysinfo);

        // ===== FOOTER =====
        draw_footer(screen);
    }
}

// ---------------------------------------------------------------------------
// System panels: CPU, RAM, NET
// ---------------------------------------------------------------------------

fn draw_system_panels(screen: &mut Screen, sysinfo: &SystemInfo) {
    let theme = screen.theme;
    let y = PANEL_ROW_Y;
    let panel_h = PANEL_ROW_H as u32;

    // Three panels across 720px: 228 + 228 + 240 with 8px gaps
    let pw1: u32 = 228;
    let pw2: u32 = 228;
    let pw3: u32 = SCREEN_WIDTH - 16 - pw1 - pw2 - 16; // remaining
    let x1 = 8;
    let x2 = x1 + pw1 as i32 + 8;
    let x3 = x2 + pw2 as i32 + 8;

    // CPU panel
    screen.draw_card(Rect::new(x1, y, pw1, panel_h), Some(Color::RGBA(20, 20, 30, 200)), Some(theme.data_readout), 4, false);
    screen.draw_text("CPU", x1 + 6, y + 4, Some(theme.text_dim), 12, true, None);
    let cpu_str = format!("{:.0}%", sysinfo.cpu_percent);
    let cw = screen.get_text_width(&cpu_str, 12, true);
    screen.draw_text(&cpu_str, x1 + pw1 as i32 - 6 - cw as i32, y + 4, Some(theme.accent), 12, true, None);
    let spark_data: Vec<f32> = sysinfo.cpu_history.iter().copied().collect();
    if spark_data.len() >= 2 {
        screen.draw_sparkline(
            &spark_data,
            Rect::new(x1 + 6, y + 22, pw1 - 12, panel_h as u32 - 28),
            Some(theme.accent),
            Some(Color::RGBA(60, 80, 120, 30)),
        );
    }

    // RAM panel
    screen.draw_card(Rect::new(x2, y, pw2, panel_h), Some(Color::RGBA(20, 20, 30, 200)), Some(theme.data_readout), 4, false);
    screen.draw_text("RAM", x2 + 6, y + 4, Some(theme.text_dim), 12, true, None);
    let ram_str = format!("{}/{}M", sysinfo.mem_used_mb, sysinfo.mem_total_mb);
    let rw = screen.get_text_width(&ram_str, 12, false);
    screen.draw_text(&ram_str, x2 + pw2 as i32 - 6 - rw as i32, y + 4, Some(theme.accent), 12, false, None);
    screen.draw_progress_bar(
        Rect::new(x2 + 6, y + 24, pw2 - 12, 8),
        sysinfo.mem_percent / 100.0,
        Some(theme.accent),
        Some(Color::RGBA(40, 40, 60, 180)),
        3,
    );
    let mem_data: Vec<f32> = sysinfo.mem_history.iter().copied().collect();
    if mem_data.len() >= 2 {
        screen.draw_sparkline(
            &mem_data,
            Rect::new(x2 + 6, y + 36, pw2 - 12, panel_h as u32 - 42),
            Some(Color::RGBA(100, 180, 255, 40)),
            None,
        );
    }

    // NET panel
    screen.draw_card(Rect::new(x3, y, pw3, panel_h), Some(Color::RGBA(20, 20, 30, 200)), Some(theme.data_readout), 4, false);
    screen.draw_text("NET", x3 + 6, y + 4, Some(theme.text_dim), 12, true, None);
    let up_str = format!("^{}", SystemInfo::format_rate(sysinfo.net_tx_rate));
    let dn_str = format!("v{}", SystemInfo::format_rate(sysinfo.net_rx_rate));
    let nw = screen.get_text_width(&up_str, 12, false);
    screen.draw_text(&up_str, x3 + pw3 as i32 - 6 - nw as i32, y + 4, Some(theme.text_success), 12, false, None);
    let dw = screen.get_text_width(&dn_str, 12, false);
    screen.draw_text(&dn_str, x3 + pw3 as i32 - 6 - nw as i32 - 8 - dw as i32, y + 4, Some(theme.text_warning), 12, false, None);
    let net_data: Vec<f32> = sysinfo.net_history.iter().copied().collect();
    if net_data.len() >= 2 {
        screen.draw_sparkline(
            &net_data,
            Rect::new(x3 + 6, y + 22, pw3 - 12, panel_h as u32 - 28),
            Some(Color::RGBA(80, 210, 120, 120)),
            Some(Color::RGBA(60, 80, 120, 20)),
        );
    }
}

// ---------------------------------------------------------------------------
// App dock panel
// ---------------------------------------------------------------------------

fn draw_dock_panel(
    screen: &mut Screen,
    installed_apps: &[&crate::data::AppEntry],
    dock_index: i32,
    zone: HomeZone,
) {
    let theme = screen.theme;

    // Bordered dock panel
    screen.draw_card(
        Rect::new(8, DOCK_PANEL_Y, SCREEN_WIDTH - 16, DOCK_PANEL_H as u32),
        Some(Color::RGBA(18, 18, 26, 180)),
        Some(theme.data_readout),
        4,
        false,
    );

    let count = installed_apps.len();
    let dock_start_x = 18;
    let icon_gap = 8;
    let icon_stride = DOCK_ICON_SZ as i32 + icon_gap;

    let max_visible = ((SCREEN_WIDTH as i32 - dock_start_x * 2) / icon_stride).max(1) as usize;
    let scroll_offset = if dock_index as usize >= max_visible {
        (dock_index as usize) - max_visible + 1
    } else {
        0
    };

    for (vis_i, app_i) in (scroll_offset..count).enumerate() {
        let x = dock_start_x + vis_i as i32 * icon_stride;
        if x + DOCK_ICON_SZ as i32 > SCREEN_WIDTH as i32 - dock_start_x {
            break;
        }

        let is_focused = app_i == dock_index as usize && zone == HomeZone::Dock;
        let app = &installed_apps[app_i];

        let (icon_size, icon_y) = if is_focused {
            (DOCK_ICON_FOCUS_SZ, DOCK_PANEL_Y + 6 - 4)
        } else {
            (DOCK_ICON_SZ, DOCK_PANEL_Y + 6)
        };
        let icon_x = if is_focused { x - 4 } else { x };

        // Glow border on focused icon
        if is_focused {
            screen.draw_card_glow(
                Rect::new(icon_x, icon_y, icon_size, icon_size),
                Color::RGBA(100, 180, 255, 50),
                6,
                3,
            );
        }

        let border_color = if is_focused { theme.accent } else { theme.card_border };
        let bg = if is_focused { theme.card_highlight } else { theme.card_bg };

        screen.draw_card(
            Rect::new(icon_x, icon_y, icon_size, icon_size),
            Some(bg),
            Some(border_color),
            CARD_RADIUS,
            is_focused,
        );

        // Try to draw icon.png, fall back to text abbreviation
        let drew_icon = if let Some(icon_path) = crate::ui_constants::resolve_icon_path(&app.id) {
            let padding = 5;
            let img_size = icon_size - padding * 2;
            screen.draw_image(
                &icon_path,
                icon_x + padding as i32,
                icon_y + padding as i32,
                Some((img_size, img_size)),
                None,
            )
        } else {
            false
        };
        if !drew_icon {
            let abbr: String = app.name.chars().take(2).collect();
            let cat_color = crate::ui_constants::category_color(&app.category);
            let tw = screen.get_text_width(&abbr, 16, true);
            screen.draw_text(
                &abbr,
                icon_x + (icon_size as i32 - tw as i32) / 2,
                icon_y + (icon_size as i32 - 16) / 2,
                Some(cat_color),
                16,
                true,
                None,
            );
        }

        // App name label below icon
        let label_w = screen.get_text_width(&app.name, 11, false);
        let label_x = icon_x + (icon_size as i32 - label_w as i32) / 2;
        screen.draw_text(
            &app.name,
            label_x,
            icon_y + icon_size as i32 + 2,
            Some(if is_focused { theme.text } else { theme.text_dim }),
            11,
            false,
            Some(icon_size + 8),
        );
    }
}

// ---------------------------------------------------------------------------
// Split detail row: app info (left) + system stats (right)
// ---------------------------------------------------------------------------

fn draw_detail_row(
    screen: &mut Screen,
    _ctx: &ScreenContext,
    installed_apps: &[&crate::data::AppEntry],
    dock_index: i32,
    sysinfo: &SystemInfo,
) {
    let theme = screen.theme;
    let left_w: u32 = 430;
    let right_w: u32 = SCREEN_WIDTH - left_w - 24;

    // Left panel: selected app details
    let left_rect = Rect::new(8, DETAIL_ROW_Y, left_w, DETAIL_ROW_H as u32);
    screen.draw_card(left_rect, Some(Color::RGBA(20, 20, 30, 200)), Some(theme.data_readout), 4, false);
    screen.draw_text("Selected App", 14, DETAIL_ROW_Y + 4, Some(theme.text_dim), 11, true, None);

    if let Some(app) = installed_apps.get(dock_index as usize) {
        // App name
        screen.draw_text(
            &app.name,
            14,
            DETAIL_ROW_Y + 20,
            Some(theme.text),
            16,
            true,
            Some(left_w - 20),
        );

        // Version
        let ver = format!("v{}", app.version);
        let vw = screen.get_text_width(&ver, 11, false);
        screen.draw_text(
            &ver,
            8 + left_w as i32 - 8 - vw as i32,
            DETAIL_ROW_Y + 24,
            Some(theme.text_dim),
            11,
            false,
            None,
        );

        // Description
        screen.draw_text(
            &app.description,
            14,
            DETAIL_ROW_Y + 42,
            Some(theme.text_dim),
            12,
            false,
            Some(left_w - 20),
        );

        // Category pill + Author
        let cat_color = crate::ui_constants::category_color(&app.category);
        let cat_upper = app.category.to_uppercase();
        let pw = screen.draw_pill(
            &cat_upper,
            14,
            DETAIL_ROW_Y + 64,
            cat_color,
            Color::RGB(20, 20, 30),
            11,
        );

        let author_label = format!("by {}", app.author);
        screen.draw_text(
            &author_label,
            14 + pw as i32 + 8,
            DETAIL_ROW_Y + 66,
            Some(theme.text_dim),
            11,
            false,
            None,
        );

        // Permissions
        if !app.permissions.is_empty() {
            let mut px = 14;
            let perm_y = DETAIL_ROW_Y + 88;
            screen.draw_text("Perms:", px, perm_y, Some(theme.text_dim), 11, false, None);
            px += screen.get_text_width("Perms: ", 11, false) as i32;
            for perm in &app.permissions {
                if px + 60 > 8 + left_w as i32 - 8 {
                    break;
                }
                let perm_color = match perm.as_str() {
                    "network" => theme.text_warning,
                    "storage" => theme.text_accent,
                    _ => theme.text_dim,
                };
                let pw = screen.draw_pill(perm, px, perm_y - 2, theme.bg_lighter, perm_color, 11);
                px += pw as i32 + 4;
            }
        }

        // Tags
        if !app.tags.is_empty() {
            let mut tx = 14;
            let tag_y = DETAIL_ROW_Y + 110;
            for tag in &app.tags {
                if tx + 60 > 8 + left_w as i32 - 8 {
                    break;
                }
                let pw = screen.draw_pill(tag, tx, tag_y, theme.bg_lighter, theme.text_dim, 11);
                tx += pw as i32 + 4;
            }
        }
    }

    // Right panel: system stats
    let right_x = 8 + left_w as i32 + 8;
    let right_rect = Rect::new(right_x, DETAIL_ROW_Y, right_w, DETAIL_ROW_H as u32);
    screen.draw_card(right_rect, Some(Color::RGBA(20, 20, 30, 200)), Some(theme.data_readout), 4, false);
    screen.draw_text("System", right_x + 6, DETAIL_ROW_Y + 4, Some(theme.text_dim), 11, true, None);

    let sx = right_x + 6;
    let mut sy = DETAIL_ROW_Y + 22;
    let line_h = 20;

    // Uptime
    let uptime_str = format!("UP {}", sysinfo.format_uptime());
    screen.draw_text(&uptime_str, sx, sy, Some(theme.text), 13, false, None);
    sy += line_h;

    // WiFi
    let wifi_str = match &sysinfo.wifi_ssid {
        Some(ssid) => {
            let sig_str = if sysinfo.wifi_signal != 0 {
                format!(" ({}dBm)", sysinfo.wifi_signal)
            } else {
                String::new()
            };
            format!("WiFi: {ssid}{sig_str}")
        }
        None => "WiFi: disconnected".to_string(),
    };
    screen.draw_text(&wifi_str, sx, sy, Some(theme.text), 13, false, Some(right_w - 16));
    // Green/red dot for connection status
    let dot_color = if sysinfo.wifi_ssid.is_some() { theme.positive } else { theme.negative };
    screen.draw_circle(right_x + right_w as i32 - 14, sy + 8, 3, dot_color);
    sy += line_h;

    // Process count
    let proc_str = format!("{} processes", sysinfo.process_count);
    screen.draw_text(&proc_str, sx, sy, Some(theme.text), 13, false, None);
    sy += line_h;

    // Disk usage bar
    let disk_str = format!(
        "Disk: {:.0}/{:.0} GB",
        sysinfo.disk_used_gb, sysinfo.disk_total_gb
    );
    screen.draw_text(&disk_str, sx, sy, Some(theme.text), 13, false, None);
    sy += 18;
    let disk_pct = if sysinfo.disk_total_gb > 0.0 {
        sysinfo.disk_used_gb / sysinfo.disk_total_gb
    } else {
        0.0
    };
    screen.draw_progress_bar(
        Rect::new(sx, sy, right_w - 16, 6),
        disk_pct,
        Some(if disk_pct > 0.85 { theme.negative } else { theme.accent }),
        Some(Color::RGBA(40, 40, 60, 180)),
        3,
    );
}

// ---------------------------------------------------------------------------
// Compact recent strip
// ---------------------------------------------------------------------------

fn draw_recent_strip(
    screen: &mut Screen,
    ctx: &ScreenContext,
    zone: HomeZone,
    recent_index: i32,
)
 {
    if ctx.recents.is_empty() {
        return;
    }

    let theme = screen.theme;
    let y = RECENT_ROW_Y;

    screen.draw_text("Recent:", 12, y + 2, Some(theme.text_dim), 11, true, None);

    let mut rx = 80_i32;
    for (ri, recent) in ctx.recents.iter().take(6).enumerate() {
        let is_focused = zone == HomeZone::Recent && ri == recent_index as usize;

        let bg = if is_focused { theme.card_highlight } else { Color::RGBA(28, 28, 40, 180) };
        let border = if is_focused { theme.accent } else { theme.card_border };

        // Small icon card
        screen.draw_card(
            Rect::new(rx, y, RECENT_ICON_SZ, RECENT_ICON_SZ),
            Some(bg),
            Some(border),
            4,
            false,
        );

        if is_focused {
            screen.draw_card_glow(
                Rect::new(rx, y, RECENT_ICON_SZ, RECENT_ICON_SZ),
                Color::RGBA(100, 180, 255, 40),
                4,
                2,
            );
        }

        let drew_icon = if let Some(icon_path) = crate::ui_constants::resolve_icon_path(&recent.app_id) {
            let p = 3;
            let sz = RECENT_ICON_SZ - p * 2;
            screen.draw_image(&icon_path, rx + p as i32, y + p as i32, Some((sz, sz)), None)
        } else {
            false
        };
        if !drew_icon {
            let abbr: String = recent.name.chars().take(2).collect();
            screen.draw_text(&abbr, rx + 5, y + 6, Some(theme.text_dim), 11, true, None);
        }

        // Name + elapsed
        let elapsed = format_elapsed(recent.timestamp_secs);
        let label = format!("{} {}", recent.name, elapsed);
        screen.draw_text(
            &label,
            rx + RECENT_ICON_SZ as i32 + 4,
            y + 7,
            Some(if is_focused { theme.text } else { theme.text_dim }),
            11,
            false,
            Some(80),
        );

        rx += RECENT_ICON_SZ as i32 + 90;
        if rx + RECENT_ICON_SZ as i32 + 80 > SCREEN_WIDTH as i32 - 8 {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Process panel (htop-like)
// ---------------------------------------------------------------------------

const PROC_PANEL_Y: i32 = 480;
const PROC_PANEL_H: i32 = 200;

fn draw_process_panel(screen: &mut Screen, sysinfo: &SystemInfo) {
    let theme = screen.theme;
    let panel_w = SCREEN_WIDTH - 16;
    let x = 8;
    let y = PROC_PANEL_Y;

    // Panel background
    screen.draw_card(
        Rect::new(x, y, panel_w, PROC_PANEL_H as u32),
        Some(Color::RGBA(16, 16, 24, 210)),
        Some(theme.data_readout),
        4,
        false,
    );

    // Header row
    let hdr_y = y + 4;
    let title = format!("PROCESSES  ({})", sysinfo.process_count);
    screen.draw_text(&title, x + 8, hdr_y, Some(theme.text_dim), 11, true, None);

    // Column headers
    let col_y = y + 20;
    let col_pid_x = x + 8;
    let col_name_x = x + 58;
    let col_cpu_x = x + panel_w as i32 - 130;
    let col_mem_x = x + panel_w as i32 - 60;

    screen.draw_text("PID", col_pid_x, col_y, Some(theme.text_dim), 11, true, None);
    screen.draw_text("COMMAND", col_name_x, col_y, Some(theme.text_dim), 11, true, None);
    screen.draw_text("CPU%", col_cpu_x, col_y, Some(theme.text_dim), 11, true, None);
    screen.draw_text("MEM", col_mem_x, col_y, Some(theme.text_dim), 11, true, None);

    // Separator line under headers
    screen.draw_line(
        (x + 6, col_y + 14),
        (x + panel_w as i32 - 6, col_y + 14),
        Some(Color::RGBA(60, 80, 120, 40)),
        1,
    );

    // Process rows
    let row_h = 15;
    let max_rows = ((PROC_PANEL_H - 40) / row_h) as usize;
    for (i, proc) in sysinfo.top_processes.iter().take(max_rows).enumerate() {
        let ry = col_y + 18 + i as i32 * row_h;

        // Alternating row background for readability
        if i % 2 == 0 {
            screen.draw_rect(
                Rect::new(x + 4, ry - 1, panel_w - 8, row_h as u32),
                Some(Color::RGBA(30, 30, 45, 60)),
                true,
                0,
                None,
            );
        }

        // State color: R=green, S=dim, other=orange
        let state_color = match proc.state {
            'R' => theme.positive,
            'S' | 'I' => theme.text_dim,
            _ => theme.text_warning,
        };

        // PID
        let pid_str = format!("{}", proc.pid);
        screen.draw_text(&pid_str, col_pid_x, ry, Some(theme.text_dim), 11, false, None);

        // Process name (truncated)
        screen.draw_text(&proc.name, col_name_x, ry, Some(state_color), 11, false, Some(250));

        // CPU% with color coding
        let cpu_str = format!("{:.1}", proc.cpu_percent);
        let cpu_color = if proc.cpu_percent > 50.0 {
            theme.negative
        } else if proc.cpu_percent > 10.0 {
            theme.text_warning
        } else {
            theme.text
        };
        // Right-align CPU
        let cw = screen.get_text_width(&cpu_str, 11, false);
        screen.draw_text(&cpu_str, col_cpu_x + 30 - cw as i32, ry, Some(cpu_color), 11, false, None);

        // Memory
        let mem_str = if proc.mem_mb >= 100.0 {
            format!("{:.0}M", proc.mem_mb)
        } else if proc.mem_mb >= 1.0 {
            format!("{:.1}M", proc.mem_mb)
        } else {
            format!("{:.0}K", proc.mem_mb * 1024.0)
        };
        let mw = screen.get_text_width(&mem_str, 11, false);
        screen.draw_text(&mem_str, col_mem_x + 40 - mw as i32, ry, Some(theme.text_dim), 11, false, None);
    }
}

fn draw_footer(screen: &mut Screen) {
    let theme = screen.theme;
    let footer_y = SCREEN_HEIGHT as i32 - FOOTER_H;

    // Semi-transparent footer (grid bleeds through)
    screen.draw_rect(
        Rect::new(0, footer_y, SCREEN_WIDTH, FOOTER_H as u32),
        Some(Color::RGBA(14, 14, 20, 220)),
        true,
        0,
        None,
    );
    // Glow line above footer
    screen.draw_glow_line(footer_y, 0, SCREEN_WIDTH as i32 - 1, Color::RGBA(100, 180, 255, 50), 2, -1);

    let mut fx = 12;
    let w = screen.draw_button_hint("A", "Open", fx, footer_y + 7, Some(theme.btn_a), 11);
    fx += w as i32 + 10;
    let w = screen.draw_button_hint("Y", "Store", fx, footer_y + 7, Some(theme.btn_y), 11);
    fx += w as i32 + 10;
    let w = screen.draw_button_hint("X", "Remove", fx, footer_y + 7, Some(theme.btn_b), 11);
    fx += w as i32 + 10;
    screen.draw_button_hint("START", "Settings", fx, footer_y + 7, Some(theme.btn_l), 11);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn record_recent(ctx: &mut ScreenContext, app_id: &str, app_name: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entry = crate::data::RecentEntry {
        app_id: app_id.to_string(),
        name: app_name.to_string(),
        timestamp_secs: now,
    };
    ctx.recents.retain(|r| r.app_id != app_id);
    ctx.recents.insert(0, entry);
    if ctx.recents.len() > 10 {
        ctx.recents.truncate(10);
    }
    ctx.save_recents();
}

fn format_elapsed(timestamp_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now <= timestamp_secs {
        return "now".to_string();
    }
    let diff = now - timestamp_secs;
    if diff < 60 {
        format!("{}s", diff)
    } else if diff < 3600 {
        format!("{}m", diff / 60)
    } else if diff < 86400 {
        format!("{}h", diff / 3600)
    } else {
        format!("{}d", diff / 86400)
    }
}
