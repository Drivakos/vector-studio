//! Dockers: the panels at the right of the window.

use egui_material_icons::{MaterialIcon, icons::*};
use serde::{Deserialize, Serialize};
use vs_core::{Orientation, PAGE_PRESETS, PagePreset, Unit};

use crate::app::AppState;
use crate::commands::Command;
use crate::panels::{self, section};
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DockerKind {
    Properties,
    Objects,
    Pages,
    Transform,
    AlignDistribute,
    StepRepeat,
    FindReplace,
}

impl DockerKind {
    pub const ALL: [DockerKind; 7] = [
        DockerKind::Properties,
        DockerKind::Objects,
        DockerKind::Pages,
        DockerKind::Transform,
        DockerKind::AlignDistribute,
        DockerKind::StepRepeat,
        DockerKind::FindReplace,
    ];
    /// Open in a new workspace.
    pub const DEFAULT_OPEN: [DockerKind; 5] = [
        DockerKind::Properties,
        DockerKind::Objects,
        DockerKind::Pages,
        DockerKind::Transform,
        DockerKind::AlignDistribute,
    ];

    pub fn icon(self) -> MaterialIcon {
        match self {
            DockerKind::Properties => ICON_TUNE,
            DockerKind::Objects => ICON_LAYERS,
            DockerKind::Pages => ICON_DESCRIPTION,
            DockerKind::Transform => ICON_TRANSFORM,
            DockerKind::AlignDistribute => ICON_ALIGN_HORIZONTAL_LEFT,
            DockerKind::StepRepeat => ICON_LIBRARY_ADD,
            DockerKind::FindReplace => ICON_FIND_REPLACE,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            DockerKind::Properties => "Properties",
            DockerKind::Objects => "Objects",
            DockerKind::Pages => "Pages",
            DockerKind::Transform => "Transform",
            DockerKind::AlignDistribute => "Align and Distribute",
            DockerKind::StepRepeat => "Step and Repeat",
            DockerKind::FindReplace => "Find and Replace",
        }
    }
}

/// The docker area at the right, as in the reference app: one docker shown at a time,
/// a vertical strip of icons to switch between the open ones, and a header with a close
/// button and the title.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerHost {
    /// Open dockers, in strip order.
    pub open: Vec<DockerKind>,
    pub active: Option<DockerKind>,
    /// Window › Show Dockers (the toolbar's Dockers button) hides the whole area.
    pub visible: bool,
}

impl Default for DockerHost {
    fn default() -> Self {
        Self {
            open: DockerKind::DEFAULT_OPEN.to_vec(),
            active: Some(DockerKind::Properties),
            visible: true,
        }
    }
}

impl DockerHost {
    /// Shown on screen right now (checked in the Window menu).
    pub fn is_shown(&self, kind: DockerKind) -> bool {
        self.visible && self.active == Some(kind)
    }

    /// Open and bring to front.
    pub fn show(&mut self, kind: DockerKind) {
        if !self.open.contains(&kind) {
            self.open.push(kind);
        }
        self.active = Some(kind);
        self.visible = true;
    }

    pub fn close(&mut self, kind: DockerKind) {
        self.open.retain(|k| *k != kind);
        if self.active == Some(kind) {
            self.active = self.open.last().copied();
        }
    }

    /// Window › Dockers › X: close it if it is in front, otherwise bring it to front.
    pub fn toggle(&mut self, kind: DockerKind) {
        if self.is_shown(kind) {
            self.close(kind);
        } else {
            self.show(kind);
        }
    }

    /// Something to show: visible, with at least one open docker.
    pub fn has_content(&self) -> bool {
        self.visible && self.active.is_some()
    }

    /// Repair state loaded from an older or edited settings file.
    pub fn repair(&mut self) {
        let mut seen = Vec::new();
        self.open.retain(|k| {
            let new = !seen.contains(k);
            seen.push(*k);
            new
        });
        if self.active.is_none_or(|a| !self.open.contains(&a)) {
            self.active = self.open.first().copied();
        }
    }
}

/// The vertical icon strip between the docker and the palette.
pub fn strip_ui(ui: &mut egui::Ui, host: &mut DockerHost) {
    let size = egui::vec2(theme::DOCKER_STRIP_WIDTH, 30.0);
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.add_space(4.0);
    let mut clicked = None;
    for &kind in &host.open {
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        let active = host.is_shown(kind);
        let r = egui::Rect::from_center_size(rect.center(), egui::vec2(26.0, 26.0));
        if active {
            ui.painter().rect_filled(r, 5.0, theme::SUNKEN);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 5.0, theme::FIELD);
        }
        let color = if active || resp.hovered() { egui::Color32::WHITE } else { theme::TEXT_DIM };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            <&str>::from(kind.icon()),
            egui::FontId::new(18.0, kind.icon().font_family()),
            color,
        );
        crate::a11y::toggle(&resp, true, active, &format!("{} docker", kind.title()));
        if resp.on_hover_text(kind.title()).clicked() {
            clicked = Some(kind);
        }
    }
    if let Some(kind) = clicked {
        if host.is_shown(kind) {
            host.visible = false;
        } else {
            host.show(kind);
        }
    }
    // "More" button at the bottom: the list of every docker.
    let bottom = egui::Rect::from_min_size(
        egui::pos2(ui.max_rect().min.x, ui.max_rect().max.y - 30.0),
        size,
    );
    let resp = ui.interact(bottom, ui.scope_id().with("docker_list"), egui::Sense::click());
    ui.painter().text(
        bottom.center(),
        egui::Align2::CENTER_CENTER,
        <&str>::from(ICON_MORE_HORIZ),
        egui::FontId::new(18.0, ICON_MORE_HORIZ.font_family()),
        if resp.hovered() { egui::Color32::WHITE } else { theme::TEXT_DIM },
    );
    crate::a11y::button(&resp, true, "Docker list");
    let resp = resp.on_hover_text("Dockers");
    egui::Popup::menu(&resp).show(|ui| {
        for kind in DockerKind::ALL {
            let shown = host.is_shown(kind);
            if ui.add(egui::Button::new(kind.title()).selected(shown)).clicked() {
                host.toggle(kind);
            }
        }
    });
}

/// The docker in front: header (close button, title) and its content.
pub fn docker_ui(ui: &mut egui::Ui, state: &mut AppState, commands: &mut Vec<Command>) {
    let Some(kind) = state.dockers.active else { return };
    let full = ui.max_rect();
    let header = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), theme::DOCKER_HEADER));
    ui.painter().rect_filled(header, 0.0, theme::BAR);
    ui.painter().text(
        header.center(),
        egui::Align2::CENTER_CENTER,
        kind.title(),
        egui::FontId::proportional(14.0),
        theme::TEXT,
    );
    let close = egui::Rect::from_center_size(egui::pos2(header.min.x + 18.0, header.center().y), egui::vec2(22.0, 22.0));
    let resp = ui.interact(close, ui.scope_id().with("docker_close"), egui::Sense::click());
    crate::a11y::button(&resp, true, &format!("Close {} docker", kind.title()));
    if resp.hovered() {
        ui.painter().rect_filled(close, 4.0, theme::FIELD);
    }
    ui.painter().text(
        close.center(),
        egui::Align2::CENTER_CENTER,
        <&str>::from(ICON_CLOSE),
        egui::FontId::new(18.0, ICON_CLOSE.font_family()),
        theme::TEXT,
    );
    if resp.on_hover_text("Close").clicked() {
        state.dockers.close(kind);
        return;
    }

    let body = egui::Rect::from_min_max(egui::pos2(full.min.x, header.max.y), full.max);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
    egui::ScrollArea::vertical().auto_shrink(false).show(&mut child, |ui| {
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| match kind {
            DockerKind::Properties if state.has_selection() => panels::object_properties_ui(ui, state),
            DockerKind::Properties => properties_ui(ui, state),
            DockerKind::Objects => panels::objects_ui(ui, state, commands),
            DockerKind::Pages => pages_ui(ui, state, commands),
            DockerKind::Transform if state.has_selection() => panels::transform_ui(ui, state),
            DockerKind::Transform => needs_selection_ui(ui, "position, size, rotate and mirror"),
            DockerKind::AlignDistribute if state.has_selection() => panels::align_ui(ui, state, commands),
            DockerKind::AlignDistribute => needs_selection_ui(ui, "align and order"),
            DockerKind::StepRepeat => crate::edit_tools::step_repeat_ui(ui, state),
            DockerKind::FindReplace => crate::find_ui::find_replace_ui(ui, state),
        });
    });
}

/// With nothing selected, Properties shows the page and document settings.
fn properties_ui(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label(egui::RichText::new("Nothing selected").weak());
    panels::new_object_note(ui);
    ui.add_space(4.0);
    section(ui, "Page", |ui| page_settings_ui(ui, state, true));
    section(ui, "Document", |ui| {
        egui::Grid::new("doc_props").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("Title");
            crate::a11y::add(ui, egui::TextEdit::singleline(&mut state.doc.title), "Document title");
            ui.end_row();
            ui.label("Units");
            unit_combo(ui, "doc_units", &mut state.doc.display_unit);
            ui.end_row();
            ui.label("Pages");
            ui.label(state.doc.page_count().to_string());
            ui.end_row();
            ui.label("Grid spacing");
            let unit = state.doc.display_unit;
            let mut g = unit.from_points(state.prefs.grid_spacing);
            let field = egui::DragValue::new(&mut g)
                .speed(unit.step() / 2.0)
                .range(unit.from_points(0.5)..=f64::MAX)
                .max_decimals(unit.decimals())
                .suffix(format!(" {}", unit.suffix()));
            if crate::a11y::add(ui, field, "Grid spacing (document)").changed() {
                state.prefs.grid_spacing = unit.to_points(g);
            }
            ui.end_row();
        });
    });
}

/// Page size controls, shared by the Properties docker and the property bar.
pub fn page_settings_ui(ui: &mut egui::Ui, state: &mut AppState, vertical: bool) {
    let unit = state.doc.display_unit;
    let page = state.doc.active_page_mut();
    let orientation = page.orientation();
    let current = PagePreset::matching(page.size).map(|p| p.name).unwrap_or("Custom");

    let mut body = |ui: &mut egui::Ui| {
        let preset = egui::ComboBox::from_id_salt(("page_preset", vertical))
            .selected_text(current)
            .width(90.0)
            .show_ui(ui, |ui| {
                for preset in PAGE_PRESETS {
                    if ui.selectable_label(current == preset.name, preset.name).clicked() {
                        page.size = preset.size(orientation);
                    }
                }
            });
        crate::a11y::name(&preset.response, "Page size (docker)");
        let mut w = unit.from_points(page.size.width);
        let mut h = unit.from_points(page.size.height);
        let speed = unit.step() / 2.0;
        let min = unit.from_points(1.0);
        let field_w = ui.add(egui::DragValue::new(&mut w).speed(speed).range(min..=f64::MAX).max_decimals(unit.decimals()).suffix(format!(" {}", unit.suffix())));
        crate::a11y::name(&field_w, "Page width (docker)");
        let changed_w = field_w.on_hover_text("Page width").changed();
        let field_h = ui.add(egui::DragValue::new(&mut h).speed(speed).range(min..=f64::MAX).max_decimals(unit.decimals()).suffix(format!(" {}", unit.suffix())));
        crate::a11y::name(&field_h, "Page height (docker)");
        let changed_h = field_h
            .on_hover_text("Page height")
            .changed();
        if changed_w || changed_h {
            page.size = vs_core::kurbo::Size::new(unit.to_points(w), unit.to_points(h));
        }
        if ui
            .selectable_label(orientation == Orientation::Portrait, ICON_CROP_PORTRAIT.rich_text().size(18.0))
            .on_hover_text("Portrait")
            .clicked()
        {
            page.set_orientation(Orientation::Portrait);
        }
        if ui
            .selectable_label(orientation == Orientation::Landscape, ICON_CROP_LANDSCAPE.rich_text().size(18.0))
            .on_hover_text("Landscape")
            .clicked()
        {
            page.set_orientation(Orientation::Landscape);
        }
    };
    if vertical {
        ui.horizontal_wrapped(|ui| body(ui));
    } else {
        body(ui);
    }
}

pub fn unit_combo(ui: &mut egui::Ui, id: &str, unit: &mut Unit) {
    let combo = egui::ComboBox::from_id_salt(id)
        .selected_text(unit.label())
        .width(100.0)
        .height(400.0)
        .show_ui(ui, |ui| {
            for u in Unit::ALL {
                ui.selectable_value(unit, u, u.label());
            }
        });
    crate::a11y::name(&combo.response, if id == "bar_units" { "Units (property bar)" } else { "Units (document)" });
}

fn pages_ui(ui: &mut egui::Ui, state: &mut AppState, commands: &mut Vec<Command>) {
    ui.horizontal(|ui| {
        if crate::a11y::icon_button(ui, ICON_ADD.rich_text(), "Insert page").clicked() {
            commands.push(Command::InsertPage);
        }
        if crate::a11y::icon_button(ui, ICON_CONTENT_COPY.rich_text(), "Duplicate page").clicked() {
            commands.push(Command::DuplicatePage);
        }
        let can_delete = state.doc.page_count() > 1;
        let delete = ui.add_enabled(can_delete, egui::Button::new(ICON_DELETE.rich_text()));
        crate::a11y::name(&delete, "Delete page (docker)");
        if delete.on_hover_text("Delete page").clicked() {
            commands.push(Command::DeletePage);
        }
    });
    ui.separator();

    let unit = state.doc.display_unit;
    for i in 0..state.doc.page_count() {
        let active = i == state.doc.active_page_index();
        let size = state.doc.pages()[i].size;
        let dims = format!(
            "{:.1} × {:.1} {}",
            unit.from_points(size.width),
            unit.from_points(size.height),
            unit.suffix()
        );
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{}", i + 1)).weak().monospace());
            if let Some((idx, name)) = &mut state.renaming_page
                && *idx == i
            {
                let resp = crate::a11y::add(ui, egui::TextEdit::singleline(name), "Page name");
                if resp.lost_focus() {
                    if let Some(page) = state.doc.page_mut(i) {
                        page.name = name.trim().to_owned();
                    }
                    state.renaming_page = None;
                } else {
                    resp.request_focus();
                }
                return;
            }
            let label = ui
                .selectable_label(active, state.doc.page_label(i))
                .on_hover_text(format!("{dims}\nDouble-click to rename"));
            if label.clicked() {
                let _ = state.doc.set_active_page(i);
            }
            if label.double_clicked() {
                state.renaming_page = Some((i, state.doc.pages()[i].name.clone()));
            }
            ui.label(egui::RichText::new(dims).weak().small());
        });
    }
}

fn needs_selection_ui(ui: &mut egui::Ui, what: &str) {
    ui.add_space(8.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(format!("Select objects to {what}.")).weak());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_opens_every_docker_with_properties_in_front() {
        let host = DockerHost::default();
        assert_eq!(host.open, DockerKind::DEFAULT_OPEN.to_vec());
        assert!(host.is_shown(DockerKind::Properties));
        assert!(!host.is_shown(DockerKind::Objects));
    }

    #[test]
    fn toggle_brings_to_front_then_closes() {
        let mut host = DockerHost::default();
        host.toggle(DockerKind::Objects);
        assert!(host.is_shown(DockerKind::Objects));
        host.toggle(DockerKind::Objects);
        assert!(!host.open.contains(&DockerKind::Objects));
        assert!(host.active.is_some());
        host.toggle(DockerKind::Objects);
        assert_eq!(host.open.last(), Some(&DockerKind::Objects));
    }

    #[test]
    fn closing_every_docker_leaves_nothing_to_show_until_one_opens() {
        let mut host = DockerHost::default();
        for kind in DockerKind::ALL {
            host.close(kind);
        }
        assert!(!host.has_content());
        host.show(DockerKind::Pages);
        assert!(host.has_content());
        assert_eq!(host.open, vec![DockerKind::Pages]);
    }

    #[test]
    fn hidden_area_shows_nothing_and_show_reveals_it() {
        let mut host = DockerHost { visible: false, ..Default::default() };
        assert!(!host.has_content());
        assert!(!host.is_shown(DockerKind::Properties));
        host.show(DockerKind::Transform);
        assert!(host.is_shown(DockerKind::Transform));
    }

    #[test]
    fn repair_drops_duplicates_and_fixes_active() {
        let mut host = DockerHost {
            open: vec![DockerKind::Pages, DockerKind::Pages, DockerKind::Objects],
            active: Some(DockerKind::Transform),
            visible: true,
        };
        host.repair();
        assert_eq!(host.open, vec![DockerKind::Pages, DockerKind::Objects]);
        assert_eq!(host.active, Some(DockerKind::Pages));
    }

    #[test]
    fn survives_serialisation() {
        let host = DockerHost::default();
        let json = serde_json::to_string(&host).unwrap();
        assert_eq!(serde_json::from_str::<DockerHost>(&json).unwrap(), host);
    }
}
