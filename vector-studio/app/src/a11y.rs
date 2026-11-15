//! Accessible names for custom-painted widgets. Screen readers read them, and the
//! end-to-end tests find widgets by them.

use egui::{Response, Role, WidgetInfo};

/// Report a painted button as a button called `name`.
pub fn button(resp: &Response, enabled: bool, name: &str) {
    resp.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, name));
}

/// Report a painted toggle (selected or not) called `name`.
pub fn toggle(resp: &Response, enabled: bool, selected: bool, name: &str) {
    resp.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, name));
}

/// Name a built-in widget that has no visible label of its own (a number field next to
/// an icon, a combo box after a separate text label).
pub fn name(resp: &Response, name: &str) {
    resp.ctx.accesskit_node_builder(resp.id, |node| node.set_label(name));
}

/// Add `widget` and give it the accessible name `name`.
pub fn add(ui: &mut egui::Ui, widget: impl egui::Widget, name: &str) -> Response {
    let resp = ui.add(widget);
    self::name(&resp, name);
    resp
}

/// An icon-only button in a docker: `tip` is its tooltip, "`tip` (docker)" its accessible
/// name, so it is not confused with the same command in the toolbar or property bar.
pub fn icon_button(ui: &mut egui::Ui, icon: impl Into<egui::WidgetText>, tip: &str) -> Response {
    let resp = ui.button(icon);
    self::name(&resp, &format!("{tip} (docker)"));
    resp.on_hover_text(tip)
}
