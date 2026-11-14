//! Look of the window chrome. Dark theme, sized for 2× scale.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

/// Title bar and toolbar background.
pub const TITLEBAR: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
/// Toolbar button ("pill") background.
pub const PILL: Color32 = Color32::from_rgb(0x14, 0x14, 0x14);
/// Property bar, toolbox, page navigator, docker header and tab strip, palette.
pub const BAR: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
/// Docker body.
pub const DOCKER: Color32 = Color32::from_rgb(0x2D, 0x2D, 0x2D);
/// Text fields and unselected segments.
pub const FIELD: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
/// Selected tool, active docker tab, active page tab.
pub const SUNKEN: Color32 = Color32::from_rgb(0x23, 0x23, 0x23);
/// Selected segment of a segmented control.
pub const SEGMENT_ON: Color32 = Color32::from_rgb(0x64, 0x64, 0x64);
/// Pop-up buttons (e.g. Units).
pub const POPUP: Color32 = Color32::from_rgb(0x47, 0x47, 0x47);
/// Separator lines between groups.
pub const SEPARATOR: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
/// macOS system accent (checked toolbar toggles).
pub const ACCENT: Color32 = Color32::from_rgb(0x34, 0x78, 0xF6);
pub const TEXT: Color32 = Color32::from_rgb(0xDC, 0xDC, 0xDC);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9A, 0x9A, 0x9A);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x62, 0x62, 0x62);

/// Heights and widths in points.
pub const TITLEBAR_HEIGHT: f32 = 83.0;
pub const PROPERTY_BAR_HEIGHT: f32 = 50.0;
pub const TOOLBOX_WIDTH: f32 = 34.0;
pub const PALETTE_WIDTH: f32 = 26.0;
pub const DOCKER_STRIP_WIDTH: f32 = 34.0;
pub const DOCKER_WIDTH: f32 = 290.0;
pub const DOCKER_HEADER: f32 = 32.0;
pub const PAGE_NAV_HEIGHT: f32 = 26.0;

/// The system UI font (SF Pro), so text matches native apps. Falls back to egui's font.
const SYSTEM_FONT: &str = "/System/Library/Fonts/SFNS.ttf";

pub fn apply(ctx: &egui::Context) {
    match std::fs::read(SYSTEM_FONT) {
        Ok(bytes) => ctx.add_font(egui::epaint::text::FontInsert::new(
            "system-ui",
            egui::FontData::from_owned(bytes),
            vec![egui::epaint::text::InsertFontFamily {
                family: FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Highest,
            }],
        )),
        Err(err) => log::info!("system font not loaded ({SYSTEM_FONT}): {err}"),
    }

    ctx.set_visuals_of(egui::Theme::Dark, visuals());
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(13.0)),
            (TextStyle::Button, FontId::proportional(13.0)),
            (TextStyle::Heading, FontId::proportional(17.0)),
            (TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
        style.spacing.button_padding = egui::vec2(6.0, 3.0);
        style.spacing.interact_size.y = 22.0;
        style.spacing.item_spacing = egui::vec2(6.0, 5.0);
    });
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BAR;
    v.window_fill = DOCKER;
    v.extreme_bg_color = FIELD;
    v.faint_bg_color = SUNKEN;
    v.code_bg_color = FIELD;
    v.override_text_color = None;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = Color32::from_rgb(0x4A, 0x9E, 0xF5);
    v.window_stroke = Stroke::new(1.0, SEPARATOR);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);

    let radius = CornerRadius::same(5);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BAR;
    w.noninteractive.weak_bg_fill = BAR;
    w.noninteractive.bg_stroke = Stroke::new(1.0, SEPARATOR);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = FIELD;
    w.inactive.weak_bg_fill = FIELD;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = Color32::from_rgb(0x2A, 0x2A, 0x2A);
    w.hovered.weak_bg_fill = Color32::from_rgb(0x2A, 0x2A, 0x2A);
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x55, 0x55, 0x55));
    w.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.hovered.corner_radius = radius;

    w.active.bg_fill = SEGMENT_ON;
    w.active.weak_bg_fill = SEGMENT_ON;
    w.active.bg_stroke = Stroke::NONE;
    w.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.active.corner_radius = radius;

    w.open.bg_fill = SUNKEN;
    w.open.weak_bg_fill = SUNKEN;
    w.open.bg_stroke = Stroke::NONE;
    w.open.fg_stroke = Stroke::new(1.0, TEXT);
    w.open.corner_radius = radius;
    v
}

/// A 1-pt vertical separator line spanning the available height.
pub fn vsep(ui: &mut egui::Ui) {
    let h = ui.available_height();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, h), egui::Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range().shrink(4.0), Stroke::new(1.0, SEPARATOR));
}
