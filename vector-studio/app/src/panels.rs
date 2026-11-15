//! Docker contents that work on the selection: Properties, Transform, Align, Objects.

use egui_material_icons::icons::*;
use vs_core::kurbo::{Affine, Vec2};
use vs_core::{Align, Order, Rgba, Shape, Style};

use crate::app::AppState;
use crate::commands::Command;

fn to_arr(c: Rgba) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

fn from_arr([r, g, b, a]: [u8; 4]) -> Rgba {
    Rgba { r, g, b, a }
}

fn grid(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([8.0, 6.0]).show(ui, add);
}

pub fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(egui::RichText::new(title).strong())
        .default_open(true)
        .show(ui, add);
}

fn length(value: &mut f64, unit: vs_core::Unit) -> egui::DragValue<'_> {
    egui::DragValue::new(value).speed(unit.step() / 2.0).max_decimals(unit.decimals()).suffix(format!(" {}", unit.suffix()))
}

/// Fill, outline and shape settings of the selection. Edits apply to every selected object.
pub fn object_properties_ui(ui: &mut egui::Ui, state: &mut AppState) {
    let ids = state.selection.clone();
    let unit = state.doc.display_unit;
    let Some(first) = state.doc.active_page().object(ids[0]).cloned() else { return };
    let title = if ids.len() == 1 { first.label() } else { format!("{} objects", ids.len()) };
    ui.label(egui::RichText::new(title).strong());
    ui.add_space(4.0);

    let locked = first.locked;
    if locked {
        ui.label(egui::RichText::new(format!("{} Locked: unlock it to edit.", <&str>::from(ICON_LOCK))).weak());
    }
    let mut style = first.shown_style().clone();
    let before = style.clone();
    let closed = !matches!(first.shape, Shape::Curve { .. }) || first.shape.is_closed();
    ui.add_enabled_ui(!locked, |ui| {
        section(ui, "Fill", |ui| {
            ui.horizontal(|ui| {
                let mut on = style.fill.is_some();
                if ui.checkbox(&mut on, "Uniform fill").changed() {
                    style.fill = on.then_some(Rgba::BLACK);
                }
                if let Some(fill) = &mut style.fill {
                    let mut c = to_arr(*fill);
                    if ui.color_edit_button_srgba_unmultiplied(&mut c).changed() {
                        *fill = from_arr(c);
                    }
                }
            });
            if !closed {
                ui.label(egui::RichText::new("Open curves are not filled.").weak().small());
            }
        });
        section(ui, "Outline", |ui| crate::outline::outline_editor(ui, &mut style));
    });
    if let Some(change) = crate::outline::style_change(&before, &style) {
        state.doc.restyle_objects(&ids, change);
    }
    if locked {
        return;
    }

    if ids.len() != 1 {
        return;
    }
    let id = ids[0];
    section(ui, first.shape.kind_label(), |ui| {
        let page = state.doc.active_page_mut();
        let Some(obj) = page.object_mut(id) else { return };
        grid(ui, "shape_props", |ui| {
            ui.label("Name");
            let mut name = obj.name.clone().unwrap_or_default();
            if crate::a11y::add(ui, egui::TextEdit::singleline(&mut name).hint_text(obj.shape.kind_label()).desired_width(120.0), "Name (properties)").changed() {
                obj.name = (!name.trim().is_empty()).then_some(name);
            }
            ui.end_row();
            match &mut obj.shape {
                Shape::Rectangle { rect, corner_radius } => {
                    ui.label("Corner radius");
                    let mut r = unit.from_points(*corner_radius);
                    let max = unit.from_points(rect.width().min(rect.height()) / 2.0);
                    if crate::a11y::add(ui, length(&mut r, unit).range(0.0..=max), "Corner radius (properties)").changed() {
                        *corner_radius = unit.to_points(r);
                    }
                    ui.end_row();
                }
                Shape::Polygon { sides, .. } => {
                    ui.label("Sides");
                    crate::a11y::add(ui, egui::DragValue::new(sides).range(3..=100), "Sides (properties)");
                    ui.end_row();
                }
                Shape::Star { points, inner_ratio, .. } => {
                    ui.label("Points");
                    crate::a11y::add(ui, egui::DragValue::new(points).range(3..=100), "Points (properties)");
                    ui.end_row();
                    ui.label("Sharpness");
                    let mut sharp = (1.0 - *inner_ratio) * 100.0;
                    if crate::a11y::add(ui, egui::DragValue::new(&mut sharp).range(0.0..=99.0).max_decimals(0).suffix("%"), "Sharpness (properties)").changed() {
                        *inner_ratio = 1.0 - sharp / 100.0;
                    }
                    ui.end_row();
                }
                Shape::Curve { path } => {
                    ui.label("Nodes");
                    ui.label(vs_core::nodes::nodes(path).len().to_string());
                    ui.end_row();
                }
                Shape::Group { children } => {
                    ui.label("Objects");
                    ui.label(children.len().to_string());
                    ui.end_row();
                }
                Shape::Ellipse { .. } => {}
            }
        });
        if !matches!(obj.shape, Shape::Curve { .. } | Shape::Group { .. }) {
            ui.add_space(4.0);
            if ui.button("Convert to Curves").clicked() {
                obj.shape = Shape::Curve { path: obj.shape.to_path() };
            }
        }
    });
}

/// Position, size, rotation and mirroring of the selection bounds.
pub fn transform_ui(ui: &mut egui::Ui, state: &mut AppState) {
    let ids = state.selection.clone();
    let unit = state.doc.display_unit;
    let Some(b) = state.doc.active_page().bounds_of(&ids) else { return };

    let lock_id = ui.make_persistent_id("transform_lock_ratio");
    let mut lock = ui.data_mut(|d| *d.get_persisted_mut_or(lock_id, true));
    let angle_id = ui.make_persistent_id("transform_angle");
    let mut angle: f64 = ui.data_mut(|d| *d.get_persisted_mut_or(angle_id, 90.0));

    section(ui, "Position and size", |ui| {
        let (mut x, mut y) = (unit.from_points(b.x0), unit.from_points(b.y0));
        let (mut w, mut h) = (unit.from_points(b.width()), unit.from_points(b.height()));
        let mut moved = false;
        let mut sized = false;
        grid(ui, "transform_grid", |ui| {
            ui.label("X");
            moved |= crate::a11y::add(ui, length(&mut x, unit), "X (transform)").changed();
            ui.end_row();
            ui.label("Y");
            moved |= crate::a11y::add(ui, length(&mut y, unit), "Y (transform)").changed();
            ui.end_row();
            ui.label("Width");
            sized |= crate::a11y::add(ui, length(&mut w, unit).range(0.001..=f64::MAX), "Width (transform)").changed();
            ui.end_row();
            ui.label("Height");
            sized |= crate::a11y::add(ui, length(&mut h, unit).range(0.001..=f64::MAX), "Height (transform)").changed();
            ui.end_row();
        });
        ui.checkbox(&mut lock, "Proportional");
        ui.label(egui::RichText::new("X and Y are the top-left of the selection.").weak().small());
        if moved {
            let delta = Vec2::new(unit.to_points(x) - b.x0, unit.to_points(y) - b.y0);
            state.doc.move_objects(&ids, delta);
        } else if sized {
            let (nw, nh) = (unit.to_points(w), unit.to_points(h));
            let mut sx = if b.width() > 1e-9 { nw / b.width() } else { 1.0 };
            let mut sy = if b.height() > 1e-9 { nh / b.height() } else { 1.0 };
            if lock {
                // Follow whichever field changed.
                let s = if (sx - 1.0).abs() > (sy - 1.0).abs() { sx } else { sy };
                sx = s;
                sy = s;
            }
            let o = b.origin().to_vec2();
            let t = Affine::translate(o) * Affine::scale_non_uniform(sx, sy) * Affine::translate(-o);
            state.doc.transform_objects(&ids, t);
        }
    });

    section(ui, "Rotate", |ui| {
        ui.horizontal(|ui| {
            crate::a11y::add(ui, egui::DragValue::new(&mut angle).range(-360.0..=360.0).speed(1.0).suffix("°"), "Rotation angle");
            if ui.button("Apply").on_hover_text("Rotate counter-clockwise around the centre").clicked() {
                rotate(state, -angle);
            }
        });
        ui.horizontal(|ui| {
            if crate::a11y::icon_button(ui, ICON_ROTATE_LEFT.rich_text().size(18.0), "Rotate 90° left").clicked() {
                rotate(state, -90.0);
            }
            if crate::a11y::icon_button(ui, ICON_ROTATE_RIGHT.rich_text().size(18.0), "Rotate 90° right").clicked() {
                rotate(state, 90.0);
            }
        });
    });

    section(ui, "Mirror", |ui| {
        ui.horizontal(|ui| {
            if crate::a11y::icon_button(ui, ICON_SWAP_HORIZ.rich_text().size(18.0), "Mirror horizontally").clicked() {
                mirror(state, true);
            }
            if crate::a11y::icon_button(ui, ICON_SWAP_VERT.rich_text().size(18.0), "Mirror vertically").clicked() {
                mirror(state, false);
            }
        });
    });

    ui.data_mut(|d| {
        d.insert_persisted(lock_id, lock);
        d.insert_persisted(angle_id, angle);
    });
}

/// Rotate the selection by `degrees` clockwise on screen (y points down).
pub fn rotate(state: &mut AppState, degrees: f64) {
    let ids = state.selection.clone();
    if let Some(b) = state.doc.active_page().bounds_of(&ids) {
        state.doc.transform_objects(&ids, Affine::rotate_about(degrees.to_radians(), b.center()));
    }
}

pub fn mirror(state: &mut AppState, horizontal: bool) {
    let ids = state.selection.clone();
    if let Some(b) = state.doc.active_page().bounds_of(&ids) {
        let c = b.center().to_vec2();
        let (sx, sy) = if horizontal { (-1.0, 1.0) } else { (1.0, -1.0) };
        let t = Affine::translate(c) * Affine::scale_non_uniform(sx, sy) * Affine::translate(-c);
        state.doc.transform_objects(&ids, t);
    }
}

pub fn align_ui(ui: &mut egui::Ui, state: &AppState, commands: &mut Vec<Command>) {
    let target = if state.selection.len() == 1 { "the page" } else { "the last selected object" };
    ui.label(egui::RichText::new(format!("Aligns to {target}.")).weak().small());
    section(ui, "Align", |ui| {
        ui.horizontal_wrapped(|ui| {
            for (icon, tip, align) in [
                (ICON_ALIGN_HORIZONTAL_LEFT, "Align left (L)", Align::Left),
                (ICON_ALIGN_HORIZONTAL_CENTER, "Align centers horizontally (E)", Align::CenterHorizontal),
                (ICON_ALIGN_HORIZONTAL_RIGHT, "Align right (R)", Align::Right),
                (ICON_ALIGN_VERTICAL_TOP, "Align top (T)", Align::Top),
                (ICON_ALIGN_VERTICAL_CENTER, "Align centers vertically (C)", Align::CenterVertical),
                (ICON_ALIGN_VERTICAL_BOTTOM, "Align bottom (B)", Align::Bottom),
            ] {
                if crate::a11y::icon_button(ui, icon.rich_text().size(18.0), tip).clicked() {
                    commands.push(Command::Align(align));
                }
            }
        });
        if ui.button("Center to Page (P)").clicked() {
            commands.push(Command::CenterToPage);
        }
    });
    section(ui, "Order", |ui| {
        ui.horizontal_wrapped(|ui| order_buttons(ui, commands));
    });
}

pub fn order_buttons(ui: &mut egui::Ui, commands: &mut Vec<Command>) {
    for (icon, tip, order) in [
        (ICON_FLIP_TO_FRONT, "To front", Order::ToFront),
        (ICON_ARROW_UPWARD, "Forward one", Order::Forward),
        (ICON_ARROW_DOWNWARD, "Back one", Order::Backward),
        (ICON_FLIP_TO_BACK, "To back", Order::ToBack),
    ] {
        if crate::a11y::icon_button(ui, icon.rich_text().size(18.0), tip).clicked() {
            commands.push(Command::Order(order));
        }
    }
}

/// Every page's objects, top of the stack first. Click selects on the active page.
pub fn objects_ui(ui: &mut egui::Ui, state: &mut AppState, commands: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        ui.add_enabled_ui(state.has_selection(), |ui| {
            order_buttons(ui, commands);
            if crate::a11y::icon_button(ui, ICON_DELETE.rich_text().size(18.0), "Delete").clicked() {
                commands.push(Command::Delete);
            }
        });
    });
    ui.separator();
    let active = state.doc.active_page_index();
    for i in 0..state.doc.page_count() {
        let page_id = state.doc.pages()[i].id;
        let header = egui::RichText::new(state.doc.page_label(i));
        let header = if i == active { header.strong() } else { header };
        egui::CollapsingHeader::new(header)
            .id_salt(("objects_page", page_id.0))
            .default_open(i == active)
            .show(ui, |ui| {
                let objects: Vec<_> = state.doc.pages()[i].objects.iter().rev().map(|o| {
                    let label = if o.locked { format!("{}  {}", o.label(), <&str>::from(ICON_LOCK)) } else { o.label() };
                    (o.id, label)
                }).collect();
                if objects.is_empty() {
                    ui.label(egui::RichText::new("No objects").weak());
                }
                for (id, label) in objects {
                    if let Some((rid, name)) = &mut state.renaming_object
                        && *rid == id
                    {
                        let resp = crate::a11y::add(ui, egui::TextEdit::singleline(name), "Object name");
                        if resp.lost_focus() {
                            let new = name.trim().to_owned();
                            if let Some(page) = state.doc.page_mut(i)
                                && let Some(o) = page.object_mut(id)
                            {
                                o.name = (!new.is_empty()).then_some(new);
                            }
                            state.renaming_object = None;
                        } else {
                            resp.request_focus();
                        }
                        continue;
                    }
                    let selected = i == active && state.selection.contains(&id);
                    let resp = ui.selectable_label(selected, &label);
                    crate::a11y::name(&resp, &format!("Object {label}"));
                    let resp = resp.on_hover_text("Double-click to rename");
                    if resp.clicked() {
                        if i != active {
                            let _ = state.doc.set_active_page(i);
                            state.selection.clear();
                        }
                        let add = ui.input(|inp| inp.modifiers.shift || inp.modifiers.command);
                        if add {
                            if let Some(pos) = state.selection.iter().position(|s| *s == id) {
                                state.selection.remove(pos);
                            } else {
                                state.selection.push(id);
                            }
                        } else {
                            state.selection = vec![id];
                        }
                    }
                    if resp.double_clicked() {
                        let current = state.doc.pages()[i].object(id).and_then(|o| o.name.clone()).unwrap_or_default();
                        state.renaming_object = Some((id, current));
                    }
                }
            });
    }
}

/// Default style shown when nothing is selected (what new objects get).
pub fn new_object_note(ui: &mut egui::Ui) {
    let s = Style::new_object();
    let w = s.outline.as_ref().map_or(0.0, |o| o.width);
    ui.label(egui::RichText::new(format!("New objects: no fill, black {w} pt outline.")).weak().small());
}
