//! Title bar and main toolbar, drawn by the app inside a full-size content view.
//!
//! The main toolbar shows a caption under each item.
//! Item order: Zoom, View Modes | Group, Ungroup | Lock, Unlock |
//! Alignment | Mirror | Arrange | View (Rulers, Grid, Guidelines) | Snap To | AI Generate |
//! Get More | Show/Hide Inspector. AI Generate and Get More are online services and are
//! left out.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use egui_material_icons::{MaterialIcon, icons::*};
use vs_core::{Align, Order};

use crate::app::AppState;
use crate::commands::Command;
use crate::dockers::DockerKind;
use crate::theme;

const PILL_TOP: f32 = 31.0;
const PILL_HEIGHT: f32 = 28.0;
const CAPTION_Y: f32 = 69.0;
const ITEM_GAP: f32 = 10.0;
const MIN_GROUP_GAP: f32 = 14.0;
const CAPTION: Color32 = Color32::from_gray(0xC8);
/// Leaves room for the window buttons (close, minimise, zoom).
#[cfg(target_os = "macos")]
const TITLE_X: f32 = 84.0;
#[cfg(not(target_os = "macos"))]
const TITLE_X: f32 = 12.0;

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Zoom,
    ViewModes,
    Group,
    Ungroup,
    Lock,
    Unlock,
    Alignment,
    MirrorH,
    MirrorV,
    Arrange(Order),
    Rulers,
    Grid,
    Guidelines,
    SnapTo,
}

enum Face {
    Text(String),
    Icon(MaterialIcon),
    /// Drawn by hand: the icon font has no magnet.
    Magnet,
}

struct Segment {
    face: Face,
    action: Action,
    tip: &'static str,
    enabled: bool,
    checked: bool,
    dropdown: bool,
}

struct Item {
    caption: &'static str,
    seg_width: f32,
    segments: Vec<Segment>,
}

impl Item {
    fn width(&self) -> f32 {
        self.seg_width * self.segments.len() as f32
    }
}

fn seg(face: Face, action: Action, tip: &'static str, enabled: bool) -> Segment {
    Segment { face, action, tip, enabled, checked: false, dropdown: false }
}

fn items(state: &AppState) -> Vec<Vec<Item>> {
    let sel = state.has_selection();
    let dropdown = |mut s: Segment| {
        s.dropdown = true;
        s
    };
    let checked = |mut s: Segment, on: bool| {
        s.checked = on;
        s
    };
    let zoom = format!("{:.0}%", state.viewport.zoom_percent());
    vec![
        vec![
            Item { caption: "Zoom", seg_width: 58.0, segments: vec![dropdown(seg(Face::Text(zoom), Action::Zoom, "Zoom levels", true))] },
            Item {
                caption: "View Modes",
                seg_width: 92.0,
                segments: vec![dropdown(seg(Face::Text("Enhanced".into()), Action::ViewModes, "View modes", true))],
            },
        ],
        vec![
            Item { caption: "Group", seg_width: 46.0, segments: vec![seg(Face::Icon(ICON_GROUP_WORK), Action::Group, "Group (not available yet)", false)] },
            Item { caption: "Ungroup", seg_width: 46.0, segments: vec![seg(Face::Icon(ICON_WORKSPACES), Action::Ungroup, "Ungroup (not available yet)", false)] },
        ],
        vec![
            Item { caption: "Lock", seg_width: 46.0, segments: vec![seg(Face::Icon(ICON_LOCK), Action::Lock, "Lock (not available yet)", false)] },
            Item { caption: "Unlock", seg_width: 46.0, segments: vec![seg(Face::Icon(ICON_LOCK_OPEN_RIGHT), Action::Unlock, "Unlock (not available yet)", false)] },
        ],
        vec![Item {
            caption: "Alignment",
            seg_width: 48.0,
            segments: vec![dropdown(seg(Face::Icon(ICON_ALIGN_HORIZONTAL_LEFT), Action::Alignment, "Align and distribute", true))],
        }],
        vec![Item {
            caption: "Mirror",
            seg_width: 46.0,
            segments: vec![
                seg(Face::Icon(ICON_SWAP_HORIZ), Action::MirrorH, "Mirror horizontally", sel),
                seg(Face::Icon(ICON_SWAP_VERT), Action::MirrorV, "Mirror vertically", sel),
            ],
        }],
        vec![Item {
            caption: "Arrange",
            seg_width: 40.0,
            segments: vec![
                seg(Face::Icon(ICON_FLIP_TO_FRONT), Action::Arrange(Order::ToFront), "To front of layer", sel),
                seg(Face::Icon(ICON_ARROW_UPWARD), Action::Arrange(Order::Forward), "Forward one", sel),
                seg(Face::Icon(ICON_ARROW_DOWNWARD), Action::Arrange(Order::Backward), "Back one", sel),
                seg(Face::Icon(ICON_FLIP_TO_BACK), Action::Arrange(Order::ToBack), "To back of layer", sel),
            ],
        }],
        vec![Item {
            caption: "View",
            seg_width: 46.0,
            segments: vec![
                checked(seg(Face::Icon(ICON_STRAIGHTEN), Action::Rulers, "Rulers", true), state.prefs.show_rulers),
                seg(Face::Icon(ICON_GRID_ON), Action::Grid, "Grid (not available yet)", false),
                seg(Face::Icon(ICON_GRID_GUIDES), Action::Guidelines, "Guidelines (not available yet)", false),
            ],
        }],
        vec![Item {
            caption: "Snap To",
            seg_width: 46.0,
            segments: vec![dropdown(seg(Face::Magnet, Action::SnapTo, "Snap to", true))],
        }],
    ]
}

/// Draw the title bar and toolbar. Clicks become commands or direct edits of `state`.
pub fn titlebar_ui(ui: &mut egui::Ui, state: &mut AppState, out: &mut Vec<Command>) {
    let rect = ui.max_rect();
    let ctx = ui.ctx().clone();

    // Empty areas move the window; a double-click zooms it, like a native title bar.
    let background = ui.interact(rect, ui.scope_id().with("titlebar_drag"), Sense::click_and_drag());
    if background.drag_started_by(egui::PointerButton::Primary) {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if background.double_clicked() {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }

    let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
    let painter = ui.painter();
    let name = match &state.file_path {
        Some(p) => p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        None => state.doc.title.clone(),
    };
    let title_color = if focused { theme::TEXT } else { theme::TEXT_DISABLED };
    let title = painter.text(pos2(rect.min.x + TITLE_X, rect.min.y + 15.0), Align2::LEFT_CENTER, &name, FontId::proportional(14.0), title_color);
    if state.history.is_dirty() {
        painter.text(
            pos2(title.max.x + 6.0, rect.min.y + 15.0),
            Align2::LEFT_CENTER,
            "— Edited",
            FontId::proportional(13.0),
            theme::TEXT_DIM,
        );
    }

    // Dockers toggle at the right edge (Show/Hide Inspector in the reference app).
    let dockers_center = pos2(rect.max.x - 30.0, rect.min.y + PILL_TOP + PILL_HEIGHT / 2.0 + 1.0);
    let dockers_rect = Rect::from_center_size(dockers_center, Vec2::splat(36.0));
    let resp = ui.interact(dockers_rect, ui.scope_id().with("dockers_toggle"), Sense::click());
    let painter = ui.painter();
    painter.circle_filled(dockers_center, 18.0, if resp.hovered() { Color32::from_gray(0x2A) } else { theme::PILL });
    let shown = state.dockers.has_content();
    painter.text(
        dockers_center,
        Align2::CENTER_CENTER,
        <&str>::from(ICON_RIGHT_PANEL_OPEN),
        FontId::new(20.0, ICON_RIGHT_PANEL_OPEN.font_family()),
        if shown { theme::TEXT } else { theme::TEXT_DIM },
    );
    if resp.on_hover_text(if shown { "Hide dockers" } else { "Show dockers" }).clicked() {
        if shown {
            state.dockers.visible = false;
        } else {
            state.dockers.show(state.dockers.active.unwrap_or(DockerKind::Properties));
        }
    }

    // Toolbar groups, spread with flexible gaps like the native toolbar.
    let groups = items(state);
    let fixed: f32 = groups
        .iter()
        .map(|g| g.iter().map(Item::width).sum::<f32>() + ITEM_GAP * (g.len() as f32 - 1.0))
        .sum();
    let left = rect.min.x + 8.0;
    let right = dockers_rect.min.x - 16.0;
    let gaps = groups.len() as f32 - 1.0;
    let gap = ((right - left - fixed) / gaps).clamp(MIN_GROUP_GAP, 60.0);

    let mut x = left;
    let mut clicked: Vec<(Action, egui::Response)> = Vec::new();
    for (gi, group) in groups.iter().enumerate() {
        for (ii, item) in group.iter().enumerate() {
            let pill = Rect::from_min_size(pos2(x, rect.min.y + PILL_TOP), vec2(item.width(), PILL_HEIGHT));
            if pill.max.x > right {
                break; // Narrow window: items past the edge are hidden (as a native toolbar does).
            }
            for r in item_ui(ui, item, pill, (gi, ii)) {
                clicked.push(r);
            }
            // Captions of button groups stay bright; a single disabled button dims its caption.
            let bright = item.segments.len() > 1 || item.segments.iter().any(|s| s.enabled);
            ui.painter().text(
                pos2(pill.center().x, rect.min.y + CAPTION_Y),
                Align2::CENTER_CENTER,
                item.caption,
                FontId::proportional(11.5),
                if bright { CAPTION } else { theme::TEXT_DISABLED },
            );
            x = pill.max.x + ITEM_GAP;
        }
        x += gap - ITEM_GAP;
    }

    for (action, resp) in clicked {
        act(state, action, &resp, out);
    }

    // Line under the toolbar.
    ui.painter().hline(rect.x_range(), rect.max.y - 0.5, Stroke::new(1.0, theme::SUNKEN));
}

/// Paint one pill and return the response of each segment (for click handling).
fn item_ui(ui: &mut egui::Ui, item: &Item, pill: Rect, key: (usize, usize)) -> Vec<(Action, egui::Response)> {
    let radius = (PILL_HEIGHT / 2.0) as u8;
    ui.painter().rect_filled(pill, radius, theme::PILL);
    let n = item.segments.len();
    let mut out = Vec::with_capacity(n);
    for (i, s) in item.segments.iter().enumerate() {
        let r = Rect::from_min_size(pos2(pill.min.x + item.seg_width * i as f32, pill.min.y), vec2(item.seg_width, PILL_HEIGHT));
        let sense = if s.enabled { Sense::click() } else { Sense::hover() };
        let resp = ui.interact(r, ui.scope_id().with(("tb", key, i)), sense).on_hover_text(s.tip);
        let corners = CornerRadius {
            nw: if i == 0 { radius } else { 0 },
            sw: if i == 0 { radius } else { 0 },
            ne: if i + 1 == n { radius } else { 0 },
            se: if i + 1 == n { radius } else { 0 },
        };
        let painter = ui.painter();
        if s.checked {
            painter.rect_filled(r, corners, theme::ACCENT);
        } else if s.enabled && resp.hovered() {
            painter.rect_filled(r, corners, Color32::from_gray(0x2A));
        }
        if i > 0 {
            painter.vline(r.min.x, r.y_range().shrink(7.0), Stroke::new(1.0, Color32::from_gray(0x33)));
        }
        let color = if !s.enabled {
            theme::TEXT_DISABLED
        } else if s.checked || resp.hovered() {
            Color32::WHITE
        } else {
            theme::TEXT
        };
        let chevron_w = if s.dropdown { 12.0 } else { 0.0 };
        let center = r.center() - vec2(chevron_w / 2.0, 0.0);
        match &s.face {
            Face::Text(t) => {
                painter.text(center, Align2::CENTER_CENTER, t, FontId::proportional(14.0), color);
            }
            Face::Icon(icon) => {
                painter.text(center, Align2::CENTER_CENTER, <&str>::from(*icon), FontId::new(20.0, icon.font_family()), color);
            }
            Face::Magnet => magnet(painter, center, color),
        }
        if s.dropdown {
            let face_w = match &s.face {
                Face::Text(t) => ui.fonts_mut(|f| f.layout_no_wrap(t.clone(), FontId::proportional(14.0), color).size().x),
                Face::Icon(_) | Face::Magnet => 20.0,
            };
            let at: Pos2 = pos2(center.x + face_w / 2.0 + 7.0, r.center().y + 1.0);
            ui.painter().text(at, Align2::CENTER_CENTER, <&str>::from(ICON_EXPAND_MORE), FontId::new(15.0, ICON_EXPAND_MORE.font_family()), color);
        }
        out.push((s.action, resp));
    }
    out
}

fn act(state: &mut AppState, action: Action, resp: &egui::Response, out: &mut Vec<Command>) {
    let sel = state.has_selection();
    match action {
        Action::Zoom => {
            egui::Popup::menu(resp).show(|ui| {
                for (label, cmd) in [
                    ("Zoom In", Command::ZoomIn),
                    ("Zoom Out", Command::ZoomOut),
                    ("Zoom to Page", Command::ZoomToPage),
                    ("Actual Size", Command::ZoomActualSize),
                ] {
                    if ui.button(label).clicked() {
                        out.push(cmd);
                    }
                }
                ui.separator();
                let center = vs_core::kurbo::Point::new(state.canvas_size.x as f64 / 2.0, state.canvas_size.y as f64 / 2.0);
                for p in [25.0, 50.0, 75.0, 100.0, 200.0, 400.0, 800.0] {
                    if ui.button(format!("{p}%")).clicked() {
                        state.viewport.set_zoom_percent(p, center);
                    }
                }
            });
        }
        Action::ViewModes => {
            egui::Popup::menu(resp).show(|ui| {
                for mode in ["Wireframe", "Normal"] {
                    ui.add_enabled(false, egui::Button::new(mode));
                }
                let _ = ui.add(egui::Button::new("Enhanced").selected(true));
                ui.add_enabled(false, egui::Button::new("Pixels"));
            });
        }
        Action::Alignment => {
            egui::Popup::menu(resp).show(|ui| {
                for (label, key, a) in [
                    ("Align Left", "L", Align::Left),
                    ("Align Right", "R", Align::Right),
                    ("Align Top", "T", Align::Top),
                    ("Align Bottom", "B", Align::Bottom),
                    ("Align Centers Horizontally", "E", Align::CenterHorizontal),
                    ("Align Centers Vertically", "C", Align::CenterVertical),
                ] {
                    if ui.add_enabled(sel, egui::Button::new(label).right_text(key)).clicked() {
                        out.push(Command::Align(a));
                    }
                }
                ui.separator();
                if ui.add_enabled(sel, egui::Button::new("Center to Page").right_text("P")).clicked() {
                    out.push(Command::CenterToPage);
                }
                ui.separator();
                if ui.button("Align and Distribute…").clicked() {
                    state.dockers.show(DockerKind::AlignDistribute);
                }
            });
        }
        Action::SnapTo => {
            egui::Popup::menu(resp).show(|ui| {
                ui.label(egui::RichText::new("Not available yet").weak().small());
                for label in ["Pixels", "Document Grid", "Guidelines", "Objects", "Page"] {
                    ui.add_enabled(false, egui::Button::new(label));
                }
            });
        }
        _ if !resp.clicked() => {}
        Action::MirrorH => crate::panels::mirror(state, true),
        Action::MirrorV => crate::panels::mirror(state, false),
        Action::Arrange(order) => out.push(Command::Order(order)),
        Action::Rulers => out.push(Command::ToggleRulers),
        Action::Group | Action::Ungroup | Action::Lock | Action::Unlock | Action::Grid | Action::Guidelines => {}
    }
}

/// A U-shaped magnet, open at the top, with pole marks.
fn magnet(painter: &egui::Painter, c: Pos2, color: Color32) {
    let (r, top, mid) = (6.0, c.y - 7.0, c.y + 1.0);
    let mut points = vec![pos2(c.x - r, top), pos2(c.x - r, mid)];
    for i in 1..16 {
        let a = std::f32::consts::PI * i as f32 / 16.0;
        points.push(pos2(c.x - r * a.cos(), mid + r * a.sin()));
    }
    points.extend([pos2(c.x + r, mid), pos2(c.x + r, top)]);
    let stroke = Stroke::new(2.0, color);
    painter.add(egui::Shape::line(points, stroke));
    for x in [c.x - r, c.x + r] {
        painter.hline((x - 2.5)..=(x + 2.5), top + 3.0, Stroke::new(1.5, color));
    }
}
