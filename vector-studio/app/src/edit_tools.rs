//! Edit menu extras: Step and Repeat (docker), Copy Properties From and Select All Nodes.

use vs_core::kurbo::{Point, Vec2};
use vs_core::{Fill, Gradient, ObjectId, Rgba, Shape};

use crate::app::AppState;
use crate::toolbox::Tool;

// ---------------------------------------------------------------------------------------
// Step and Repeat

/// How one direction of Step and Repeat moves each copy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StepMode {
    /// By `distance`, measured from one copy's position to the next.
    Offset,
    /// So `distance` separates one copy's edge from the next one's.
    Spacing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepAxis {
    pub mode: StepMode,
    /// Points; negative goes left (horizontal) or up (vertical).
    pub distance: f64,
}

impl StepAxis {
    /// The move per copy for a selection `size` long in this direction.
    pub fn step(self, size: f64) -> f64 {
        match self.mode {
            StepMode::Offset => self.distance,
            StepMode::Spacing if self.distance < 0.0 => self.distance - size,
            StepMode::Spacing => self.distance + size,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepRepeat {
    pub copies: u32,
    pub horizontal: StepAxis,
    pub vertical: StepAxis,
}

impl Default for StepRepeat {
    fn default() -> Self {
        Self {
            copies: 3,
            horizontal: StepAxis { mode: StepMode::Spacing, distance: 72.0 / 25.4 * 5.0 },
            vertical: StepAxis { mode: StepMode::Offset, distance: 0.0 },
        }
    }
}

/// Make the copies for the current selection with `state.step_repeat`; select them.
pub fn apply_step_repeat(state: &mut AppState) {
    let ids = state.selection.clone();
    let Some(bounds) = state.doc.active_page().bounds_of(&ids) else { return };
    let s = state.step_repeat;
    let step = Vec2::new(s.horizontal.step(bounds.width()), s.vertical.step(bounds.height()));
    let made = state.doc.step_and_repeat(&ids, s.copies, step);
    if !made.is_empty() {
        state.selection = made;
        state.marquee_selection = None;
    }
}

/// The Step and Repeat docker.
pub fn step_repeat_ui(ui: &mut egui::Ui, state: &mut AppState) {
    let unit = state.doc.display_unit;
    let mut s = state.step_repeat;
    for (title, axis, offset_name, spacing_name, distance_name) in [
        ("Horizontal", &mut s.horizontal, "Horizontal offset", "Horizontal spacing between objects", "Horizontal distance"),
        ("Vertical", &mut s.vertical, "Vertical offset", "Vertical spacing between objects", "Vertical distance"),
    ] {
        crate::panels::section(ui, title, |ui| {
            ui.horizontal(|ui| {
                let segs = [
                    (egui_material_icons::icons::ICON_SWIPE_RIGHT_ALT, offset_name, axis.mode == StepMode::Offset),
                    (egui_material_icons::icons::ICON_SPACE_BAR, spacing_name, axis.mode == StepMode::Spacing),
                ];
                if let Some(i) = crate::chrome::segmented(ui, title, &segs) {
                    axis.mode = if i == 0 { StepMode::Offset } else { StepMode::Spacing };
                }
                let mut d = unit.from_points(axis.distance);
                let field = egui::DragValue::new(&mut d).speed(unit.step() / 2.0).max_decimals(unit.decimals()).suffix(format!(" {}", unit.suffix()));
                if crate::a11y::add(ui, field, &format!("{distance_name} (Step and Repeat)")).changed() {
                    axis.distance = unit.to_points(d);
                }
            });
        });
    }
    ui.horizontal(|ui| {
        ui.label("Number of copies");
        crate::a11y::add(ui, egui::DragValue::new(&mut s.copies).range(1..=500), "Number of copies (Step and Repeat)");
    });
    state.step_repeat = s;
    ui.add_space(6.0);
    let enabled = state.has_selection();
    let resp = ui.add_enabled(enabled, egui::Button::new("Apply"));
    crate::a11y::button(&resp, enabled, "Apply (Step and Repeat)");
    if resp.clicked() {
        apply_step_repeat(state);
    }
}

// ---------------------------------------------------------------------------------------
// Copy Properties From

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropertySet {
    pub outline_pen: bool,
    pub outline_color: bool,
    pub fill: bool,
    pub text: bool,
}

impl Default for PropertySet {
    fn default() -> Self {
        Self { outline_pen: true, outline_color: true, fill: true, text: true }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CopyProperties {
    /// The options window is open.
    pub open: bool,
    pub what: PropertySet,
    /// Waiting for the click on the object to copy from.
    pub picking: bool,
}

/// The Copy Properties window: what to copy, then OK to pick the source object.
pub fn copy_properties_window(ctx: &egui::Context, state: &mut AppState) {
    if !state.copy_properties.open {
        return;
    }
    let mut open = true;
    let mut what = state.copy_properties.what;
    let mut ok = false;
    egui::Window::new("Copy Properties")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            for (flag, label) in [
                (&mut what.outline_pen, "Outline pen"),
                (&mut what.outline_color, "Outline color"),
                (&mut what.fill, "Fill"),
                (&mut what.text, "Text properties"),
            ] {
                let resp = ui.checkbox(flag, label);
                crate::a11y::name(&resp, &format!("{label} (Copy Properties)"));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let any = what.outline_pen || what.outline_color || what.fill || what.text;
                let resp = ui.add_enabled(any, egui::Button::new("OK"));
                crate::a11y::button(&resp, any, "OK (Copy Properties)");
                if resp.clicked() {
                    ok = true;
                }
                if ui.button("Cancel").clicked() {
                    state.copy_properties.open = false;
                }
            });
        });
    state.copy_properties.what = what;
    if !open {
        state.copy_properties.open = false;
    }
    if ok {
        state.copy_properties.open = false;
        state.copy_properties.picking = true;
    }
}

/// While picking, a click copies from the object under `p` to the selection. Returns
/// whether the click was used.
pub fn press_copy_from(state: &mut AppState, p: Point, tolerance: f64) -> bool {
    if !state.copy_properties.picking {
        return false;
    }
    state.copy_properties.picking = false;
    let page = state.doc.active_page();
    let Some(source) = page.hit_test(p, tolerance).and_then(|id| page.object(id)).cloned() else {
        state.status = Some("Copy Properties: no object there".into());
        return true;
    };
    let targets: Vec<ObjectId> = state.selection.iter().copied().filter(|id| *id != source.id).collect();
    copy_properties(state, &targets, &source, state.copy_properties.what);
    true
}

/// Copy the chosen properties of `source` to the unlocked objects among `targets`.
pub fn copy_properties(state: &mut AppState, targets: &[ObjectId], source: &vs_core::Object, what: PropertySet) {
    let from = source.shown_style().clone();
    state.doc.restyle_objects_framed(targets, |s, frame| {
        if what.fill {
            s.fill = from.fill.clone().map(|f| match f {
                // A fountain is laid across the new object, as the eyedropper does.
                Fill::Gradient(g) => {
                    let fresh = Gradient::across(g.kind, frame, Rgba::BLACK, Rgba::WHITE);
                    Fill::Gradient(Gradient { start: fresh.start, end: fresh.end, ..g })
                }
                solid => solid,
            });
        }
        match (&mut s.outline, &from.outline) {
            (Some(o), Some(src)) => {
                let color = o.color;
                if what.outline_pen {
                    *o = src.clone();
                    o.color = color;
                }
                if what.outline_color {
                    o.color = src.color;
                }
            }
            (None, Some(src)) if what.outline_pen => s.outline = Some(src.clone()),
            (Some(_), None) if what.outline_pen => s.outline = None,
            _ => {}
        }
    });
    if what.text
        && let Shape::Text(src) = &source.shape
    {
        let page = state.doc.active_page_mut();
        for id in targets {
            if let Some(obj) = page.object_mut(*id).filter(|o| !o.locked)
                && let Shape::Text(t) = &mut obj.shape
            {
                t.family.clone_from(&src.family);
                t.style.clone_from(&src.style);
                t.size = src.size;
                t.align = src.align;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Select All Nodes

/// The one unlocked curve selected, whose nodes Select All Nodes picks.
fn node_target(state: &AppState) -> Option<ObjectId> {
    let [id] = state.selection.as_slice() else { return None };
    state.doc.active_page().object(*id).filter(|o| !o.locked && matches!(o.shape, Shape::Curve { .. })).map(|o| o.id)
}

pub fn can_select_all_nodes(state: &AppState) -> bool {
    node_target(state).is_some()
}

/// Select every node of the selected curve, with the Shape tool.
pub fn select_all_nodes(state: &mut AppState) {
    let Some(id) = node_target(state) else { return };
    let Some(Shape::Curve { path }) = state.doc.active_page().object(id).map(|o| &o.shape) else { return };
    state.tools.nodes = vs_core::nodes::nodes(path).iter().map(|n| n.elem).collect();
    state.tool = Tool::Shape;
    state.toolbox.remember(Tool::Shape);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_steps_past_the_selection_and_offset_does_not() {
        let spacing = StepAxis { mode: StepMode::Spacing, distance: 5.0 };
        assert_eq!(spacing.step(20.0), 25.0);
        assert_eq!(StepAxis { distance: -5.0, ..spacing }.step(20.0), -25.0);
        assert_eq!(StepAxis { mode: StepMode::Offset, distance: 7.0 }.step(20.0), 7.0);
    }
}
