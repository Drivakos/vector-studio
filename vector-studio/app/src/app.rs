//! Top-level app: state, command execution, keyboard, frame layout and persistence.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use vs_core::kurbo::{Affine, Point, Vec2};
use vs_core::{Align, Document, History, Object, ObjectId, Shape, Unit};
use vs_render::Viewport;

use crate::canvas::{CanvasInteraction, GpuCanvas, fit_page};
use crate::chrome;
use crate::commands::{Command, Layout, SnapTarget};
use crate::dockers::{self, DockerHost, DockerKind};
use crate::files::{self, ExportDialog};
use crate::toolbox::{self, GROUPS, Tool, ToolboxState};
use crate::tools::{self, ToolState};
use crate::{theme, titlebar};

const KEY_DOCKERS: &str = "dockers";
/// v2: defaults changed to match the reference app (no rulers, no status bar).
const KEY_PREFS: &str = "prefs.v2";
const KEY_TOOLBOX: &str = "toolbox";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub show_rulers: bool,
    pub show_status_bar: bool,
    pub show_palette: bool,
    /// Arrow-key nudge, in points (Shift: ×10).
    pub nudge: f64,
    /// Offset of Edit › Duplicate copies, in points.
    pub duplicate_offset: [f64; 2],
    pub show_grid: bool,
    pub show_guidelines: bool,
    /// Document grid spacing, in points.
    pub grid_spacing: f64,
    pub snap_grid: bool,
    pub snap_guidelines: bool,
    pub snap_objects: bool,
    pub snap_page: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            // The reference app shows neither rulers nor a status bar by default.
            show_rulers: false,
            show_status_bar: false,
            show_palette: true,
            nudge: Unit::Millimeters.to_points(1.0),
            duplicate_offset: [18.0, 18.0],
            show_grid: false,
            show_guidelines: true,
            grid_spacing: Unit::Millimeters.to_points(5.0),
            snap_grid: false,
            snap_guidelines: true,
            snap_objects: false,
            snap_page: false,
        }
    }
}

impl Prefs {
    pub fn snap_flag(&self, target: SnapTarget) -> &bool {
        match target {
            SnapTarget::Grid => &self.snap_grid,
            SnapTarget::Guidelines => &self.snap_guidelines,
            SnapTarget::Objects => &self.snap_objects,
            SnapTarget::Page => &self.snap_page,
        }
    }

    pub fn snap_flag_mut(&mut self, target: SnapTarget) -> &mut bool {
        match target {
            SnapTarget::Grid => &mut self.snap_grid,
            SnapTarget::Guidelines => &mut self.snap_guidelines,
            SnapTarget::Objects => &mut self.snap_objects,
            SnapTarget::Page => &mut self.snap_page,
        }
    }
}

/// Everything panes and bars read and edit.
pub struct AppState {
    pub doc: Document,
    pub history: History,
    pub file_path: Option<PathBuf>,
    /// Selected objects on the active page, in the order they were selected.
    pub selection: Vec<ObjectId>,
    /// The selection a marquee (or Select All) made, until objects are clicked. While it
    /// matches `selection`, shaping styles results after the bottom-most object instead of
    /// the last selected.
    pub marquee_selection: Option<Vec<ObjectId>>,
    pub viewport: Viewport,
    /// Fit the page to the window on the next frame (new document, startup).
    pub needs_fit: bool,
    pub tool: Tool,
    pub tools: ToolState,
    pub toolbox: ToolboxState,
    pub prefs: Prefs,
    pub dockers: DockerHost,
    pub cursor_doc: Option<Point>,
    /// Canvas size in logical points, from the last frame.
    pub canvas_size: egui::Vec2,
    /// Canvas area in window coordinates, from the last frame.
    pub canvas_rect: egui::Rect,
    /// Native file and message dialogs; tests swap in scripted ones.
    pub dialogs: Box<dyn files::Dialogs>,
    pub renaming_page: Option<(usize, String)>,
    /// The page number in the open Go to Page window (1-based).
    pub go_to_page: Option<usize>,
    pub page_background: Option<crate::page_dialogs::PageBackgroundDialog>,
    pub show_document_properties: bool,
    pub renaming_object: Option<(ObjectId, String)>,
    /// The layer (page index, layer) being renamed in the Objects docker.
    pub renaming_layer: Option<(usize, vs_core::LayerId, String)>,
    pub interaction: CanvasInteraction,
    pub export: ExportDialog,
    pub step_repeat: crate::edit_tools::StepRepeat,
    pub copy_properties: crate::edit_tools::CopyProperties,
    pub powerclip: crate::edit_tools::PowerClipState,
    /// How the canvas draws objects (not saved).
    pub view_mode: vs_render::ViewMode,
    /// What Edit > Repeat does again.
    pub last_action: Option<Repeatable>,
    /// Last status message (e.g. "Exported …").
    pub status: Option<String>,
    clipboard: Vec<Object>,
    /// Text we put on the system clipboard with the last copy, to recognise our own paste.
    clipboard_text: String,
    pub(crate) untitled_count: u32,
    show_about: bool,
    last_tool: Tool,
    nodes_owner: Option<ObjectId>,
    window_title: String,
    close_confirmed: bool,
}

/// Something Edit > Repeat (Cmd+R) can do again to the current selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Repeatable {
    Command(Command),
    /// A move, scale, rotation or mirror, relative to the selection's centre.
    Transform(Affine),
}

impl Repeatable {
    /// Commands that act on the selection and make sense to run again.
    pub fn command(cmd: Command) -> bool {
        matches!(
            cmd,
            Command::Duplicate
                | Command::Clone
                | Command::Align(_)
                | Command::CenterToPage
                | Command::Distribute(_)
                | Command::Order(_)
                | Command::ConvertToCurves
                | Command::ConvertOutline
                | Command::Shape(_)
                | Command::Combine
                | Command::BreakApart
                | Command::Group
                | Command::Lock
                | Command::Unlock
        )
    }
}

impl AppState {
    /// Transform the selection by `t` (page space) and remember it for Repeat.
    pub fn transform_selection(&mut self, t: Affine) {
        let ids = self.selection.clone();
        if let Some(b) = self.doc.active_page().bounds_of(&ids) {
            self.remember_transform(t, b.center());
        }
        self.doc.transform_objects(&ids, t);
    }

    /// Remember `t`, done to a selection centred on `center`, for Repeat.
    pub fn remember_transform(&mut self, t: Affine, center: Point) {
        let c = center.to_vec2();
        self.last_action = Some(Repeatable::Transform(Affine::translate(-c) * t * Affine::translate(c)));
    }

    /// Do a remembered transform again, about the current selection's centre.
    fn repeat_transform(&mut self, relative: Affine) {
        let ids = self.selection.clone();
        if let Some(b) = self.doc.active_page().bounds_of(&ids) {
            let c = b.center().to_vec2();
            self.doc.transform_objects(&ids, Affine::translate(c) * relative * Affine::translate(-c));
        }
    }
}

impl AppState {
    pub fn new(doc: Document, toolbox: ToolboxState, prefs: Prefs, dockers: DockerHost) -> Self {
        Self {
            history: History::new(&doc, true),
            doc,
            file_path: None,
            selection: Vec::new(),
            marquee_selection: None,
            viewport: Viewport::default(),
            needs_fit: true,
            tool: Tool::Pick,
            tools: ToolState::default(),
            toolbox,
            prefs,
            dockers,
            cursor_doc: None,
            canvas_size: egui::Vec2::ZERO,
            canvas_rect: egui::Rect::NOTHING,
            dialogs: Box::new(files::NativeDialogs),
            renaming_page: None,
            go_to_page: None,
            page_background: None,
            show_document_properties: false,
            renaming_object: None,
            renaming_layer: None,
            interaction: CanvasInteraction::default(),
            export: ExportDialog::default(),
            step_repeat: Default::default(),
            copy_properties: Default::default(),
            powerclip: Default::default(),
            view_mode: Default::default(),
            last_action: None,
            status: None,
            clipboard: Vec::new(),
            clipboard_text: String::new(),
            untitled_count: 1,
            show_about: false,
            last_tool: Tool::Pick,
            nodes_owner: None,
            window_title: String::new(),
            close_confirmed: false,
        }
    }

    /// What dragging snaps to now. Grid and guidelines snap only while they are shown.
    pub fn snap_options(&self) -> vs_core::SnapOptions {
        let p = &self.prefs;
        vs_core::SnapOptions {
            grid: (p.snap_grid && p.show_grid).then_some(p.grid_spacing),
            guides: p.snap_guidelines && p.show_guidelines,
            objects: p.snap_objects,
            page: p.snap_page,
        }
    }

    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// The SVG text the last copy put on the system clipboard.
    pub fn system_clipboard_text(&self) -> &str {
        &self.clipboard_text
    }

    pub fn selected_objects(&self) -> impl Iterator<Item = &Object> {
        let page = self.doc.active_page();
        self.selection.iter().filter_map(|id| page.object(*id))
    }

    /// The object whose fill and outline shaping and Combine results take: the
    /// last one selected, or the bottom-most after a marquee.
    pub fn shaping_target(&self) -> Option<ObjectId> {
        if self.marquee_selection.as_ref() == Some(&self.selection) {
            let page = self.doc.active_page();
            page.objects.iter().find(|o| !o.locked && self.selection.contains(&o.id)).map(|o| o.id)
        } else {
            self.selected_objects().filter(|o| !o.locked).last().map(|o| o.id)
        }
    }
}

/// How the app starts. The window binary installs the native menu and opens the file
/// named on the command line; tests start without either.
#[derive(Clone, Debug, Default)]
pub struct Startup {
    pub native_menu: bool,
    pub open: Option<PathBuf>,
}

pub struct VectorStudioApp {
    state: AppState,
    layout: Layout,
    canvas: Option<GpuCanvas>,
    canvas_error: Option<String>,
    #[cfg(target_os = "macos")]
    native_menu: Option<crate::native_menu::NativeMenu>,
}

impl VectorStudioApp {
    pub fn new(cc: &eframe::CreationContext<'_>, startup: Startup) -> Self {
        egui_material_icons::initialize(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let layout = Layout::load_default();

        let storage = cc.storage;
        let mut dockers: DockerHost = storage.and_then(|s| eframe::get_value(s, KEY_DOCKERS)).unwrap_or_default();
        dockers.repair();
        let prefs = storage.and_then(|s| eframe::get_value(s, KEY_PREFS)).unwrap_or_default();
        let toolbox = storage.and_then(|s| eframe::get_value(s, KEY_TOOLBOX)).unwrap_or_default();

        let (canvas, canvas_error) = match cc.wgpu_render_state.as_ref() {
            Some(rs) => match GpuCanvas::new(rs) {
                Ok(c) => (Some(c), None),
                Err(e) => (None, Some(e)),
            },
            None => (None, Some("No GPU render state: the app must run with the wgpu backend.".into())),
        };

        #[cfg(target_os = "macos")]
        let native_menu = if !startup.native_menu {
            None
        } else {
            match crate::native_menu::NativeMenu::install(&layout, cc.egui_ctx.clone()) {
                Ok(m) => Some(m),
                Err(err) => {
                    log::warn!("native menu unavailable, using in-window menu: {err}");
                    None
                }
            }
        };

        let mut state = AppState::new(Document::new("Untitled-1"), toolbox, prefs, dockers);
        if let Some(path) = &startup.open {
            files::open_path(&mut state, path);
        }

        Self {
            state,
            layout,
            canvas,
            canvas_error,
            #[cfg(target_os = "macos")]
            native_menu,
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    fn has_native_menu(&self) -> bool {
        #[cfg(target_os = "macos")]
        return self.native_menu.is_some();
        #[cfg(not(target_os = "macos"))]
        false
    }

    /// (enabled, checked) for a command, used by every menu and toolbar.
    pub(crate) fn command_state(state: &AppState, cmd: Command) -> (bool, Option<bool>) {
        let doc = &state.doc;
        let selected = state.has_selection();
        match cmd {
            Command::DeletePage => (doc.page_count() > 1, None),
            Command::PreviousPage => (doc.active_page_index() > 0, None),
            Command::NextPage => (doc.active_page_index() + 1 < doc.page_count(), None),
            Command::GoToPage => (doc.page_count() > 1, None),
            Command::ToggleRulers => (true, Some(state.prefs.show_rulers)),
            Command::ToggleGrid => (true, Some(state.prefs.show_grid)),
            Command::Repeat => (state.has_selection() && state.last_action.is_some(), None),
            Command::Clone => (doc.can_clone(&state.selection), None),
            Command::ViewMode(mode) => (true, Some(state.view_mode == mode)),
            Command::ToggleGuidelines => (true, Some(state.prefs.show_guidelines)),
            Command::ToggleSnap(target) => (true, Some(*state.prefs.snap_flag(target))),
            Command::ToggleStatusBar => (true, Some(state.prefs.show_status_bar)),
            Command::TogglePalette => (true, Some(state.prefs.show_palette)),
            Command::ToggleDocker(kind) => (true, Some(state.dockers.is_shown(kind))),
            Command::ToggleDockers => (true, Some(state.dockers.has_content())),
            Command::Undo => (state.history.can_undo(), None),
            Command::Redo => (state.history.can_redo(), None),
            Command::Paste => (!state.clipboard.is_empty(), None),
            Command::SelectAll => (!doc.active_page().objects.is_empty(), None),
            Command::SelectAllNodes => (crate::edit_tools::can_select_all_nodes(state), None),
            Command::CopyProperties => (state.selected_objects().any(|o| !o.locked), None),
            Command::Group => (state.selected_objects().filter(|o| !o.locked).count() >= 2, None),
            Command::Ungroup | Command::UngroupAll => {
                (state.selected_objects().any(|o| !o.locked && matches!(o.shape, Shape::Group { .. })), None)
            }
            Command::Lock => (state.selected_objects().any(|o| !o.locked), None),
            Command::Unlock => (state.selected_objects().any(|o| o.locked), None),
            Command::UnlockAll => (doc.active_page().objects.iter().any(|o| o.locked), None),
            Command::Order(vs_core::Order::Reverse) => (state.selected_objects().count() >= 2, None),
            Command::Cut
            | Command::Copy
            | Command::Delete
            | Command::Duplicate
            | Command::Align(_)
            | Command::CenterToPage
            | Command::Order(_) => (selected, None),
            Command::ConvertToCurves => (state.selected_objects().any(vs_core::Object::can_convert_to_curves), None),
            Command::Shape(_) => (doc.can_shape(&state.selection), None),
            Command::Distribute(_) => (state.selected_objects().filter(|o| !o.locked).count() >= 3, None),
            Command::Combine => (doc.can_combine(&state.selection), None),
            Command::BreakApart => (doc.can_break_apart(&state.selection), None),
            Command::JoinCurves => (doc.can_join_curves(&state.selection, vs_core::curves::JOIN_TOLERANCE), None),
            Command::ConvertOutline => (doc.can_convert_outline(&state.selection), None),
            Command::ConvertText => (doc.can_convert_text(&state.selection), None),
            Command::Revert => (files::can_revert(state), None),
            Command::FitTextToPath => (doc.can_fit_text_to_path(&state.selection), None),
            Command::StraightenText => (doc.can_straighten_text(&state.selection), None),
            Command::ClipPlaceInside => (doc.can_place_inside(&state.selection), None),
            Command::ClipExtract => (doc.can_extract_contents(&state.selection), None),
            Command::ClipEdit => (crate::edit_tools::can_edit_contents(state), None),
            Command::ClipFinishEdit => (state.powerclip.editing.is_some(), None),
            _ => (true, None),
        }
    }

    fn run(&mut self, cmd: Command, ctx: &egui::Context) {
        if cmd == Command::Repeat {
            match self.state.last_action {
                Some(Repeatable::Command(again)) => self.run(again, ctx),
                Some(Repeatable::Transform(t)) => self.state.repeat_transform(t),
                None => {}
            }
            return;
        }
        if Repeatable::command(cmd) {
            self.state.last_action = Some(Repeatable::Command(cmd));
        }
        let s = &mut self.state;
        let center = Point::new(s.canvas_size.x as f64 / 2.0, s.canvas_size.y as f64 / 2.0);
        let page = s.doc.active_page_index();
        let result = match cmd {
            Command::NewDocument => {
                if files::confirm_discard(s) {
                    s.untitled_count += 1;
                    let doc = Document::new(format!("Untitled-{}", s.untitled_count));
                    files::set_document(s, doc, None);
                }
                Ok(())
            }
            Command::Open => {
                files::open(s);
                Ok(())
            },
            Command::Close => {
                files::close(s);
                Ok(())
            }
            Command::Revert => {
                files::revert(s);
                Ok(())
            }
            Command::Save => Ok(_ = files::save(s, false)),
            Command::SaveAs => Ok(_ = files::save(s, true)),
            Command::Import => {
                files::import(s);
                Ok(())
            }
            Command::DocumentProperties => {
                let _: () = s.show_document_properties = true;
                Ok(())
            }
            Command::Export => {
                let _: () = s.export.open = true;
                Ok(())
            },
            // Handled before this match.
            Command::Repeat => Ok(()),
            Command::Undo | Command::Redo => {
                tools::finish_pen(s);
                // Typing so far becomes its own step, so Undo takes back just that.
                crate::text_tool::finish(s);
                s.history.commit(&s.doc);
                if cmd == Command::Undo {
                    s.history.undo(&mut s.doc);
                } else {
                    s.history.redo(&mut s.doc);
                }
                s.tools.nodes.clear();
                Ok(())
            }
            Command::Copy | Command::Cut => {
                copy_selection(s, ctx);
                if cmd == Command::Cut {
                    s.doc.delete_objects(&s.selection);
                    keep_surviving(s);
                }
                Ok(())
            }
            Command::Paste => {
                let objects = s.clipboard.clone();
                s.selection = s.doc.paste_objects(&objects, Vec2::ZERO);
                Ok(())
            }
            Command::Delete => {
                if !tools::delete_selected_nodes(s) {
                    s.doc.delete_objects(&s.selection);
                    keep_surviving(s);
                }
                Ok(())
            }
            Command::Duplicate => {
                let [dx, dy] = s.prefs.duplicate_offset;
                s.selection = s.doc.duplicate_objects(&s.selection, Vec2::new(dx, dy));
                Ok(())
            }
            Command::Clone => {
                let [dx, dy] = s.prefs.duplicate_offset;
                let clones = s.doc.clone_objects(&s.selection, Vec2::new(dx, dy));
                if !clones.is_empty() {
                    s.selection = clones;
                    s.marquee_selection = None;
                }
                Ok(())
            }
            Command::SelectAllNodes => {
                crate::edit_tools::select_all_nodes(s);
                Ok(())
            }
            Command::CopyProperties => {
                s.copy_properties.open = true;
                Ok(())
            }
            Command::StepRepeat => {
                s.dockers.show(DockerKind::StepRepeat);
                Ok(())
            }
            Command::SelectAll => {
                s.selection = s.doc.all_object_ids();
                s.marquee_selection = Some(s.selection.clone());
                Ok(())
            }
            Command::Align(align) => {
                align_selection(s, align);
                Ok(())
            },
            Command::CenterToPage => {
                let bounds = s.doc.active_page().bounds();
                s.doc.align_objects(&s.selection, bounds, Align::CenterHorizontal);
                s.doc.align_objects(&s.selection, bounds, Align::CenterVertical);
                Ok(())
            }
            Command::Order(order) => {
                s.doc.reorder_objects(&s.selection, order);
                Ok(())
            },
            Command::Distribute(how) => {
                s.doc.distribute_objects(&s.selection, how);
                Ok(())
            }
            Command::ConvertToCurves => {
                s.doc.convert_to_curves(&s.selection);
                Ok(())
            },
            Command::Group => {
                if let Some(id) = s.doc.group_objects(&s.selection) {
                    // Locked objects stay out of the group and out of the selection.
                    s.selection = vec![id];
                }
                Ok(())
            }
            Command::Ungroup => {
                s.selection = s.doc.ungroup_objects(&s.selection);
                Ok(())
            }
            Command::UngroupAll => {
                // Ungroup until no selected group is left, so nested groups dissolve too.
                loop {
                    let next = s.doc.ungroup_objects(&s.selection);
                    if next == s.selection {
                        break;
                    }
                    s.selection = next;
                }
                Ok(())
            }
            Command::Shape(op) => {
                if let Some(target) = s.shaping_target() {
                    s.selection = s.doc.shape_objects(&s.selection, target, op);
                    s.tools.nodes.clear();
                }
                Ok(())
            }
            Command::Combine => {
                if let Some(id) = s.shaping_target().and_then(|t| s.doc.combine_objects(&s.selection, t)) {
                    s.selection = vec![id];
                    s.tools.nodes.clear();
                }
                Ok(())
            }
            Command::BreakApart => {
                s.selection = s.doc.break_apart(&s.selection);
                s.tools.nodes.clear();
                Ok(())
            }
            Command::JoinCurves => {
                let tolerance = vs_core::curves::JOIN_TOLERANCE;
                if let Some(id) = s.shaping_target().and_then(|t| s.doc.join_curves(&s.selection, t, tolerance)) {
                    s.selection = vec![id];
                    s.tools.nodes.clear();
                }
                Ok(())
            }
            Command::FitTextToPath => {
                crate::text_tool::finish(s);
                if let Some(text) = s.doc.fit_text_to_path(&s.selection) {
                    s.selection = vec![text];
                    s.marquee_selection = None;
                }
                Ok(())
            }
            Command::StraightenText => {
                s.doc.straighten_text(&s.selection);
                Ok(())
            }
            Command::ClipPlaceInside => {
                s.copy_properties.picking = false;
                s.powerclip.picking = true;
                Ok(())
            }
            Command::ClipExtract => {
                let out = s.doc.extract_contents(&s.selection);
                if !out.is_empty() {
                    s.selection = out;
                    s.marquee_selection = None;
                }
                Ok(())
            }
            Command::ClipEdit => {
                crate::edit_tools::edit_contents(s);
                Ok(())
            }
            Command::ClipFinishEdit => {
                crate::edit_tools::finish_contents(s);
                Ok(())
            }
            Command::ConvertText => {
                crate::text_tool::finish(s);
                s.doc.convert_text(&s.selection);
                Ok(())
            }
            Command::ConvertOutline => {
                let made = s.doc.convert_outline(&s.selection);
                if !made.is_empty() {
                    s.selection = made;
                    s.tools.nodes.clear();
                }
                Ok(())
            }
            Command::Lock | Command::Unlock => {
                s.doc.set_locked(&s.selection, cmd == Command::Lock);
                Ok(())
            }
            Command::UnlockAll => {
                let all = s.doc.all_object_ids();
                s.doc.set_locked(&all, false);
                Ok(())
            }
            Command::ObjectProperties => {
                s.dockers.show(DockerKind::Properties);
                Ok(())
            }
            Command::ZoomIn => {
                s.viewport.zoom_at(center, 2.0);
                Ok(())
            },
            Command::ZoomOut => {
                s.viewport.zoom_at(center, 0.5);
                Ok(())
            },
            Command::ZoomToPage => {
                fit_page(s);
                Ok(())
            },
            Command::ZoomActualSize => {
                s.viewport.set_zoom_percent(100.0, center);
                Ok(())
            },
            Command::ToggleRulers => {
                let _: () = s.prefs.show_rulers ^= true;
                Ok(())
            },
            Command::ToggleGrid => {
                let _: () = s.prefs.show_grid ^= true;
                Ok(())
            }
            Command::ViewMode(mode) => {
                s.view_mode = mode;
                Ok(())
            }
            Command::ToggleGuidelines => {
                let _: () = s.prefs.show_guidelines ^= true;
                Ok(())
            }
            Command::ToggleSnap(target) => {
                let _: () = *s.prefs.snap_flag_mut(target) ^= true;
                Ok(())
            }
            Command::ToggleStatusBar => {
                let _: () = s.prefs.show_status_bar ^= true;
                Ok(())
            },
            Command::TogglePalette => {
                let _: () = s.prefs.show_palette ^= true;
                Ok(())
            },
            Command::InsertPage => s.doc.insert_page_after(page).map(|_| ()),
            Command::InsertPageBefore => s.doc.insert_page_before(page).map(|_| ()),
            Command::DuplicatePage => s.doc.duplicate_page(page).map(|_| ()),
            Command::DeletePage => s.doc.delete_page(page).map(|_| ()),
            Command::PreviousPage => s.doc.set_active_page(page.saturating_sub(1)),
            Command::NextPage => s.doc.set_active_page(page + 1),
            Command::SwitchOrientation => {
                let p = s.doc.active_page_mut();
                let next = match p.orientation() {
                    vs_core::Orientation::Portrait => vs_core::Orientation::Landscape,
                    vs_core::Orientation::Landscape => vs_core::Orientation::Portrait,
                };
                p.set_orientation(next);
                Ok(())
            }
            Command::RenamePage => {
                s.dockers.show(DockerKind::Pages);
                s.renaming_page = Some((page, s.doc.active_page().name.clone()));
                Ok(())
            }
            Command::GoToPage => {
                crate::page_dialogs::open_go_to_page(s);
                Ok(())
            }
            Command::PageBackground => {
                crate::page_dialogs::open_page_background(s);
                Ok(())
            }
            Command::InsertPageNumber => {
                crate::text_tool::finish(s);
                let font = s.tools.settings.text.clone();
                s.selection = vec![s.doc.insert_page_number(&font)];
                s.marquee_selection = None;
                Ok(())
            }
            Command::ToggleDocker(kind) => {
                s.dockers.toggle(kind);
                Ok(())
            },
            Command::ToggleDockers => {
                if s.dockers.has_content() {
                    s.dockers.visible = false;
                } else {
                    s.dockers.show(s.dockers.active.unwrap_or(DockerKind::Properties));
                }
                Ok(())
            }
            Command::ResetWorkspace => {
                s.dockers = DockerHost::default();
                s.prefs = Prefs::default();
                s.toolbox = ToolboxState::default();
                Ok(())
            }
            Command::About => {
                let _: () = s.show_about = true;
                Ok(())
            },
        };
        if let Err(err) = result {
            log::warn!("{cmd:?}: {err}");
        }
    }

    /// Keys the canvas owns. Skipped while a text field has focus.
    fn handle_keys(&mut self, ctx: &egui::Context, commands: &mut Vec<Command>) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::{Key, KeyboardShortcut, Modifiers};
        let s = &mut self.state;
        let typing = crate::text_tool::keyboard(s, ctx);

        // Clipboard keys arrive as events, not key presses.
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            match event {
                egui::Event::Copy => commands.push(Command::Copy),
                egui::Event::Cut => commands.push(Command::Cut),
                egui::Event::Paste(text) if text == s.clipboard_text && !s.clipboard.is_empty() => {
                    commands.push(Command::Paste)
                }
                _ => {}
            }
        }

        let shortcut = |m, k| KeyboardShortcut::new(m, k);
        ctx.input_mut(|i| {
            if i.consume_shortcut(&shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)) {
                commands.push(Command::Redo);
            } else if i.consume_shortcut(&shortcut(Modifiers::COMMAND, Key::Z)) {
                commands.push(Command::Undo);
            }
            if i.consume_shortcut(&shortcut(Modifiers::COMMAND, Key::A)) {
                // With the Shape tool on a curve, Cmd+A selects its nodes.
                let nodes = s.tool == Tool::Shape && crate::edit_tools::can_select_all_nodes(s);
                commands.push(if nodes { Command::SelectAllNodes } else { Command::SelectAll });
            }
            if !typing && (i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace)) {
                commands.push(Command::Delete);
            }
            if i.consume_key(Modifiers::NONE, Key::Enter) {
                tools::finish_pen(s);
                if s.tool == Tool::Crop {
                    tools::apply_crop(s);
                }
            }
        });

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if s.copy_properties.picking {
                s.copy_properties.picking = false;
            } else if s.powerclip.picking {
                s.powerclip.picking = false;
            } else if s.tool == Tool::Crop && s.tools.crop.is_some() {
                s.tools.crop = None;
            } else if s.tools.has_open_path() {
                tools::finish_pen(s);
            } else if s.tools.is_dragging() || s.tools.has_first_edge() {
                tools::cancel(s);
            } else if s.has_selection() {
                s.selection.clear();
            } else {
                self.select_tool(Tool::Pick);
            }
        }

        let s = &mut self.state;
        if typing {
            return;
        }
        if s.has_selection() {
            // Nudge distance from the property bar; Shift for ten times that.
            let step = s.prefs.nudge;
            for (key, dx, dy) in [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)] {
                // Shift first: egui matches a plain key even with Shift held.
                for (mods, k) in [(Modifiers::SHIFT, 10.0), (Modifiers::NONE, 1.0)] {
                    if ctx.input_mut(|i| i.consume_key(mods, key)) {
                        s.transform_selection(Affine::translate(Vec2::new(dx * step * k, dy * step * k)));
                    }
                }
            }
            // Single-letter align keys.
            for (key, align) in [
                (Key::L, Some(Align::Left)),
                (Key::R, Some(Align::Right)),
                (Key::T, Some(Align::Top)),
                (Key::B, Some(Align::Bottom)),
                (Key::E, Some(Align::CenterHorizontal)),
                (Key::C, Some(Align::CenterVertical)),
                (Key::P, None),
            ] {
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                    commands.push(align.map_or(Command::CenterToPage, Command::Align));
                }
            }
        }

        for group in GROUPS {
            for &tool in group.tools {
                let Some(key) = tool.shortcut().and_then(Key::from_name) else { continue };
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key)) {
                    self.select_tool(tool);
                }
            }
        }
    }

    fn select_tool(&mut self, tool: Tool) {
        self.state.tool = tool;
        self.state.toolbox.remember(tool);
    }

    /// Housekeeping after every frame's edits.
    fn settle(&mut self, ctx: &egui::Context) {
        let s = &mut self.state;
        if s.tool != s.last_tool {
            tools::finish_pen(s);
            if s.tool != Tool::Text {
                crate::text_tool::finish(s);
            }
            s.tools.eyedropper.applying = false;
            s.tools.crop = None;
            tools::cancel(s);
            s.last_tool = s.tool;
        }
        s.doc.settle_layers();
        s.doc.sync_clones();
        s.doc.sync_page_numbers();
        let page = s.doc.active_page();
        s.selection.retain(|id| page.object(*id).is_some_and(|o| page.is_selectable(o)));
        let target = match s.selection.as_slice() {
            [id] => Some(*id),
            _ => None,
        };
        if s.nodes_owner != target {
            s.tools.nodes.clear();
            s.nodes_owner = target;
        }

        // One undo step per finished gesture or edit.
        let busy = s.tools.is_dragging() || s.tools.text.is_some() || ctx.input(|i| i.pointer.any_down()) || ctx.egui_wants_keyboard_input();
        if !busy {
            s.history.commit(&s.doc);
        }

        let name = match &s.file_path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            None => s.doc.title.clone(),
        };
        let title = format!("{}{} — Vector Studio", name, if s.history.is_dirty() { " •" } else { "" });
        if title != s.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            s.window_title = title;
        }

        if ctx.input(|i| i.viewport().close_requested()) && !s.close_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if files::confirm_discard(s) {
                s.close_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

/// After a delete: locked objects were not deleted and stay selected.
fn keep_surviving(s: &mut AppState) {
    let page = s.doc.active_page();
    s.selection.retain(|id| page.object(*id).is_some());
}

fn copy_selection(s: &mut AppState, ctx: &egui::Context) {
    s.clipboard = s.doc.copy_objects(&s.selection);
    // Put the selection on the system clipboard as SVG, so it pastes into other apps.
    let mut page = s.doc.active_page().clone();
    page.objects = s.clipboard.clone();
    s.clipboard_text = vs_io::page_to_svg(&page);
    ctx.copy_text(s.clipboard_text.clone());
}

/// Align to the page for one object, otherwise to the last selected object.
fn align_selection(s: &mut AppState, align: Align) {
    let target = match s.selection.as_slice() {
        [] => return,
        [_] => s.doc.active_page().bounds(),
        [.., last] => match s.doc.active_page().object(*last) {
            Some(o) => o.world_bounds(),
            None => return,
        },
    };
    s.doc.align_objects(&s.selection, target, align);
}

impl eframe::App for VectorStudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut commands = Vec::new();

        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            commands.extend(menu.drain());
        }
        self.handle_keys(&ctx, &mut commands);

        let native = self.has_native_menu();
        if !native {
            let state = &self.state;
            let cmd_state = |c| Self::command_state(state, c);
            chrome::consume_menu_shortcuts(&ctx, &self.layout, &cmd_state, &mut commands);
            egui::Panel::top("menu_bar").show(ui, |ui| {
                chrome::menu_bar_ui(ui, &self.layout, &cmd_state, &mut commands);
            });
        }

        let bar = |fill| egui::Frame::new().fill(fill);
        egui::Panel::top("titlebar")
            .exact_size(theme::TITLEBAR_HEIGHT)
            .resizable(false)
            .show_separator_line(false)
            .frame(bar(theme::TITLEBAR))
            .show(ui, |ui| titlebar::titlebar_ui(ui, &mut self.state, &mut commands));
        egui::Panel::top("property_bar")
            .exact_size(theme::PROPERTY_BAR_HEIGHT)
            .resizable(false)
            .show_separator_line(false)
            .frame(bar(theme::BAR).inner_margin(egui::Margin::symmetric(4, 0)))
            .show(ui, |ui| chrome::property_bar_ui(ui, &mut self.state, &mut commands));

        if self.state.prefs.show_status_bar {
            egui::Panel::bottom("status_bar")
                .show_separator_line(false)
                .frame(bar(theme::BAR).inner_margin(egui::Margin::symmetric(8, 3)))
                .show(ui, |ui| chrome::status_bar_ui(ui, &self.state));
        }

        egui::Panel::left("toolbox")
            .resizable(false)
            .exact_size(theme::TOOLBOX_WIDTH)
            .show_separator_line(false)
            .frame(bar(theme::BAR).inner_margin(egui::Margin { top: 6, ..Default::default() }))
            .show(ui, |ui| {
                if let Some(tool) = toolbox::toolbox_ui(ui, &mut self.state.toolbox, self.state.tool) {
                    self.state.tool = tool;
                }
            });

        if self.state.prefs.show_palette {
            egui::Panel::right("palette")
                .resizable(false)
                .exact_size(theme::PALETTE_WIDTH)
                .show_separator_line(false)
                .frame(bar(theme::BAR))
                .show(ui, |ui| chrome::palette_ui(ui, &mut self.state));
        }
        egui::Panel::right("docker_strip")
            .resizable(false)
            .exact_size(theme::DOCKER_STRIP_WIDTH)
            .show_separator_line(false)
            .frame(bar(theme::BAR))
            .show(ui, |ui| dockers::strip_ui(ui, &mut self.state.dockers));
        if self.state.dockers.has_content() {
            egui::Panel::right("docker")
                .resizable(true)
                .default_size(theme::DOCKER_WIDTH)
                .size_range(220.0..=520.0)
                .show_separator_line(false)
                .frame(bar(theme::DOCKER))
                .show(ui, |ui| dockers::docker_ui(ui, &mut self.state, &mut commands));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BAR))
            .show(ui, |ui| match self.canvas.as_mut() {
                Some(canvas) => crate::canvas::canvas_pane_ui(ui, &mut self.state, canvas, &mut commands),
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.label(self.canvas_error.as_deref().unwrap_or("The canvas could not start."));
                    });
                }
            });

        for cmd in commands {
            let (enabled, _) = Self::command_state(&self.state, cmd);
            if enabled {
                self.run(cmd, &ctx);
            }
        }

        files::export_window(&ctx, &mut self.state);
        files::document_properties_window(&ctx, &mut self.state);
        crate::page_dialogs::go_to_page_window(&ctx, &mut self.state);
        crate::page_dialogs::page_background_window(&ctx, &mut self.state);
        crate::edit_tools::copy_properties_window(&ctx, &mut self.state);
        self.settle(&ctx);

        #[cfg(target_os = "macos")]
        if let Some(menu) = &mut self.native_menu {
            let state = &self.state;
            menu.sync(|c| Self::command_state(state, c));
        }

        if self.state.show_about {
            egui::Window::new("About Vector Studio")
                .collapsible(false)
                .resizable(false)
                .open(&mut self.state.show_about)
                .show(&ctx, |ui| {
                    ui.label(format!("Vector Studio {}", env!("CARGO_PKG_VERSION")));
                    ui.label("Phase 1: drawing, editing, save/open and SVG/PNG/PDF export.");
                });
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, KEY_DOCKERS, &self.state.dockers);
        eframe::set_value(storage, KEY_PREFS, &self.state.prefs);
        eframe::set_value(storage, KEY_TOOLBOX, &self.state.toolbox);
    }
}
