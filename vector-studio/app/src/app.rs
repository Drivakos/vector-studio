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
    pub snap_pixels: bool,
    pub alignment_guides: bool,
    /// Text > Show Nonprinting Characters.
    pub show_nonprinting: bool,
    pub dynamic_guides: bool,
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
            snap_pixels: false,
            alignment_guides: false,
            show_nonprinting: false,
            dynamic_guides: false,
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
            SnapTarget::Pixels => &self.snap_pixels,
        }
    }

    pub fn snap_flag_mut(&mut self, target: SnapTarget) -> &mut bool {
        match target {
            SnapTarget::Grid => &mut self.snap_grid,
            SnapTarget::Guidelines => &mut self.snap_guidelines,
            SnapTarget::Objects => &mut self.snap_objects,
            SnapTarget::Page => &mut self.snap_page,
            SnapTarget::Pixels => &mut self.snap_pixels,
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
    /// Selected guidelines: the page they are on and their indices in its guide list.
    pub guide_selection: Option<(vs_core::PageId, Vec<usize>)>,
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
    /// The system's printers; tests swap in a scripted one.
    pub printing: Box<dyn crate::printing::Printing>,
    pub print: crate::printing::PrintDialog,
    pub find: crate::find_ui::FindState,
    /// The system clipboard, for Paste Special; tests swap in a scripted one.
    pub clipboard_source: Box<dyn crate::paste_special::ClipboardText>,
    pub paste_special: Option<crate::paste_special::PasteSpecialDialog>,
    /// The text object and its words in the open Edit Text window.
    pub edit_text: Option<(ObjectId, String)>,
    pub renaming_page: Option<(usize, String)>,
    /// The page number in the open Go to Page window (1-based).
    pub go_to_page: Option<usize>,
    pub page_background: Option<crate::page_dialogs::PageBackgroundDialog>,
    /// Where templates are kept; tests point it at a temporary folder.
    pub templates_dir: Option<PathBuf>,
    pub export_for: Option<crate::export_for::ExportFor>,
    pub document_options: Option<crate::page_dialogs::DocumentOptionsDialog>,
    /// The case chosen in the open Change Case window.
    pub change_case: Option<vs_core::text_tools::Case>,
    pub show_text_statistics: bool,
    pub columns: Option<crate::text_menu::ColumnsDialog>,
    /// The list style chosen in the open Bullets and Numbering window.
    pub bullets: Option<Option<vs_core::ListStyle>>,
    pub drop_cap: Option<crate::text_menu::DropCapDialog>,
    pub tabs: Option<crate::text_menu::TabsDialog>,
    pub convert_to_bitmap: Option<crate::bitmap_menu::ConvertDialog>,
    pub resample: Option<crate::bitmap_menu::ResampleDialog>,
    /// The bitmap effect whose settings window is open.
    pub effect: Option<crate::bitmap_menu::EffectDialog>,
    pub insert_qr: Option<crate::insert_codes::QrDialog>,
    pub print_merge: Option<crate::print_merge::PrintMergeDialog>,
    /// Picked table cells, a cell being typed into, and the Table menu's windows.
    pub table: crate::table_tool::TableState,
    pub glyphs: Option<crate::glyphs::GlyphsDialog>,
    pub insert_barcode: Option<crate::insert_codes::BarcodeDialog>,
    pub save_template: Option<crate::templates::SaveTemplateDialog>,
    pub new_from_template: Option<crate::templates::NewFromTemplateDialog>,
    pub show_document_properties: bool,
    pub renaming_object: Option<(ObjectId, String)>,
    /// The layer (page index, layer) being renamed in the Objects docker.
    pub renaming_layer: Option<(usize, vs_core::LayerId, String)>,
    pub interaction: CanvasInteraction,
    pub export: ExportDialog,
    pub step_repeat: crate::edit_tools::StepRepeat,
    pub copy_properties: crate::edit_tools::CopyProperties,
    pub powerclip: crate::edit_tools::PowerClipState,
    /// Copy Effect > Perspective From is waiting for the click on the source.
    pub copy_perspective: bool,
    /// Edit Symmetry or Edit Symbol in progress.
    pub group_edit: Option<crate::symbol_menu::GroupEdit>,
    /// The Symbol Manager window is open.
    pub symbol_manager: bool,
    /// How the canvas draws objects (not saved).
    pub view_mode: vs_render::ViewMode,
    /// View > Simulate Overprints (not saved).
    pub simulate_overprints: bool,
    /// Full-Screen Preview is on.
    pub preview: Option<Preview>,
    pub page_sorter: crate::page_sorter::PageSorter,
    /// What Edit > Repeat does again.
    pub last_action: Option<Repeatable>,
    /// Last status message (e.g. "Exported …").
    pub status: Option<String>,
    clipboard: Vec<Object>,
    /// The symbols the copied objects are instances of, for pasting into another drawing.
    clipboard_symbols: Vec<vs_core::Symbol>,
    /// Text we put on the system clipboard with the last copy, to recognise our own paste.
    clipboard_text: String,
    pub(crate) untitled_count: u32,
    show_about: bool,
    last_tool: Tool,
    nodes_owner: Option<ObjectId>,
    window_title: String,
    close_confirmed: bool,
}

/// Full-Screen Preview's placement of the page.
#[derive(Clone, Copy, Debug)]
pub struct Preview {
    pub viewport: Viewport,
    /// The window area the page is shown in.
    pub rect: egui::Rect,
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
            guide_selection: None,
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
            printing: Box::new(crate::printing::SystemPrinting),
            print: Default::default(),
            find: Default::default(),
            edit_text: None,
            clipboard_source: Box::new(crate::paste_special::SystemClipboard),
            paste_special: None,
            renaming_page: None,
            go_to_page: None,
            page_background: None,
            templates_dir: crate::templates::default_dir(),
            export_for: None,
            document_options: None,
            change_case: None,
            show_text_statistics: false,
            columns: None,
            bullets: None,
            drop_cap: None,
            tabs: None,
            convert_to_bitmap: None,
            resample: None,
            effect: None,
            insert_qr: None,
            print_merge: None,
            table: Default::default(),
            glyphs: None,
            insert_barcode: None,
            save_template: None,
            new_from_template: None,
            show_document_properties: false,
            renaming_object: None,
            renaming_layer: None,
            interaction: CanvasInteraction::default(),
            export: ExportDialog::default(),
            step_repeat: Default::default(),
            copy_properties: Default::default(),
            powerclip: Default::default(),
            copy_perspective: false,
            group_edit: None,
            symbol_manager: false,
            view_mode: Default::default(),
            simulate_overprints: false,
            preview: None,
            page_sorter: Default::default(),
            last_action: None,
            status: None,
            clipboard: Vec::new(),
            clipboard_symbols: Vec::new(),
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
            // The pixels of Pixels view: the Export window's resolution.
            pixels: p.snap_pixels.then(|| 72.0 / self.export.dpi),
            guides: p.snap_guidelines && p.show_guidelines,
            objects: p.snap_objects,
            page: p.snap_page,
        }
    }

    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// Indices of the selected guidelines of the active page.
    pub fn selected_guides(&self) -> Vec<usize> {
        let page = self.doc.active_page();
        match &self.guide_selection {
            Some((id, indices)) if *id == page.id => indices.iter().copied().filter(|i| *i < page.guides.len()).collect(),
            _ => Vec::new(),
        }
    }

    /// The SVG text the last copy put on the system clipboard.
    pub fn system_clipboard_text(&self) -> &str {
        &self.clipboard_text
    }

    /// The objects of the last copy or cut.
    pub fn internal_clipboard(&self) -> &[Object] {
        &self.clipboard
    }

    /// The symbols [`Self::internal_clipboard`]'s instances belong to.
    pub fn internal_clipboard_symbols(&self) -> &[vs_core::Symbol] {
        &self.clipboard_symbols
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
            Command::PageSorter => (true, Some(state.page_sorter.open)),
            Command::ToggleGuidelines => (true, Some(state.prefs.show_guidelines)),
            Command::ToggleAlignmentGuides => (true, Some(state.prefs.alignment_guides)),
            Command::ToggleNonprinting => (true, Some(state.prefs.show_nonprinting)),
            Command::SimulateOverprints => (true, Some(state.simulate_overprints)),
            Command::Overprint(part) => (state.selected_objects().any(|o| !o.locked), Some(doc.overprints(&state.selection, part))),
            Command::ToggleDynamicGuides => (true, Some(state.prefs.dynamic_guides)),
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
            Command::SelectAllText => {
                let page = doc.active_page();
                (page.objects.iter().any(|o| page.is_selectable(o) && matches!(o.shape, vs_core::Shape::Text(_))), None)
            }
            Command::SelectAllGuidelines => (state.prefs.show_guidelines && !doc.active_page().guides.is_empty(), None),
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
            | Command::Duplicate
            | Command::Align(_)
            | Command::CenterToPage
            | Command::Order(_) => (selected, None),
            Command::Delete => (selected || !state.selected_guides().is_empty(), None),
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
            Command::EditText => (crate::text_tool::can_edit_text(state), None),
            Command::ChangeCase => (!crate::text_menu::case_targets(state).is_empty(), None),
            Command::Columns => (!crate::text_menu::column_targets(state).is_empty(), None),
            Command::Bullets => (!crate::text_menu::list_targets(state).is_empty(), None),
            Command::DropCap => (!crate::text_menu::column_targets(state).is_empty(), None),
            Command::Tabs => (!crate::text_menu::list_targets(state).is_empty(), None),
            Command::CropBitmap => (!crate::bitmap_menu::bitmap_targets(state).is_empty(), None),
            Command::ConvertToBitmap => (crate::bitmap_menu::can_convert(state), None),
            Command::Resample => (crate::bitmap_menu::resample_target(state).is_some(), None),
            Command::BitmapEffect(_) => (!crate::bitmap_menu::bitmap_targets(state).is_empty(), None),
            Command::StraightenText => (doc.can_straighten_text(&state.selection), None),
            Command::ClipPlaceInside => (doc.can_place_inside(&state.selection), None),
            Command::ClipExtract => (doc.can_extract_contents(&state.selection), None),
            Command::ClipEdit => (crate::edit_tools::can_edit_contents(state), None),
            Command::ClipEmptyFrame => (doc.can_create_empty_frame(&state.selection), None),
            Command::ClipFinishEdit => (state.powerclip.editing.is_some(), None),
            Command::SymmetryNew => (crate::symbol_menu::can_create_symmetry(state), None),
            Command::SymmetryEdit => (crate::symbol_menu::can_edit_symmetry(state), None),
            Command::SymmetryRemove => (!doc.symmetry_groups(&state.selection).is_empty(), None),
            Command::SymbolNew => (crate::symbol_menu::can_make_symbol(state), None),
            Command::SymbolEdit => (crate::symbol_menu::can_edit_symbol(state), None),
            Command::SymbolRevert => (!doc.instances(&state.selection).is_empty(), None),
            Command::SymbolManager => (true, Some(state.symbol_manager)),
            Command::GroupFinishEdit => (state.group_edit.is_some(), None),
            Command::AddPerspective => (doc.can_add_perspective(&state.selection), None),
            Command::CopyPerspective => (crate::edit_tools::can_copy_perspective(state), None),
            Command::ClearEffect => (!doc.with_perspective(&state.selection).is_empty(), None),
            Command::TextToTable => (crate::table_tool::can_text_to_table(state), None),
            Command::TableToText => (crate::table_tool::selected_table(state).is_some(), None),
            Command::TableRowAbove | Command::TableRowBelow | Command::TableColumnLeft | Command::TableColumnRight => (crate::table_tool::can_insert(state), None),
            Command::TableMergeCells => (crate::table_tool::can_merge(state), None),
            Command::TableSplitRows | Command::TableSplitColumns => (crate::table_tool::can_split(state), None),
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
            Command::Print => {
                crate::printing::open(s);
                Ok(())
            }
            Command::PrintMerge => {
                crate::print_merge::open(s);
                Ok(())
            }
            Command::TableCreate => {
                crate::table_tool::open_create(s);
                Ok(())
            }
            Command::TextToTable => {
                crate::table_tool::open_convert(s, true);
                Ok(())
            }
            Command::TableToText => {
                crate::table_tool::open_convert(s, false);
                Ok(())
            }
            Command::TableRowAbove | Command::TableRowBelow => {
                crate::table_tool::insert(s, false, cmd == Command::TableRowBelow);
                Ok(())
            }
            Command::TableColumnLeft | Command::TableColumnRight => {
                crate::table_tool::insert(s, true, cmd == Command::TableColumnRight);
                Ok(())
            }
            Command::TableMergeCells => {
                crate::table_tool::merge(s);
                Ok(())
            }
            Command::TableSplitRows | Command::TableSplitColumns => {
                crate::table_tool::open_split(s, cmd == Command::TableSplitColumns);
                Ok(())
            }
            Command::DocumentProperties => {
                let _: () = s.show_document_properties = true;
                Ok(())
            }
            Command::ChangeCase => {
                crate::text_menu::open_change_case(s);
                Ok(())
            }
            Command::Columns => {
                crate::text_tool::finish(s);
                crate::text_menu::open_columns(s);
                Ok(())
            }
            Command::DropCap => {
                crate::text_tool::finish(s);
                crate::text_menu::open_drop_cap(s);
                Ok(())
            }
            Command::Bullets => {
                crate::text_menu::open_bullets(s);
                Ok(())
            }
            Command::ConvertToBitmap => {
                crate::text_tool::finish(s);
                crate::bitmap_menu::open_convert(s);
                Ok(())
            }
            Command::Resample => {
                crate::bitmap_menu::open_resample(s);
                Ok(())
            }
            Command::CropBitmap => {
                crate::bitmap_menu::crop_bitmaps(s);
                Ok(())
            }
            Command::Tabs => {
                crate::text_menu::open_tabs(s);
                Ok(())
            }
            Command::InsertQrCode => {
                crate::text_tool::finish(s);
                crate::insert_codes::open_qr(s);
                Ok(())
            }
            Command::InsertBarcode => {
                crate::text_tool::finish(s);
                crate::insert_codes::open_barcode(s);
                Ok(())
            }
            Command::BitmapEffect(kind) => {
                crate::bitmap_menu::run_effect(s, kind);
                Ok(())
            }
            Command::TextProperties => {
                s.dockers.show(DockerKind::Properties);
                Ok(())
            }
            Command::TextStatistics => {
                let _: () = s.show_text_statistics = true;
                Ok(())
            }
            Command::DocumentOptions => {
                crate::page_dialogs::open_document_options(s);
                Ok(())
            }
            Command::ExportForWeb => {
                s.export_for = Some(crate::export_for::ExportFor::web());
                Ok(())
            }
            Command::ExportPrintPdf => {
                s.export_for = Some(crate::export_for::ExportFor::print_pdf());
                Ok(())
            }
            Command::SaveAsTemplate => {
                crate::templates::open_save(s);
                Ok(())
            }
            Command::NewFromTemplate => {
                crate::templates::open_new(s);
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
                s.doc.adopt_symbols(&s.selection, &s.clipboard_symbols);
                Ok(())
            }
            Command::PasteSpecial => {
                crate::paste_special::open(s);
                Ok(())
            }
            Command::Delete => {
                let guides = s.selected_guides();
                if !guides.is_empty() {
                    let page = s.doc.active_page_mut();
                    for i in guides.into_iter().rev() {
                        page.guides.remove(i);
                    }
                    s.guide_selection = None;
                } else if !tools::delete_selected_nodes(s) {
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
            Command::FindReplace => {
                s.dockers.show(DockerKind::FindReplace);
                Ok(())
            }
            Command::StepRepeat => {
                s.dockers.show(DockerKind::StepRepeat);
                Ok(())
            }
            Command::SelectAll => {
                s.selection = s.doc.all_object_ids();
                s.marquee_selection = Some(s.selection.clone());
                s.guide_selection = None;
                Ok(())
            }
            Command::SelectAllText => {
                let page = s.doc.active_page();
                s.selection = page.objects.iter().filter(|o| page.is_selectable(o) && matches!(o.shape, vs_core::Shape::Text(_))).map(|o| o.id).collect();
                s.marquee_selection = Some(s.selection.clone());
                s.guide_selection = None;
                Ok(())
            }
            Command::SelectAllGuidelines => {
                let page = s.doc.active_page();
                s.guide_selection = Some((page.id, (0..page.guides.len()).collect()));
                s.selection.clear();
                s.marquee_selection = None;
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
            Command::EditText => {
                crate::text_tool::open_edit_text(s);
                Ok(())
            }
            Command::Glyphs => {
                crate::glyphs::open(s);
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
                s.copy_perspective = false;
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
            Command::ClipEmptyFrame => {
                s.doc.create_empty_frame(&s.selection);
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
            Command::SymmetryNew => {
                crate::symbol_menu::create_symmetry(s);
                Ok(())
            }
            Command::SymmetryEdit => {
                crate::symbol_menu::edit_symmetry(s);
                Ok(())
            }
            Command::SymmetryRemove => {
                s.doc.remove_symmetry(&s.selection);
                Ok(())
            }
            Command::SymbolNew => {
                if crate::symbol_menu::can_make_symbol(s)
                    && let Some(id) = s.doc.new_symbol(&s.selection)
                {
                    s.selection = vec![id];
                    s.marquee_selection = None;
                }
                Ok(())
            }
            Command::SymbolEdit => {
                crate::symbol_menu::edit_symbol(s);
                Ok(())
            }
            Command::SymbolRevert => {
                s.doc.revert_symbols(&s.selection);
                Ok(())
            }
            Command::SymbolManager => {
                s.symbol_manager = !s.symbol_manager;
                Ok(())
            }
            Command::GroupFinishEdit => {
                crate::symbol_menu::finish(s);
                Ok(())
            }
            Command::AddPerspective => {
                // Its corners are dragged with the Shape tool.
                if let Some(id) = s.doc.add_perspective(&s.selection) {
                    s.selection = vec![id];
                    s.marquee_selection = None;
                    s.tool = Tool::Shape;
                    s.toolbox.remember(Tool::Shape);
                    s.tools.nodes.clear();
                }
                Ok(())
            }
            Command::CopyPerspective => {
                s.copy_properties.picking = false;
                s.powerclip.picking = false;
                s.copy_perspective = crate::edit_tools::can_copy_perspective(s);
                Ok(())
            }
            Command::ClearEffect => {
                s.doc.clear_perspective(&s.selection);
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
            Command::SimulateOverprints => {
                let _: () = s.simulate_overprints ^= true;
                Ok(())
            }
            Command::Overprint(part) => {
                let on = !s.doc.overprints(&s.selection, part);
                s.doc.set_overprint(&s.selection, part, on);
                Ok(())
            }
            Command::PageSorter => {
                crate::text_tool::finish(s);
                tools::cancel(s);
                s.page_sorter.open ^= true;
                s.needs_fit |= !s.page_sorter.open;
                Ok(())
            }
            Command::FullScreenPreview => {
                crate::text_tool::finish(s);
                tools::cancel(s);
                s.preview = Some(Preview { viewport: s.viewport, rect: egui::Rect::NOTHING });
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                Ok(())
            }
            Command::ToggleGuidelines => {
                let _: () = s.prefs.show_guidelines ^= true;
                Ok(())
            }
            Command::ToggleNonprinting => {
                let _: () = s.prefs.show_nonprinting ^= true;
                Ok(())
            }
            Command::ToggleAlignmentGuides => {
                let _: () = s.prefs.alignment_guides ^= true;
                Ok(())
            }
            Command::ToggleDynamicGuides => {
                let _: () = s.prefs.dynamic_guides ^= true;
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
        if self.state.preview.is_some() {
            // Any key or click ends Full-Screen Preview, and does nothing else.
            let leave = ctx.input_mut(|i| {
                let ends = |e: &egui::Event| matches!(e, egui::Event::Key { pressed: true, .. } | egui::Event::PointerButton { pressed: true, .. });
                let any = i.events.iter().any(ends);
                i.events.retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::PointerButton { .. }));
                any
            });
            if leave {
                self.state.preview = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            }
            return;
        }
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
            if s.page_sorter.open {
                s.page_sorter.open = false;
                s.needs_fit = true;
            } else if s.copy_properties.picking {
                s.copy_properties.picking = false;
            } else if s.copy_perspective {
                s.copy_perspective = false;
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
        s.doc.sync_symbols();
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
    s.clipboard_symbols = s.doc.symbols_used_by(&s.clipboard);
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

        if self.state.preview.is_some() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(egui::Color32::BLACK))
                .show(ui, |ui| {
                    if let Some(canvas) = self.canvas.as_mut() {
                        crate::canvas::preview_ui(ui, &mut self.state, canvas);
                    }
                });
            self.settle(&ctx);
            return;
        }

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
                _ if self.state.page_sorter.open => crate::page_sorter::page_sorter_ui(ui, &mut self.state),
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
        crate::printing::print_window(&ctx, &mut self.state);
        crate::print_merge::print_merge_window(&ctx, &mut self.state);
        crate::table_tool::table_window(&ctx, &mut self.state);
        crate::text_tool::edit_text_window(&ctx, &mut self.state);
        crate::paste_special::paste_special_window(&ctx, &mut self.state);
        crate::page_dialogs::go_to_page_window(&ctx, &mut self.state);
        crate::page_dialogs::page_background_window(&ctx, &mut self.state);
        crate::templates::save_window(&ctx, &mut self.state);
        crate::export_for::export_for_window(&ctx, &mut self.state);
        crate::page_dialogs::document_options_window(&ctx, &mut self.state);
        crate::text_menu::change_case_window(&ctx, &mut self.state);
        crate::text_menu::statistics_window(&ctx, &mut self.state);
        crate::text_menu::columns_window(&ctx, &mut self.state);
        crate::text_menu::bullets_window(&ctx, &mut self.state);
        crate::text_menu::drop_cap_window(&ctx, &mut self.state);
        crate::text_menu::tabs_window(&ctx, &mut self.state);
        crate::symbol_menu::symbol_manager_window(&ctx, &mut self.state);
        crate::bitmap_menu::convert_window(&ctx, &mut self.state);
        crate::bitmap_menu::resample_window(&ctx, &mut self.state);
        crate::bitmap_menu::effect_window(&ctx, &mut self.state);
        crate::insert_codes::qr_window(&ctx, &mut self.state);
        crate::glyphs::window(&ctx, &mut self.state);
        crate::insert_codes::barcode_window(&ctx, &mut self.state);
        crate::templates::new_window(&ctx, &mut self.state);
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
