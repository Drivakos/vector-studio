//! Command registry and the workspace layout file that the menus are built from.
//!
//! The layout lists every command id the full product will have. Ids that map to a
//! [`Command`] are live; all others render disabled until they are implemented.

use serde::Deserialize;

use vs_core::{Align, Distribute, Order, Shaping};

use crate::dockers::DockerKind;

pub const DEFAULT_LAYOUT_JSON: &str = include_str!("../layout/workspace.json");

/// What View › Snap To turns on and off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapTarget {
    Grid,
    Guidelines,
    Objects,
    Page,
}

impl SnapTarget {
    pub const ALL: [SnapTarget; 4] = [SnapTarget::Grid, SnapTarget::Guidelines, SnapTarget::Objects, SnapTarget::Page];

    /// The View › Snap To label.
    pub fn label(self) -> &'static str {
        match self {
            SnapTarget::Grid => "Document Grid",
            SnapTarget::Guidelines => "Guidelines",
            SnapTarget::Objects => "Objects",
            SnapTarget::Page => "Page",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    NewDocument,
    Open,
    Save,
    SaveAs,
    Import,
    Export,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    Duplicate,
    SelectAll,
    SelectAllNodes,
    CopyProperties,
    StepRepeat,
    Align(Align),
    CenterToPage,
    Distribute(Distribute),
    Order(Order),
    ConvertToCurves,
    Group,
    Ungroup,
    UngroupAll,
    Lock,
    Unlock,
    UnlockAll,
    Shape(Shaping),
    Combine,
    BreakApart,
    JoinCurves,
    ConvertOutline,
    ConvertText,
    ObjectProperties,
    ZoomIn,
    ZoomOut,
    ZoomToPage,
    ZoomActualSize,
    ToggleRulers,
    ToggleGrid,
    ToggleGuidelines,
    ToggleSnap(SnapTarget),
    ToggleStatusBar,
    TogglePalette,
    InsertPage,
    InsertPageBefore,
    DuplicatePage,
    DeletePage,
    PreviousPage,
    NextPage,
    SwitchOrientation,
    ToggleDocker(DockerKind),
    ToggleDockers,
    ResetWorkspace,
    About,
}

impl Command {
    pub fn from_id(id: &str) -> Option<Self> {
        use Command::*;
        Some(match id {
            "file.new" => NewDocument,
            "file.open" => Open,
            "file.save" => Save,
            "file.save_as" => SaveAs,
            "file.import" => Import,
            "file.export" => Export,
            "edit.undo" => Undo,
            "edit.redo" => Redo,
            "edit.cut" => Cut,
            "edit.copy" => Copy,
            "edit.paste" => Paste,
            "edit.delete" => Delete,
            "edit.duplicate" => Duplicate,
            "edit.select_all_objects" => SelectAll,
            "edit.select_all_nodes" => SelectAllNodes,
            "edit.copy_properties" => CopyProperties,
            "edit.step_repeat" => StepRepeat,
            "object.align_left" => Align(vs_core::Align::Left),
            "object.align_right" => Align(vs_core::Align::Right),
            "object.align_top" => Align(vs_core::Align::Top),
            "object.align_bottom" => Align(vs_core::Align::Bottom),
            "object.align_center_h" => Align(vs_core::Align::CenterHorizontal),
            "object.align_center_v" => Align(vs_core::Align::CenterVertical),
            "object.center_to_page" => CenterToPage,
            "object.to_front" => Order(vs_core::Order::ToFront),
            "object.to_back" => Order(vs_core::Order::ToBack),
            "object.forward" => Order(vs_core::Order::Forward),
            "object.backward" => Order(vs_core::Order::Backward),
            "object.reverse_order" => Order(vs_core::Order::Reverse),
            "object.distribute_left" => Distribute(vs_core::Distribute::Left),
            "object.distribute_center_h" => Distribute(vs_core::Distribute::CenterHorizontal),
            "object.distribute_right" => Distribute(vs_core::Distribute::Right),
            "object.distribute_spacing_h" => Distribute(vs_core::Distribute::SpacingHorizontal),
            "object.distribute_top" => Distribute(vs_core::Distribute::Top),
            "object.distribute_center_v" => Distribute(vs_core::Distribute::CenterVertical),
            "object.distribute_bottom" => Distribute(vs_core::Distribute::Bottom),
            "object.distribute_spacing_v" => Distribute(vs_core::Distribute::SpacingVertical),
            "object.convert_to_curves" => ConvertToCurves,
            "object.group" => Group,
            "object.ungroup" => Ungroup,
            "object.ungroup_all" => UngroupAll,
            "object.lock" => Lock,
            "object.unlock" => Unlock,
            "object.unlock_all" => UnlockAll,
            "object.weld" => Shape(Shaping::Weld),
            "object.trim" => Shape(Shaping::Trim),
            "object.intersect" => Shape(Shaping::Intersect),
            "object.simplify" => Shape(Shaping::Simplify),
            "object.front_minus_back" => Shape(Shaping::FrontMinusBack),
            "object.back_minus_front" => Shape(Shaping::BackMinusFront),
            "object.boundary" => Shape(Shaping::Boundary),
            "object.combine" => Combine,
            "object.break_apart" => BreakApart,
            "object.join_curves" => JoinCurves,
            "object.convert_outline" => ConvertOutline,
            "text.convert" => ConvertText,
            "object.properties" => ObjectProperties,
            "view.zoom_in" => ZoomIn,
            "view.zoom_out" => ZoomOut,
            "view.zoom_page" => ZoomToPage,
            "view.zoom_actual" => ZoomActualSize,
            "view.rulers" => ToggleRulers,
            "view.grid" => ToggleGrid,
            "view.guidelines" => ToggleGuidelines,
            "view.snap_grid" => ToggleSnap(SnapTarget::Grid),
            "view.snap_guidelines" => ToggleSnap(SnapTarget::Guidelines),
            "view.snap_objects" => ToggleSnap(SnapTarget::Objects),
            "view.snap_page" => ToggleSnap(SnapTarget::Page),
            "view.status_bar" => ToggleStatusBar,
            "view.color_palette" => TogglePalette,
            "layout.insert_page" => InsertPage,
            "layout.insert_page_before" => InsertPageBefore,
            "layout.duplicate_page" => DuplicatePage,
            "layout.delete_page" => DeletePage,
            "layout.prev_page" => PreviousPage,
            "layout.next_page" => NextPage,
            "layout.switch_orientation" => SwitchOrientation,
            "window.docker.properties" => ToggleDocker(DockerKind::Properties),
            "window.docker.objects" => ToggleDocker(DockerKind::Objects),
            "window.docker.pages" => ToggleDocker(DockerKind::Pages),
            "window.docker.transform" => ToggleDocker(DockerKind::Transform),
            "window.docker.align" => ToggleDocker(DockerKind::AlignDistribute),
            "window.docker.step_repeat" => ToggleDocker(DockerKind::StepRepeat),
            "window.show_dockers" => ToggleDockers,
            "window.reset_workspace" => ResetWorkspace,
            "help.about" => About,
            _ => return None,
        })
    }

    /// Every live command, for coverage checks.
    #[cfg(test)]
    pub const ALL: &'static [Command] = &[
        Command::NewDocument,
        Command::Open,
        Command::Save,
        Command::SaveAs,
        Command::Import,
        Command::Export,
        Command::Undo,
        Command::Redo,
        Command::Cut,
        Command::Copy,
        Command::Paste,
        Command::Delete,
        Command::Duplicate,
        Command::SelectAll,
        Command::SelectAllNodes,
        Command::CopyProperties,
        Command::StepRepeat,
        Command::Align(Align::Left),
        Command::Align(Align::Right),
        Command::Align(Align::Top),
        Command::Align(Align::Bottom),
        Command::Align(Align::CenterHorizontal),
        Command::Align(Align::CenterVertical),
        Command::CenterToPage,
        Command::Order(Order::ToFront),
        Command::Order(Order::ToBack),
        Command::Order(Order::Forward),
        Command::Order(Order::Backward),
        Command::Order(Order::Reverse),
        Command::Distribute(Distribute::Left),
        Command::Distribute(Distribute::CenterHorizontal),
        Command::Distribute(Distribute::Right),
        Command::Distribute(Distribute::SpacingHorizontal),
        Command::Distribute(Distribute::Top),
        Command::Distribute(Distribute::CenterVertical),
        Command::Distribute(Distribute::Bottom),
        Command::Distribute(Distribute::SpacingVertical),
        Command::ConvertToCurves,
        Command::Group,
        Command::Ungroup,
        Command::UngroupAll,
        Command::Lock,
        Command::Unlock,
        Command::UnlockAll,
        Command::Shape(Shaping::Weld),
        Command::Shape(Shaping::Trim),
        Command::Shape(Shaping::Intersect),
        Command::Shape(Shaping::Simplify),
        Command::Shape(Shaping::FrontMinusBack),
        Command::Shape(Shaping::BackMinusFront),
        Command::Shape(Shaping::Boundary),
        Command::Combine,
        Command::BreakApart,
        Command::JoinCurves,
        Command::ConvertOutline,
        Command::ConvertText,
        Command::ObjectProperties,
        Command::ZoomIn,
        Command::ZoomOut,
        Command::ZoomToPage,
        Command::ZoomActualSize,
        Command::ToggleRulers,
        Command::ToggleGrid,
        Command::ToggleGuidelines,
        Command::ToggleSnap(SnapTarget::Grid),
        Command::ToggleSnap(SnapTarget::Guidelines),
        Command::ToggleSnap(SnapTarget::Objects),
        Command::ToggleSnap(SnapTarget::Page),
        Command::ToggleStatusBar,
        Command::TogglePalette,
        Command::InsertPage,
        Command::InsertPageBefore,
        Command::DuplicatePage,
        Command::DeletePage,
        Command::PreviousPage,
        Command::NextPage,
        Command::SwitchOrientation,
        Command::ToggleDocker(DockerKind::Properties),
        Command::ToggleDocker(DockerKind::Objects),
        Command::ToggleDocker(DockerKind::Pages),
        Command::ToggleDocker(DockerKind::Transform),
        Command::ToggleDocker(DockerKind::AlignDistribute),
        Command::ToggleDocker(DockerKind::StepRepeat),
        Command::ToggleDockers,
        Command::ResetWorkspace,
        Command::About,
    ];
}

/// Commands whose shortcuts the canvas key handler owns, so text fields keep these keys
/// (copy/paste/undo inside a field must not act on objects). Menus show them without
/// registering a native key equivalent.
pub const FIELD_SAFE_IDS: &[&str] = &[
    "edit.undo",
    "edit.redo",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.select_all_objects",
];

#[derive(Clone, Debug, Deserialize)]
pub struct Layout {
    pub menus: Vec<Menu>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MenuRole {
    #[default]
    None,
    Window,
    Help,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Menu {
    pub label: String,
    #[serde(default)]
    pub role: MenuRole,
    pub items: Vec<MenuItem>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum MenuItem {
    Command(CommandItem),
    Submenu { label: String, items: Vec<MenuItem> },
    /// Written as `"-"` in the layout file (checked by tests).
    Separator(#[allow(dead_code)] String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct CommandItem {
    pub cmd: String,
    pub label: String,
    #[serde(default)]
    pub shortcut: Option<String>,
    #[serde(default)]
    pub checkable: bool,
}

impl Layout {
    pub fn load_default() -> Self {
        serde_json::from_str(DEFAULT_LAYOUT_JSON).expect("built-in workspace.json is valid")
    }

    /// Every command item in the menus, depth first.
    pub fn command_items(&self) -> Vec<&CommandItem> {
        fn walk<'a>(items: &'a [MenuItem], out: &mut Vec<&'a CommandItem>) {
            for item in items {
                match item {
                    MenuItem::Command(c) => out.push(c),
                    MenuItem::Submenu { items, .. } => walk(items, out),
                    MenuItem::Separator(_) => {}
                }
            }
        }
        let mut out = Vec::new();
        for menu in &self.menus {
            walk(&menu.items, &mut out);
        }
        out
    }
}

/// Parse "CmdOrCtrl+Shift+S" style shortcuts into an egui shortcut.
pub fn parse_shortcut(text: &str) -> Option<egui::KeyboardShortcut> {
    let mut modifiers = egui::Modifiers::NONE;
    let mut key = None;
    for part in text.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "cmdorctrl" | "cmd" | "ctrl" => modifiers |= egui::Modifiers::COMMAND,
            "shift" => modifiers |= egui::Modifiers::SHIFT,
            "alt" | "option" => modifiers |= egui::Modifiers::ALT,
            "" => key = Some(egui::Key::Minus), // "CmdOrCtrl+-" splits into an empty part
            "=" => key = Some(egui::Key::Equals),
            "-" => key = Some(egui::Key::Minus),
            "pageup" => key = Some(egui::Key::PageUp),
            "pagedown" => key = Some(egui::Key::PageDown),
            "return" | "enter" => key = Some(egui::Key::Enter),
            "delete" => key = Some(egui::Key::Delete),
            other => key = egui::Key::from_name(&other.to_ascii_uppercase())
                .or_else(|| egui::Key::from_name(other)),
        }
    }
    key.map(|k| egui::KeyboardShortcut::new(modifiers, k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_layout_parses_and_separators_are_dashes() {
        let layout = Layout::load_default();
        assert!(layout.menus.len() >= 10);
        fn check(items: &[MenuItem]) {
            for item in items {
                match item {
                    MenuItem::Separator(s) => assert_eq!(s, "-"),
                    MenuItem::Submenu { items, .. } => check(items),
                    MenuItem::Command(_) => {}
                }
            }
        }
        for menu in &layout.menus {
            check(&menu.items);
        }
    }

    #[test]
    fn command_ids_are_unique() {
        let layout = Layout::load_default();
        let mut seen = HashSet::new();
        for item in layout.command_items() {
            assert!(seen.insert(item.cmd.as_str()), "duplicate id {}", item.cmd);
        }
    }

    #[test]
    fn every_live_command_is_in_the_menus() {
        let layout = Layout::load_default();
        let live: HashSet<_> = layout
            .command_items()
            .iter()
            .filter_map(|c| Command::from_id(&c.cmd))
            .collect();
        for cmd in Command::ALL {
            assert!(live.contains(cmd), "{cmd:?} has no menu item");
        }
    }

    #[test]
    fn every_shortcut_parses() {
        let layout = Layout::load_default();
        for item in layout.command_items() {
            if let Some(s) = &item.shortcut {
                assert!(parse_shortcut(s).is_some(), "cannot parse {s} on {}", item.cmd);
            }
        }
    }

    #[test]
    fn shortcut_parsing() {
        let s = parse_shortcut("CmdOrCtrl+Shift+S").unwrap();
        assert_eq!(s.logical_key, egui::Key::S);
        assert!(s.modifiers.command && s.modifiers.shift);
        assert_eq!(parse_shortcut("CmdOrCtrl+-").unwrap().logical_key, egui::Key::Minus);
        assert_eq!(parse_shortcut("CmdOrCtrl+=").unwrap().logical_key, egui::Key::Equals);
        assert_eq!(parse_shortcut("Shift+F4").unwrap().logical_key, egui::Key::F4);
        assert_eq!(parse_shortcut("PageDown").unwrap().logical_key, egui::Key::PageDown);
    }
}
