//! Window chrome around the workspace: menu bar (non-native), property bar, status bar
//! and colour palette. The title bar and main toolbar are in `titlebar`.

use egui::{Color32, Sense, Stroke, Vec2, vec2};
use egui_material_icons::{MaterialIcon, icons::*};
use vs_core::{Orientation, PAGE_PRESETS, PagePreset};
use vs_render::viewport::{MAX_ZOOM_PERCENT, MIN_ZOOM_PERCENT};

use crate::app::AppState;
use crate::commands::{Command, FIELD_SAFE_IDS, Layout, MenuItem, parse_shortcut};
use crate::theme;
use crate::toolbox::Tool;

pub type CommandState<'a> = &'a dyn Fn(Command) -> (bool, Option<bool>);

/// In-window menu bar, used when the native one is unavailable.
pub fn menu_bar_ui(ui: &mut egui::Ui, layout: &Layout, cmd_state: CommandState, out: &mut Vec<Command>) {
    egui::MenuBar::new().ui(ui, |ui| {
        for menu in &layout.menus {
            ui.menu_button(&menu.label, |ui| menu_items_ui(ui, &menu.items, cmd_state, out));
        }
    });
}

fn menu_items_ui(ui: &mut egui::Ui, items: &[MenuItem], cmd_state: CommandState, out: &mut Vec<Command>) {
    for item in items {
        match item {
            MenuItem::Separator(_) => {
                ui.separator();
            }
            MenuItem::Submenu { label, items } => {
                if items.is_empty() {
                    ui.add_enabled(false, egui::Button::new(label));
                } else {
                    ui.menu_button(label, |ui| menu_items_ui(ui, items, cmd_state, out));
                }
            }
            MenuItem::Command(c) => {
                let cmd = Command::from_id(&c.cmd);
                let (enabled, checked) = cmd.map(cmd_state).unwrap_or((false, None));
                let shortcut = c
                    .shortcut
                    .as_deref()
                    .and_then(parse_shortcut)
                    .map(|s| ui.ctx().format_shortcut(&s))
                    .unwrap_or_default();
                let mut button = egui::Button::new(&c.label).right_text(shortcut);
                if c.checkable {
                    button = button.selected(checked.unwrap_or(false));
                }
                if ui.add_enabled(enabled, button).clicked()
                    && let Some(cmd) = cmd
                {
                    out.push(cmd);
                }
            }
        }
    }
}

/// Shortcuts for the in-window menu. On macOS the native menu handles them instead.
/// Clipboard/undo keys and bare letters are left to the canvas key handler.
pub fn consume_menu_shortcuts(ctx: &egui::Context, layout: &Layout, cmd_state: CommandState, out: &mut Vec<Command>) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    for item in layout.command_items() {
        if FIELD_SAFE_IDS.contains(&item.cmd.as_str()) || item.shortcut.as_deref().is_some_and(|s| s.len() == 1) {
            continue;
        }
        let (Some(cmd), Some(text)) = (Command::from_id(&item.cmd), item.shortcut.as_deref()) else {
            continue;
        };
        let Some(shortcut) = parse_shortcut(text) else { continue };
        if cmd_state(cmd).0 && ctx.input_mut(|i| i.consume_shortcut(&shortcut)) {
            out.push(cmd);
        }
    }
}

/// Context-sensitive property bar: its controls depend on the active tool and selection.
pub fn property_bar_ui(ui: &mut egui::Ui, state: &mut AppState, out: &mut Vec<Command>) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_space(4.0);
        match state.tool {
            Tool::Pick if state.has_selection() => selection_bar(ui, state, out),
            Tool::Pick | Tool::FreehandPick | Tool::FreeTransform => page_bar(ui, state),
            Tool::Zoom | Tool::Pan => {
                zoom_field(ui, state);
                theme::vsep(ui);
                for (icon, tip, cmd) in [
                    (ICON_ZOOM_IN, "Zoom In", Command::ZoomIn),
                    (ICON_ZOOM_OUT, "Zoom Out", Command::ZoomOut),
                    (ICON_FIT_SCREEN, "Zoom to Page", Command::ZoomToPage),
                    (ICON_ASPECT_RATIO, "Actual Size", Command::ZoomActualSize),
                ] {
                    if icon_button(ui, icon, tip, true).clicked() {
                        out.push(cmd);
                    }
                }
            }
            Tool::Polygon => {
                let mut sides = state.tools.settings.polygon_sides as f64;
                if number_row(ui, Some(ICON_PENTAGON), "Points or sides", &mut sides, "", 3.0..=100.0, 1.0, 0) {
                    state.tools.settings.polygon_sides = sides as u32;
                }
            }
            Tool::Star => {
                let mut points = state.tools.settings.star_points as f64;
                if number_row(ui, Some(ICON_STAR), "Points or sides", &mut points, "", 3.0..=100.0, 1.0, 0) {
                    state.tools.settings.star_points = points as u32;
                }
                theme::vsep(ui);
                let mut sharp = (1.0 - state.tools.settings.star_inner_ratio) * 100.0;
                if number_row(ui, Some(ICON_CHANGE_HISTORY), "Sharpness", &mut sharp, "", 1.0..=99.0, 1.0, 0) {
                    state.tools.settings.star_inner_ratio = 1.0 - sharp / 100.0;
                }
            }
            Tool::Shape if state.selection.len() == 1 => {
                let n = state.tools.nodes.len();
                ui.label(egui::RichText::new(format!("{n} node(s) selected")).color(theme::TEXT_DIM));
                theme::vsep(ui);
                if icon_button(ui, ICON_REMOVE, "Delete nodes (Delete)", n > 0).clicked() {
                    out.push(Command::Delete);
                }
                if icon_button(ui, ICON_TIMELINE, "Convert to Curves", true).clicked() {
                    out.push(Command::ConvertToCurves);
                }
            }
            t if crate::tools::is_implemented(t) => {
                ui.label(egui::RichText::new(crate::tools::hint(state)).color(theme::TEXT_DIM));
            }
            _ => {
                ui.label(egui::RichText::new("This tool is not available yet.").color(theme::TEXT_DIM));
            }
        }
    });
}

/// Pick tool with nothing selected: page size, orientation, units, nudge, duplicate distance.
fn page_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let unit = state.doc.display_unit;
    let suffix = format!(" {}", unit.suffix());
    let page = state.doc.active_page_mut();
    let orientation = page.orientation();
    let current = PagePreset::matching(page.size).map(|p| p.name).unwrap_or("Custom");
    egui::ComboBox::from_id_salt("bar_page_preset")
        .selected_text(current)
        .width(150.0)
        .height(400.0)
        .show_ui(ui, |ui| {
            for preset in PAGE_PRESETS {
                if ui.selectable_label(current == preset.name, preset.name).clicked() {
                    page.size = preset.size(orientation);
                }
            }
        })
        .response
        .on_hover_text("Page size");
    theme::vsep(ui);

    let (mut w, mut h) = (unit.from_points(page.size.width), unit.from_points(page.size.height));
    let changed = stacked(ui, |ui| {
        let a = number_row(ui, Some(ICON_WIDTH), "Page width", &mut w, &suffix, 1.0..=f64::MAX, 1.0, 3);
        let b = number_row(ui, Some(ICON_HEIGHT), "Page height", &mut h, &suffix, 1.0..=f64::MAX, 1.0, 3);
        a || b
    });
    if changed {
        page.size = vs_core::kurbo::Size::new(unit.to_points(w), unit.to_points(h));
    }
    theme::vsep(ui);

    let portrait = orientation == Orientation::Portrait;
    if let Some(i) = segmented(ui, "orientation", &[(ICON_CROP_PORTRAIT, "Portrait", portrait), (ICON_CROP_LANDSCAPE, "Landscape", !portrait)]) {
        page.set_orientation(if i == 0 { Orientation::Portrait } else { Orientation::Landscape });
    }
    theme::vsep(ui);

    ui.label("Units:");
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = theme::POPUP;
        w.inactive.bg_fill = theme::POPUP;
        crate::dockers::unit_combo(ui, "bar_units", &mut state.doc.display_unit);
    });
    theme::vsep(ui);

    let mut nudge = unit.from_points(state.prefs.nudge);
    if number_row(ui, Some(ICON_OPEN_WITH), "Nudge distance (arrow keys)", &mut nudge, &suffix, 0.001..=f64::MAX, 1.0, 3) {
        state.prefs.nudge = unit.to_points(nudge);
    }
    theme::vsep(ui);

    let [mut dx, mut dy] = state.prefs.duplicate_offset.map(|v| unit.from_points(v));
    let changed = stacked(ui, |ui| {
        let a = number_row(ui, Some(ICON_CONTENT_COPY), "Duplicate distance, horizontal", &mut dx, &suffix, f64::MIN..=f64::MAX, 1.0, 3);
        let b = number_row(ui, Some(ICON_CONTENT_COPY), "Duplicate distance, vertical", &mut dy, &suffix, f64::MIN..=f64::MAX, 1.0, 3);
        a || b
    });
    if changed {
        state.prefs.duplicate_offset = [unit.to_points(dx), unit.to_points(dy)];
    }
}

/// Pick tool with a selection: position, size, rotate, mirror, curves.
fn selection_bar(ui: &mut egui::Ui, state: &mut AppState, out: &mut Vec<Command>) {
    let ids = state.selection.clone();
    let unit = state.doc.display_unit;
    let suffix = format!(" {}", unit.suffix());
    let Some(b) = state.doc.active_page().bounds_of(&ids) else { return };
    let (mut x, mut y) = (unit.from_points(b.x0), unit.from_points(b.y0));
    let (mut w, mut h) = (unit.from_points(b.width()), unit.from_points(b.height()));
    let (mx, my) = stacked(ui, |ui| {
        let a = number_row(ui, None, "X: object position", &mut x, &suffix, f64::MIN..=f64::MAX, 1.0, 3);
        let b = number_row(ui, None, "Y: object position", &mut y, &suffix, f64::MIN..=f64::MAX, 1.0, 3);
        (a, b)
    });
    theme::vsep(ui);
    let (mw, mh) = stacked(ui, |ui| {
        let a = number_row(ui, Some(ICON_WIDTH), "Object width", &mut w, &suffix, 0.001..=f64::MAX, 1.0, 3);
        let b = number_row(ui, Some(ICON_HEIGHT), "Object height", &mut h, &suffix, 0.001..=f64::MAX, 1.0, 3);
        (a, b)
    });
    if mx || my {
        state.doc.move_objects(&ids, vs_core::kurbo::Vec2::new(unit.to_points(x) - b.x0, unit.to_points(y) - b.y0));
    } else if mw || mh {
        let sx = if b.width() > 1e-9 { unit.to_points(w) / b.width() } else { 1.0 };
        let sy = if b.height() > 1e-9 { unit.to_points(h) / b.height() } else { 1.0 };
        let o = b.origin().to_vec2();
        let t = vs_core::kurbo::Affine::translate(o) * vs_core::kurbo::Affine::scale_non_uniform(sx, sy) * vs_core::kurbo::Affine::translate(-o);
        state.doc.transform_objects(&ids, t);
    }
    theme::vsep(ui);
    if icon_button(ui, ICON_ROTATE_LEFT, "Rotate 90° left", true).clicked() {
        crate::panels::rotate(state, -90.0);
    }
    if icon_button(ui, ICON_ROTATE_RIGHT, "Rotate 90° right", true).clicked() {
        crate::panels::rotate(state, 90.0);
    }
    theme::vsep(ui);
    if icon_button(ui, ICON_SWAP_HORIZ, "Mirror horizontally", true).clicked() {
        crate::panels::mirror(state, true);
    }
    if icon_button(ui, ICON_SWAP_VERT, "Mirror vertically", true).clicked() {
        crate::panels::mirror(state, false);
    }
    theme::vsep(ui);
    if icon_button(ui, ICON_TIMELINE, "Convert to Curves", true).clicked() {
        out.push(Command::ConvertToCurves);
    }
}

fn zoom_field(ui: &mut egui::Ui, state: &mut AppState) {
    let mut percent = state.viewport.zoom_percent();
    let center = vs_core::kurbo::Point::new(state.canvas_size.x as f64 / 2.0, state.canvas_size.y as f64 / 2.0);
    if number_row(ui, Some(ICON_ZOOM_IN), "Zoom level", &mut percent, "%", MIN_ZOOM_PERCENT..=MAX_ZOOM_PERCENT, 10.0, 0) {
        state.viewport.set_zoom_percent(percent, center);
    }
}

/// Two rows on top of each other, as the reference app stacks X/Y and W/H.
fn stacked<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        add(ui)
    })
    .inner
}

/// Icon, number field and stepper. Returns true if the value changed.
#[allow(clippy::too_many_arguments)]
fn number_row(
    ui: &mut egui::Ui,
    icon: Option<MaterialIcon>,
    tip: &str,
    value: &mut f64,
    suffix: &str,
    range: std::ops::RangeInclusive<f64>,
    step: f64,
    decimals: usize,
) -> bool {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        match icon {
            Some(icon) => ui.label(icon.rich_text().size(16.0).color(theme::TEXT_DIM)),
            None => ui.add_sized([16.0, 20.0], egui::Label::new(egui::RichText::new(tip.chars().next().unwrap_or(' ').to_string()).color(theme::TEXT_DIM))),
        };
        let field = egui::DragValue::new(value)
            .speed(0.1)
            .max_decimals(decimals)
            .suffix(suffix)
            .range(range.clone());
        let mut changed = ui.add_sized([92.0, 20.0], field).on_hover_text(tip).changed();
        changed |= stepper(ui, value, step, &range);
        changed
    })
    .inner
}

/// Small up/down stepper next to a number field.
fn stepper(ui: &mut egui::Ui, value: &mut f64, step: f64, range: &std::ops::RangeInclusive<f64>) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(15.0, 20.0), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, theme::FIELD);
    let mid = rect.center().y;
    for (icon, y) in [(ICON_KEYBOARD_ARROW_UP, mid - 4.5), (ICON_KEYBOARD_ARROW_DOWN, mid + 4.5)] {
        painter.text(egui::pos2(rect.center().x, y), egui::Align2::CENTER_CENTER, <&str>::from(icon), egui::FontId::new(13.0, icon.font_family()), theme::TEXT_DIM);
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let delta = if p.y < mid { step } else { -step };
        *value = (*value + delta).clamp(*range.start(), *range.end());
        return true;
    }
    false
}

/// A row of icon segments in one rounded box; the selected one is lighter.
fn segmented(ui: &mut egui::Ui, id: &str, segments: &[(MaterialIcon, &str, bool)]) -> Option<usize> {
    let seg = vec2(28.0, 22.0);
    let (rect, _) = ui.allocate_exact_size(vec2(seg.x * segments.len() as f32 + 4.0, seg.y + 4.0), Sense::hover());
    ui.painter().rect_filled(rect, 6.0, theme::SUNKEN);
    let mut picked = None;
    for (i, (icon, tip, on)) in segments.iter().enumerate() {
        let r = egui::Rect::from_min_size(rect.min + vec2(2.0 + seg.x * i as f32, 2.0), seg);
        let resp = ui.interact(r, ui.scope_id().with((id, i)), Sense::click()).on_hover_text(*tip);
        if *on {
            ui.painter().rect_filled(r, 4.0, theme::SEGMENT_ON);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 4.0, Color32::from_gray(0x30));
        }
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, <&str>::from(*icon), egui::FontId::new(17.0, icon.font_family()), theme::TEXT);
        if resp.clicked() {
            picked = Some(i);
        }
    }
    picked
}

/// Square icon button that only shows a background on hover.
fn icon_button(ui: &mut egui::Ui, icon: MaterialIcon, tip: &str, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 5.0, theme::FIELD);
    }
    let color = if enabled { theme::TEXT } else { theme::TEXT_DISABLED };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, <&str>::from(icon), egui::FontId::new(19.0, icon.font_family()), color);
    resp.on_hover_text(tip)
}

pub fn status_bar_ui(ui: &mut egui::Ui, state: &AppState) {
    ui.horizontal(|ui| {
        let hint = match &state.status {
            Some(msg) => msg.clone(),
            None => crate::tools::hint(state),
        };
        ui.label(egui::RichText::new(hint).small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(format!("{:.0}%", state.viewport.zoom_percent())).small().monospace());
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "{} of {}",
                    state.doc.active_page_index() + 1,
                    state.doc.page_count()
                ))
                .small(),
            );
            ui.separator();
            let unit = state.doc.display_unit;
            let pos = match state.cursor_doc {
                Some(p) => format!(
                    "( {:.3}, {:.3} ) {}",
                    unit.from_points(p.x),
                    unit.from_points(p.y),
                    unit.suffix()
                ),
                None => String::new(),
            };
            ui.label(egui::RichText::new(pos).small().monospace());
        });
    });
}

/// Default palette: a grey ramp, then hues. Values are our own, not the reference app's.
const PALETTE: &[(&str, [u8; 3])] = &[
    ("Black", [0x1A, 0x1A, 0x1A]),
    ("90% Black", [0x3C, 0x3C, 0x3B]),
    ("80% Black", [0x57, 0x57, 0x56]),
    ("70% Black", [0x6F, 0x6F, 0x6E]),
    ("60% Black", [0x87, 0x87, 0x87]),
    ("50% Black", [0x9D, 0x9D, 0x9C]),
    ("40% Black", [0xB2, 0xB2, 0xB2]),
    ("30% Black", [0xC6, 0xC6, 0xC6]),
    ("20% Black", [0xDA, 0xDA, 0xDA]),
    ("10% Black", [0xED, 0xED, 0xED]),
    ("White", [0xFF, 0xFF, 0xFF]),
    ("Navy Blue", [0x2E, 0x31, 0x92]),
    ("Cyan", [0x00, 0xA0, 0xE3]),
    ("Green", [0x00, 0x96, 0x40]),
    ("Yellow", [0xFF, 0xED, 0x00]),
    ("Red", [0xE3, 0x06, 0x13]),
    ("Magenta", [0xE5, 0x00, 0x7E]),
    ("Purple", [0x93, 0x27, 0x8F]),
    ("Orange", [0xF3, 0x92, 0x00]),
    ("Pink", [0xF1, 0x9D, 0xB2]),
    ("Brown", [0x7B, 0x5B, 0x47]),
    ("Lavender", [0xC5, 0xBF, 0xE0]),
    ("Light Purple", [0xA2, 0x8F, 0xC6]),
    ("Periwinkle", [0x6F, 0x84, 0xC2]),
    ("Violet", [0x6A, 0x5B, 0xA6]),
    ("Dusk", [0x7E, 0x7A, 0xA6]),
    ("Slate Blue", [0x4A, 0x55, 0x82]),
    ("Midnight", [0x3B, 0x3B, 0x63]),
    ("Steel", [0x5E, 0x6E, 0x8C]),
    ("Blue", [0x00, 0x71, 0xBC]),
    ("Sky Blue", [0x8E, 0xCF, 0xF1]),
    ("Powder Blue", [0xA5, 0xBE, 0xD0]),
    ("Blue Grey", [0x84, 0x9B, 0xAE]),
    ("Slate", [0x5F, 0x73, 0x82]),
    ("Charcoal", [0x3E, 0x4B, 0x55]),
    ("Pine", [0x3C, 0x6B, 0x63]),
    ("Jade", [0x2F, 0x82, 0x6A]),
    ("Sea Green", [0x5C, 0x9C, 0x8E]),
    ("Mint", [0x9C, 0xCF, 0xB6]),
    ("Lime", [0x95, 0xC1, 0x1F]),
    ("Olive", [0x8A, 0x8A, 0x2E]),
    ("Gold", [0xD9, 0xA4, 0x1E]),
    ("Sand", [0xE6, 0xCF, 0x9C]),
    ("Peach", [0xF7, 0xB9, 0x8B]),
    ("Coral", [0xEE, 0x75, 0x5F]),
    ("Brick", [0xA6, 0x3A, 0x2B]),
    ("Wine", [0x7D, 0x1F, 0x3C]),
];

const SWATCH: f32 = 16.0;

/// Vertical colour palette at the right edge. Click sets the fill of the selection,
/// right-click its outline; the crossed swatch at the top removes them.
pub fn palette_ui(ui: &mut egui::Ui, state: &mut AppState) {
    let full = ui.max_rect();
    let x = full.center().x - SWATCH / 2.0;
    let mut y = full.min.y + 6.0;

    // No colour: white with a red diagonal.
    let r = egui::Rect::from_min_size(egui::pos2(x, y), Vec2::splat(SWATCH));
    let resp = ui.interact(r, ui.scope_id().with("no_color"), Sense::click());
    ui.painter().rect_filled(r, 3.0, Color32::WHITE);
    ui.painter().line_segment([r.left_bottom() + vec2(2.0, -2.0), r.right_top() + vec2(-2.0, 2.0)], Stroke::new(1.5, Color32::from_rgb(0xE0, 0x1E, 0x1E)));
    let resp = resp.on_hover_text("No color\nClick: remove fill · Right-click: remove outline");
    if resp.clicked() {
        set_style(state, |s| s.fill = None);
    }
    if resp.secondary_clicked() {
        set_style(state, |s| s.outline = None);
    }
    y += SWATCH + 6.0;

    // Swatches scroll; the chevron at the bottom scrolls down, as in the reference app.
    let chevron = egui::Rect::from_min_max(egui::pos2(full.min.x, full.max.y - 22.0), full.max);
    let list = egui::Rect::from_min_max(egui::pos2(full.min.x, y), egui::pos2(full.max.x, chevron.min.y));
    let offset_id = ui.scope_id().with("palette_offset");
    let max_offset = (PALETTE.len() as f32 * SWATCH - list.height()).max(0.0);
    let mut offset: f32 = ui.data(|d| d.get_temp(offset_id)).unwrap_or(0.0);
    if ui.rect_contains_pointer(list) {
        offset -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    let resp = ui.interact(chevron, ui.scope_id().with("palette_more"), Sense::click());
    if resp.clicked() {
        offset += SWATCH * 5.0;
    }
    offset = offset.clamp(0.0, max_offset);
    ui.data_mut(|d| d.insert_temp(offset_id, offset));
    ui.painter().text(
        chevron.center(),
        egui::Align2::CENTER_CENTER,
        <&str>::from(ICON_EXPAND_MORE),
        egui::FontId::new(16.0, ICON_EXPAND_MORE.font_family()),
        if offset < max_offset { theme::TEXT } else { theme::TEXT_DISABLED },
    );

    let painter = ui.painter().with_clip_rect(list);
    for (i, (name, [r, g, b])) in PALETTE.iter().enumerate() {
        let rect = egui::Rect::from_min_size(egui::pos2(x, list.min.y + i as f32 * SWATCH - offset), Vec2::splat(SWATCH));
        if !list.intersects(rect) {
            continue;
        }
        painter.rect_filled(rect, 0.0, Color32::from_rgb(*r, *g, *b));
        let resp = ui.interact(rect.intersect(list), ui.scope_id().with(("swatch", i)), Sense::click());
        if resp.hovered() {
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
        }
        let resp = resp.on_hover_text(format!("{name}\nClick: fill · Right-click: outline"));
        let color = vs_core::Rgba::rgb(*r, *g, *b);
        if resp.clicked() {
            set_style(state, |s| s.fill = Some(color));
        }
        if resp.secondary_clicked() {
            set_style(state, |s| match &mut s.outline {
                Some(o) => o.color = color,
                None => s.outline = Some(vs_core::Outline { color, width: 0.5 }),
            });
        }
    }
}

fn set_style(state: &mut AppState, edit: impl Fn(&mut vs_core::Style)) {
    let ids = state.selection.clone();
    let page = state.doc.active_page_mut();
    for id in ids {
        if let Some(o) = page.object_mut(id) {
            edit(&mut o.style);
        }
    }
}
