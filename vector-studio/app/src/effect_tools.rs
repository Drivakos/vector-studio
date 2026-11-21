//! The interactive effect tools besides Drop Shadow: Contour, Blend, Distort, Envelope,
//! Extrude, Block Shadow, and Mesh Fill. Each works on the object under the pointer and
//! keeps the effect editable: drag again, use the property bar, or Object > Clear Effect.

use egui::{Color32, Pos2, Stroke};
use egui_material_icons::icons::*;
use vs_core::kurbo::{Affine, BezPath, Point, Shape as _, Vec2};
use vs_core::{
    BlockShadow, Contour, ContourDirection, Distortion, DistortionKind, Envelope, EnvelopeMode, Extrude, ExtrudeShading, MeshFill, Object,
    ObjectId, Rgba, Shape,
};

use crate::app::AppState;
use crate::chrome::{icon_button, number_row, segmented};
use crate::fill_ui::color_button;
use crate::theme;
use crate::toolbox::Tool;

/// Settings new effects take (their size or place comes from the drag).
#[derive(Clone, Debug)]
pub struct EffectSettings {
    /// Its direction is the one an inward drag makes (Inside or To Center).
    pub contour: Contour,
    pub blend_steps: u32,
    /// Kind, amplitude, frequency and angle (the drag sets the centre).
    pub distortion: Distortion,
    pub envelope_mode: EnvelopeMode,
    pub extrude: Extrude,
    pub block_shadow: BlockShadow,
    pub mesh_columns: u32,
    pub mesh_rows: u32,
}

impl Default for EffectSettings {
    fn default() -> Self {
        Self {
            contour: Contour::new(ContourDirection::Inside),
            blend_steps: vs_core::Blend::STEPS,
            distortion: Distortion::new(DistortionKind::PushPull, Point::ZERO),
            envelope_mode: EnvelopeMode::Straight,
            extrude: Extrude::new(Vec2::ZERO),
            block_shadow: BlockShadow::new(Vec2::ZERO),
            mesh_columns: MeshFill::COLUMNS,
            mesh_rows: MeshFill::ROWS,
        }
    }
}

#[derive(Default)]
pub struct EffectState {
    pub settings: EffectSettings,
    /// Mesh Fill: the selected node of the selected object's mesh.
    pub mesh_node: Option<usize>,
}

/// A drag with one of these tools.
#[derive(Clone, Debug)]
pub enum EffectDrag {
    /// From `object` at `start`: makes the tool's effect as the pointer moves.
    Cast { object: ObjectId, start: Point },
    /// Blend: from `from`, to the object where the pointer is let go.
    Blend { from: ObjectId, start: Point },
    /// Blend: moving end `end` (0 start, 1 end) of a blend group.
    BlendEnd { group: ObjectId, end: usize, last: Point },
    EnvelopeNode { object: ObjectId, node: usize },
    VanishingPoint { object: ObjectId },
    MeshNode { object: ObjectId, node: usize },
}

pub fn is_effect_tool(tool: Tool) -> bool {
    matches!(tool, Tool::Contour | Tool::Blend | Tool::Distort | Tool::Envelope | Tool::Extrude | Tool::BlockShadow | Tool::MeshFill)
}

fn selected(state: &AppState) -> Option<&Object> {
    match state.selection.as_slice() {
        [id] => state.doc.active_page().object(*id),
        _ => None,
    }
}

/// Select the object under `p` (or clear the selection); returns it if it is unlocked.
fn pick(state: &mut AppState, p: Point, tolerance: f64) -> Option<ObjectId> {
    let page = state.doc.active_page();
    let Some(hit) = page.hit_test(p, tolerance) else {
        state.selection.clear();
        state.tools.effect.mesh_node = None;
        return None;
    };
    if state.selection != [hit] {
        state.selection = vec![hit];
        state.marquee_selection = None;
        state.tools.effect.mesh_node = None;
    }
    page.object(hit).filter(|o| !o.locked).map(|o| o.id)
}

/// The envelope nodes of the selected object (its own, or the one a drag would start
/// from), on the page.
fn envelope_nodes(state: &AppState) -> Option<(ObjectId, [Point; 8])> {
    let obj = selected(state).filter(|o| state.doc.can_warp(o.id))?;
    let envelope = obj.effects.envelope.unwrap_or_else(|| Envelope::around(obj.warp_frame(), state.tools.effect.settings.envelope_mode));
    Some((obj.id, envelope.nodes.map(|n| obj.transform * n)))
}

fn vanishing_point(obj: &Object) -> Option<Point> {
    obj.effects.extrude.as_ref().map(|x| obj.effect_bounds().center() + x.vanishing_point)
}

fn mesh_nodes(state: &AppState) -> Option<(ObjectId, Vec<Point>)> {
    let obj = selected(state)?;
    let mesh = obj.effects.mesh.as_ref()?;
    Some((obj.id, mesh.points.iter().map(|p| obj.transform * *p).collect()))
}

fn nearest(points: &[Point], p: Point, tolerance: f64) -> Option<usize> {
    (0..points.len()).filter(|i| (points[*i] - p).hypot() <= tolerance).min_by(|a, b| (points[*a] - p).hypot().total_cmp(&(points[*b] - p).hypot()))
}

/// A press with an effect tool at `p`; `tolerance` is the pick distance in points.
pub fn press(state: &mut AppState, p: Point, tolerance: f64) -> Option<EffectDrag> {
    match state.tool {
        Tool::Envelope => {
            if let Some((object, nodes)) = envelope_nodes(state)
                && let Some(node) = nearest(&nodes, p, tolerance)
            {
                return Some(EffectDrag::EnvelopeNode { object, node });
            }
            pick(state, p, tolerance);
            None
        }
        Tool::Extrude => {
            if let Some(obj) = selected(state)
                && vanishing_point(obj).is_some_and(|vp| (vp - p).hypot() <= tolerance)
                && !obj.locked
            {
                return Some(EffectDrag::VanishingPoint { object: obj.id });
            }
            pick(state, p, tolerance).map(|object| EffectDrag::Cast { object, start: p })
        }
        Tool::MeshFill => {
            if let Some((object, nodes)) = mesh_nodes(state)
                && let Some(node) = nearest(&nodes, p, tolerance)
            {
                state.tools.effect.mesh_node = Some(node);
                return Some(EffectDrag::MeshNode { object, node });
            }
            let object = pick(state, p, tolerance)?;
            let s = &state.tools.effect.settings;
            let (columns, rows) = (s.mesh_columns, s.mesh_rows);
            state.doc.edit_effects(&[object], |o| {
                if o.effects.mesh.is_none() && o.can_take_mesh() {
                    let color = o.style.fill.as_ref().map_or(Rgba::WHITE, vs_core::Fill::color);
                    o.effects.mesh = Some(MeshFill::new(o.warp_frame(), columns, rows, color));
                }
            });
            None
        }
        Tool::Blend => {
            if let Some(group) = selected(state).filter(|o| o.is_blend() && !o.locked)
                && let Shape::Group { children } = &group.shape
                && let Some(end) = [1, 0].into_iter().find(|i| children[*i].hit_in(group.transform, p, tolerance))
            {
                return Some(EffectDrag::BlendEnd { group: group.id, end, last: p });
            }
            pick(state, p, tolerance).map(|from| EffectDrag::Blend { from, start: p })
        }
        Tool::Distort => pick(state, p, tolerance).filter(|id| state.doc.can_warp(*id)).map(|object| EffectDrag::Cast { object, start: p }),
        _ => pick(state, p, tolerance).map(|object| EffectDrag::Cast { object, start: p }),
    }
}

/// The pointer moved to `p` during `drag`; `moved` says whether it has gone far enough
/// from the press to count as a drag.
pub fn drag(state: &mut AppState, drag: &mut EffectDrag, p: Point, moved: bool) {
    let settings = state.tools.effect.settings.clone();
    match drag {
        EffectDrag::Cast { object, start } if moved => {
            let (object, start) = (*object, *start);
            let Some(obj) = state.doc.active_page().object(object) else { return };
            let to_local = obj.transform.inverse();
            let frame = obj.warp_frame();
            let center = obj.effect_bounds().center();
            let inside = obj.world_path().winding(p) % 2 != 0;
            state.doc.edit_effects(&[object], |o| match state.tool {
                Tool::Contour => {
                    let mut c = o.effects.contour.take().unwrap_or_else(|| settings.contour.clone());
                    c.direction = match (inside, settings.contour.direction) {
                        (false, _) => ContourDirection::Outside,
                        (true, ContourDirection::ToCenter) => ContourDirection::ToCenter,
                        _ => ContourDirection::Inside,
                    };
                    c.offset = ((p - start).hypot() / f64::from(c.steps.max(1))).max(0.1);
                    o.effects.contour = Some(c);
                }
                Tool::Extrude => {
                    let mut x = o.effects.extrude.take().unwrap_or_else(|| settings.extrude.clone());
                    x.vanishing_point = p - center;
                    o.effects.extrude = Some(x);
                }
                Tool::BlockShadow => {
                    let mut b = o.effects.block_shadow.take().unwrap_or_else(|| settings.block_shadow.clone());
                    b.offset = p - start;
                    o.effects.block_shadow = Some(b);
                }
                Tool::Distort => {
                    let (c, q) = (to_local * start, to_local * p);
                    let d = q - c;
                    let mut x = Distortion { center: c, ..settings.distortion };
                    match x.kind {
                        DistortionKind::PushPull => x.amplitude = (d.x / (frame.width() / 2.0).max(1e-9) * 100.0).clamp(-Distortion::MAX_PUSH_PULL, Distortion::MAX_PUSH_PULL),
                        DistortionKind::Zipper => x.amplitude = (d.hypot() / (frame.width().min(frame.height()) / 2.0).max(1e-9) * 100.0).min(Distortion::MAX_ZIPPER),
                        DistortionKind::Twister => x.angle = d.y.atan2(d.x).to_degrees(),
                    }
                    o.effects.distortion = Some(x);
                }
                _ => {}
            });
        }
        EffectDrag::BlendEnd { group, end, last } => {
            let (group_id, end, d) = (*group, *end, p - *last);
            *last = p;
            if let Some(g) = state.doc.active_page_mut().object_mut(group_id)
                && let Shape::Group { children } = &mut g.shape
            {
                // The move, in the group's space.
                let t = g.transform.inverse() * Affine::translate(d) * g.transform;
                children[end].transform = t * children[end].transform;
            }
        }
        EffectDrag::EnvelopeNode { object, node } => {
            let (object, node) = (*object, *node);
            let mode = settings.envelope_mode;
            state.doc.edit_effects(&[object], |o| {
                let local = o.transform.inverse() * p;
                let mut e = o.effects.envelope.unwrap_or_else(|| Envelope::around(o.warp_frame(), mode));
                e.move_node(node, local);
                o.effects.envelope = Some(e);
            });
        }
        EffectDrag::VanishingPoint { object } => {
            let object = *object;
            let Some(center) = state.doc.active_page().object(object).map(|o| o.effect_bounds().center()) else { return };
            state.doc.edit_effects(&[object], |o| {
                if let Some(x) = o.effects.extrude.as_mut() {
                    x.vanishing_point = p - center;
                }
            });
        }
        EffectDrag::MeshNode { object, node } => {
            let (object, node) = (*object, *node);
            state.doc.edit_effects(&[object], |o| {
                let local = o.transform.inverse() * p;
                if let Some(point) = o.effects.mesh.as_mut().and_then(|m| m.points.get_mut(node)) {
                    *point = local;
                }
            });
        }
        _ => {}
    }
}

/// The pointer was let go at `p`.
pub fn release(state: &mut AppState, drag: EffectDrag, p: Point, tolerance: f64) {
    if let EffectDrag::Blend { from, .. } = drag {
        let to = state.doc.active_page().hit_test(p, tolerance);
        if let Some(to) = to.filter(|to| state.doc.can_blend(from, *to))
            && let Some(group) = state.doc.blend_objects(from, to, state.tools.effect.settings.blend_steps)
        {
            state.selection = vec![group];
            state.marquee_selection = None;
        }
    }
}

const ACCENT: Color32 = Color32::from_rgb(0x1E, 0x6F, 0xD9);

fn node(painter: &egui::Painter, at: Pos2, filled: bool) {
    let r = egui::Rect::from_center_size(at, egui::vec2(7.0, 7.0));
    painter.rect_filled(r, 0.0, if filled { ACCENT } else { Color32::WHITE });
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, if filled { Color32::WHITE } else { Color32::BLACK }), egui::StrokeKind::Inside);
}

fn polyline(painter: &egui::Painter, path: &BezPath, screen: &impl Fn(Point) -> Pos2, stroke: Stroke) {
    let mut points: Vec<Pos2> = Vec::new();
    vs_core::kurbo::flatten(path, 0.25, |el| match el {
        vs_core::kurbo::PathEl::MoveTo(p) => {
            if points.len() > 1 {
                painter.add(egui::Shape::line(std::mem::take(&mut points), stroke));
            }
            points = vec![screen(p)];
        }
        vs_core::kurbo::PathEl::LineTo(p) => points.push(screen(p)),
        vs_core::kurbo::PathEl::ClosePath => {
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
        }
        _ => {}
    });
    if points.len() > 1 {
        painter.add(egui::Shape::line(points, stroke));
    }
}

/// What the tool shows on the canvas: envelope and mesh nodes, the vanishing point, the
/// blend being dragged.
pub fn draw(painter: &egui::Painter, state: &AppState, drag: Option<&EffectDrag>, pointer: Option<Point>, screen: impl Fn(Point) -> Pos2) {
    let thin = Stroke::new(1.0, ACCENT);
    match state.tool {
        Tool::Envelope => {
            if let Some(obj) = selected(state).filter(|o| state.doc.can_warp(o.id)) {
                let envelope = obj.effects.envelope.unwrap_or_else(|| Envelope::around(obj.warp_frame(), state.tools.effect.settings.envelope_mode));
                let mut outline = envelope.outline(obj.warp_frame());
                outline.apply_affine(obj.transform);
                polyline(painter, &outline, &screen, thin);
                for n in envelope.nodes {
                    node(painter, screen(obj.transform * n), false);
                }
            }
        }
        Tool::Extrude => {
            if let Some(obj) = selected(state)
                && let Some(vp) = vanishing_point(obj)
            {
                let (c, v) = (screen(obj.effect_bounds().center()), screen(vp));
                painter.line_segment([c, v], thin);
                let d = 5.0;
                let s = Stroke::new(2.0, ACCENT);
                painter.line_segment([v + egui::vec2(-d, -d), v + egui::vec2(d, d)], s);
                painter.line_segment([v + egui::vec2(-d, d), v + egui::vec2(d, -d)], s);
            }
        }
        Tool::MeshFill => {
            if let Some(obj) = selected(state)
                && let Some(mesh) = &obj.effects.mesh
            {
                let at = |i: usize| screen(obj.transform * mesh.points[i]);
                let w = mesh.columns as usize + 1;
                for r in 0..=mesh.rows as usize {
                    for c in 0..=mesh.columns as usize {
                        let i = r * w + c;
                        if c < mesh.columns as usize {
                            painter.line_segment([at(i), at(i + 1)], thin);
                        }
                        if r < mesh.rows as usize {
                            painter.line_segment([at(i), at(i + w)], thin);
                        }
                    }
                }
                for i in 0..mesh.points.len() {
                    node(painter, at(i), state.tools.effect.mesh_node == Some(i));
                }
            }
        }
        Tool::Distort => {
            if let Some(obj) = selected(state)
                && let Some(d) = obj.effects.distortion
            {
                let c = screen(obj.transform * d.center);
                painter.add(egui::Shape::convex_polygon(
                    vec![c + egui::vec2(0.0, -5.0), c + egui::vec2(5.0, 0.0), c + egui::vec2(0.0, 5.0), c + egui::vec2(-5.0, 0.0)],
                    Color32::WHITE,
                    thin,
                ));
            }
        }
        Tool::Blend => {
            if let (Some(EffectDrag::Blend { start, .. }), Some(p)) = (drag, pointer) {
                painter.line_segment([screen(*start), screen(p)], Stroke::new(1.0, ACCENT));
            }
        }
        _ => {}
    }
}

/// The selected unlocked objects that `has` the effect.
fn targets(state: &AppState, has: impl Fn(&Object) -> bool) -> Vec<ObjectId> {
    state.doc.with_effect(&state.selection, has)
}

fn hint_label(ui: &mut egui::Ui, state: &AppState) {
    theme::vsep(ui);
    ui.label(egui::RichText::new(hint(state)).color(theme::TEXT_DIM));
}

fn clear_button(ui: &mut egui::Ui, tip: &str, targets: &[ObjectId]) -> bool {
    theme::vsep(ui);
    icon_button(ui, ICON_CLOSE, tip, !targets.is_empty()).clicked()
}

pub fn property_bar(ui: &mut egui::Ui, state: &mut AppState) {
    match state.tool {
        Tool::Contour => contour_bar(ui, state),
        Tool::Blend => blend_bar(ui, state),
        Tool::Distort => distort_bar(ui, state),
        Tool::Envelope => envelope_bar(ui, state),
        Tool::Extrude => extrude_bar(ui, state),
        Tool::BlockShadow => block_shadow_bar(ui, state),
        Tool::MeshFill => mesh_bar(ui, state),
        _ => {}
    }
    hint_label(ui, state);
}

fn steps_row(ui: &mut egui::Ui, tip: &str, steps: &mut u32, max: u32) -> bool {
    let mut n = f64::from(*steps);
    let changed = number_row(ui, Some(ICON_LINEAR_SCALE), tip, &mut n, "", 1.0..=f64::from(max), 1.0, 0);
    if changed {
        *steps = (n.round() as u32).clamp(1, max);
    }
    changed
}

fn length_row(ui: &mut egui::Ui, state: &AppState, icon: egui_material_icons::MaterialIcon, tip: &str, points: &mut f64, max: f64) -> bool {
    let unit = state.doc.display_unit;
    let mut v = unit.from_points(*points);
    let changed = number_row(ui, Some(icon), tip, &mut v, unit.suffix(), 0.0..=unit.from_points(max), unit.step(), unit.decimals());
    if changed {
        *points = unit.to_points(v).clamp(0.0, max);
    }
    changed
}

fn contour_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let targets = targets(state, |o| o.effects.contour.is_some());
    let current = targets.first().and_then(|id| state.doc.active_page().object(*id)).and_then(|o| o.effects.contour.clone());
    let mut edit = current.clone().unwrap_or_else(|| state.tools.effect.settings.contour.clone());
    let mut changed = false;
    let d = edit.direction;
    let segments = [
        (ICON_CENTER_FOCUS_STRONG, "Contour to center", d == ContourDirection::ToCenter),
        (ICON_ZOOM_IN_MAP, "Inside contour", d == ContourDirection::Inside),
        (ICON_ZOOM_OUT_MAP, "Outside contour", d == ContourDirection::Outside),
    ];
    if let Some(i) = segmented(ui, "contour_direction", &segments) {
        edit.direction = [ContourDirection::ToCenter, ContourDirection::Inside, ContourDirection::Outside][i];
        changed = true;
    }
    changed |= steps_row(ui, "Contour steps", &mut edit.steps, Contour::MAX_STEPS);
    changed |= length_row(ui, state, ICON_SPACE_BAR, "Contour offset", &mut edit.offset, 1000.0);
    edit.offset = edit.offset.max(0.1);
    changed |= color_button(ui, &mut edit.fill, "Contour fill color");
    changed |= color_button(ui, &mut edit.outline, "Contour outline color");
    if changed {
        if targets.is_empty() {
            state.tools.effect.settings.contour = edit;
        } else {
            state.doc.edit_effects(&targets, |o| o.effects.contour = Some(edit.clone()));
        }
    }
    if clear_button(ui, "Clear contour", &targets) {
        state.doc.edit_effects(&targets, |o| o.effects.contour = None);
    }
}

fn blend_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let targets = targets(state, Object::is_blend);
    let current = targets.first().and_then(|id| state.doc.active_page().object(*id)).and_then(|o| o.effects.blend.as_ref()).map(|b| b.steps);
    let mut steps = current.unwrap_or(state.tools.effect.settings.blend_steps);
    if steps_row(ui, "Blend steps", &mut steps, vs_core::Blend::MAX_STEPS) {
        if targets.is_empty() {
            state.tools.effect.settings.blend_steps = steps;
        } else {
            state.doc.edit_effects(&targets, |o| {
                if let Some(b) = o.effects.blend.as_mut() {
                    b.steps = steps;
                }
            });
        }
    }
    if clear_button(ui, "Clear blend", &targets) {
        state.doc.edit_effects(&targets, |o| o.effects.blend = None);
    }
}

fn distort_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let targets = targets(state, |o| o.effects.distortion.is_some());
    let current = targets.first().and_then(|id| state.doc.active_page().object(*id)).and_then(|o| o.effects.distortion);
    let mut edit = current.unwrap_or(state.tools.effect.settings.distortion);
    let mut changed = false;
    let k = edit.kind;
    let segments = [
        (ICON_OPEN_WITH, "Push and pull distortion", k == DistortionKind::PushPull),
        (ICON_SHOW_CHART, "Zipper distortion", k == DistortionKind::Zipper),
        (ICON_CYCLONE, "Twister distortion", k == DistortionKind::Twister),
    ];
    if let Some(i) = segmented(ui, "distortion_kind", &segments) {
        edit.kind = [DistortionKind::PushPull, DistortionKind::Zipper, DistortionKind::Twister][i];
        changed = true;
    }
    match edit.kind {
        DistortionKind::PushPull => {
            let max = Distortion::MAX_PUSH_PULL;
            changed |= number_row(ui, Some(ICON_OPEN_WITH), "Push and pull amplitude", &mut edit.amplitude, "", -max..=max, 1.0, 0);
        }
        DistortionKind::Zipper => {
            changed |= number_row(ui, Some(ICON_HEIGHT), "Zipper amplitude", &mut edit.amplitude, "", 0.0..=Distortion::MAX_ZIPPER, 1.0, 0);
            let mut f = f64::from(edit.frequency);
            if number_row(ui, Some(ICON_WAVES), "Zipper frequency", &mut f, "", 0.0..=f64::from(Distortion::MAX_FREQUENCY), 1.0, 0) {
                edit.frequency = f.round() as u32;
                changed = true;
            }
        }
        DistortionKind::Twister => {
            let max = Distortion::MAX_ANGLE;
            changed |= number_row(ui, Some(ICON_ROTATE_RIGHT), "Twister angle", &mut edit.angle, "°", -max..=max, 1.0, 0);
        }
    }
    if changed {
        if targets.is_empty() {
            state.tools.effect.settings.distortion = edit;
        } else {
            state.doc.edit_effects(&targets, |o| {
                // Each object keeps its own centre.
                let center = o.effects.distortion.map_or(edit.center, |d| d.center);
                o.effects.distortion = Some(Distortion { center, ..edit });
            });
        }
    }
    if clear_button(ui, "Clear distortion", &targets) {
        state.doc.edit_effects(&targets, |o| o.effects.distortion = None);
    }
}

fn envelope_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let targets = targets(state, |o| o.effects.envelope.is_some());
    let current = targets.first().and_then(|id| state.doc.active_page().object(*id)).and_then(|o| o.effects.envelope).map(|e| e.mode);
    let mode = current.unwrap_or(state.tools.effect.settings.envelope_mode);
    let segments = [(ICON_TIMELINE, "Straight line envelope", mode == EnvelopeMode::Straight), (ICON_GESTURE, "Single arc envelope", mode == EnvelopeMode::Arc)];
    if let Some(i) = segmented(ui, "envelope_mode", &segments) {
        let mode = [EnvelopeMode::Straight, EnvelopeMode::Arc][i];
        state.tools.effect.settings.envelope_mode = mode;
        state.doc.edit_effects(&targets, |o| {
            if let Some(e) = o.effects.envelope.as_mut() {
                e.mode = mode;
            }
        });
    }
    if clear_button(ui, "Clear envelope", &targets) {
        state.doc.edit_effects(&targets, |o| o.effects.envelope = None);
    }
}

fn extrude_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let targets = targets(state, |o| o.effects.extrude.is_some());
    let current = targets.first().and_then(|id| state.doc.active_page().object(*id)).and_then(|o| o.effects.extrude.clone());
    let mut edit = current.unwrap_or_else(|| state.tools.effect.settings.extrude.clone());
    let mut changed = number_row(ui, Some(ICON_LAYERS), "Extrude depth", &mut edit.depth, "", 1.0..=Extrude::MAX_DEPTH, 1.0, 0);
    let s = edit.shading;
    let segments = [
        (ICON_FORMAT_COLOR_FILL, "Extrude with object fill", s == ExtrudeShading::ObjectFill),
        (ICON_FORMAT_PAINT, "Extrude with solid fill", s == ExtrudeShading::Solid),
        (ICON_GRADIENT, "Extrude with shading", s == ExtrudeShading::Shade),
    ];
    if let Some(i) = segmented(ui, "extrude_shading", &segments) {
        edit.shading = [ExtrudeShading::ObjectFill, ExtrudeShading::Solid, ExtrudeShading::Shade][i];
        changed = true;
    }
    if edit.shading != ExtrudeShading::ObjectFill {
        changed |= color_button(ui, &mut edit.color, "Extrude color");
    }
    if edit.shading == ExtrudeShading::Shade {
        changed |= color_button(ui, &mut edit.shade, "Extrude shade color");
    }
    if changed {
        if targets.is_empty() {
            state.tools.effect.settings.extrude = edit;
        } else {
            state.doc.edit_effects(&targets, |o| {
                // Each object keeps its own vanishing point.
                let vp = o.effects.extrude.as_ref().map_or(edit.vanishing_point, |x| x.vanishing_point);
                o.effects.extrude = Some(Extrude { vanishing_point: vp, ..edit.clone() });
            });
        }
    }
    if clear_button(ui, "Clear extrude", &targets) {
        state.doc.edit_effects(&targets, |o| o.effects.extrude = None);
    }
}

fn block_shadow_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let targets = targets(state, |o| o.effects.block_shadow.is_some());
    let current = targets.first().and_then(|id| state.doc.active_page().object(*id)).and_then(|o| o.effects.block_shadow.clone());
    let mut edit = current.clone().unwrap_or_else(|| state.tools.effect.settings.block_shadow.clone());
    let mut changed = false;
    if current.is_some() {
        let mut depth = edit.offset.hypot();
        let mut angle = (-edit.offset.y).atan2(edit.offset.x).to_degrees();
        let depth_changed = length_row(ui, state, ICON_LINE_WEIGHT, "Block shadow depth", &mut depth, 5000.0);
        let angle_changed = number_row(ui, Some(ICON_EXPLORE), "Block shadow direction", &mut angle, "°", -180.0..=180.0, 1.0, 0);
        if depth_changed || angle_changed {
            let a = angle.to_radians();
            edit.offset = Vec2::new(a.cos(), -a.sin()) * depth;
            changed = true;
        }
    }
    changed |= color_button(ui, &mut edit.color, "Block shadow color");
    if changed {
        if targets.is_empty() {
            state.tools.effect.settings.block_shadow = edit;
        } else {
            state.doc.edit_effects(&targets, |o| o.effects.block_shadow = Some(edit.clone()));
        }
    }
    if clear_button(ui, "Clear block shadow", &targets) {
        state.doc.edit_effects(&targets, |o| o.effects.block_shadow = None);
    }
}

fn mesh_bar(ui: &mut egui::Ui, state: &mut AppState) {
    let targets = targets(state, |o| o.effects.mesh.is_some());
    let current = targets.first().and_then(|id| state.doc.active_page().object(*id)).and_then(|o| o.effects.mesh.clone());
    let s = &state.tools.effect.settings;
    let (mut columns, mut rows) = current.as_ref().map_or((s.mesh_columns, s.mesh_rows), |m| (m.columns, m.rows));
    let changed = steps_row(ui, "Mesh columns", &mut columns, MeshFill::MAX_CELLS) | steps_row(ui, "Mesh rows", &mut rows, MeshFill::MAX_CELLS);
    if changed {
        state.tools.effect.settings.mesh_columns = columns;
        state.tools.effect.settings.mesh_rows = rows;
        state.tools.effect.mesh_node = None;
        state.doc.edit_effects(&targets, |o| {
            let frame = o.warp_frame();
            if let Some(m) = o.effects.mesh.as_mut() {
                *m = m.regrid(frame, columns, rows);
            }
        });
    }
    let node = state.tools.effect.mesh_node.filter(|i| current.as_ref().is_some_and(|m| *i < m.colors.len()));
    if let (Some(i), Some(mesh)) = (node, &current) {
        theme::vsep(ui);
        let mut color = mesh.colors[i];
        if color_button(ui, &mut color, "Mesh node color") {
            state.doc.edit_effects(&targets[..1], |o| {
                if let Some(c) = o.effects.mesh.as_mut().and_then(|m| m.colors.get_mut(i)) {
                    *c = color;
                }
            });
        }
    }
    if clear_button(ui, "Clear mesh", &targets) {
        state.tools.effect.mesh_node = None;
        state.doc.edit_effects(&targets, |o| o.effects.mesh = None);
    }
}

/// Set the selected mesh node's colour (a palette swatch with the Mesh Fill tool).
/// Returns false if no node is selected.
pub fn color_mesh_node(state: &mut AppState, color: Rgba) -> bool {
    let (Some(i), Some(obj)) = (state.tools.effect.mesh_node, selected(state)) else { return false };
    if state.tool != Tool::MeshFill || obj.effects.mesh.is_none() {
        return false;
    }
    let id = obj.id;
    state.doc.edit_effects(&[id], |o| {
        if let Some(c) = o.effects.mesh.as_mut().and_then(|m| m.colors.get_mut(i)) {
            *c = color;
        }
    });
    true
}

pub fn hint(state: &AppState) -> String {
    let s = |t: &str| t.to_string();
    match state.tool {
        Tool::Contour => s("Drag from an object outward for an outside contour or inward for an inside one · Object > Clear Effect removes it"),
        Tool::Blend if selected(state).is_some_and(Object::is_blend) => s("Drag the start or end object to move it · the steps follow"),
        Tool::Blend => s("Drag from one object to another to blend them"),
        Tool::Distort => s("Drag from a point on an object to distort it about that point · pick the distortion in the property bar"),
        Tool::Envelope if selected(state).is_some_and(|o| state.doc.can_warp(o.id)) => s("Drag the envelope's nodes to reshape the object"),
        Tool::Envelope => s("Click an object, then drag the envelope's nodes"),
        Tool::Extrude => s("Drag from an object toward the vanishing point · drag the cross at its end to move it"),
        Tool::BlockShadow => s("Drag from an object to cast its block shadow"),
        Tool::MeshFill if state.tools.effect.mesh_node.is_some() => s("Pick the node's colour in the property bar or the palette · drag nodes to bend the mesh"),
        Tool::MeshFill => s("Click an object to add a mesh, then click a node to colour it · drag nodes to bend the mesh"),
        _ => String::new(),
    }
}
