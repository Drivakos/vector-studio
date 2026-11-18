//! Drives the whole app the way a person does: real window chrome, real GPU canvas,
//! pointer and keyboard events, widgets found by their accessible names. Native file and
//! message dialogs are replaced by a script.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;

use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use vector_studio::app::AppState;
use vector_studio::files::{Dialogs, Unsaved};
use vector_studio::paste_special::ClipboardText;
use vector_studio::printing::{PrintJob, Printing};
use vector_studio::{Startup, VectorStudioApp};
use vs_core::kurbo::{Point, Rect};
use vs_core::{Document, Object, ObjectId};

/// Answers the dialogs will give, and what they were asked.
#[derive(Default)]
pub struct Script {
    pub save_paths: VecDeque<PathBuf>,
    pub open_paths: VecDeque<PathBuf>,
    pub unsaved: VecDeque<Unsaved>,
    /// Answers to OK/Cancel questions.
    pub confirms: VecDeque<bool>,
    /// Printers the scripted print system offers, and its default.
    pub printers: Vec<String>,
    pub default_printer: Option<String>,
    /// Jobs sent to the printer, with the PDF each one printed.
    pub printed: Vec<(PrintJob, Vec<u8>)>,
    /// What the scripted system clipboard holds, for Paste Special.
    pub clipboard: Option<String>,
    /// Titles of every dialog shown, in order.
    pub shown: Vec<String>,
    pub errors: Vec<String>,
}

struct ScriptedDialogs(Rc<RefCell<Script>>);

impl Dialogs for ScriptedDialogs {
    fn save_file(&mut self, title: &str, _filter: &str, _extension: &str, _file_name: &str) -> Option<PathBuf> {
        let mut s = self.0.borrow_mut();
        s.shown.push(title.to_owned());
        s.save_paths.pop_front()
    }

    fn open_file(&mut self, title: &str, _filter: &str, _extensions: &[&str]) -> Option<PathBuf> {
        let mut s = self.0.borrow_mut();
        s.shown.push(title.to_owned());
        s.open_paths.pop_front()
    }

    fn unsaved_changes(&mut self, _document: &str) -> Unsaved {
        let mut s = self.0.borrow_mut();
        s.shown.push("Unsaved changes".to_owned());
        s.unsaved.pop_front().expect("the test did not script an answer to the unsaved-changes dialog")
    }

    fn confirm(&mut self, title: &str, _detail: &str) -> bool {
        let mut s = self.0.borrow_mut();
        s.shown.push(title.to_owned());
        s.confirms.pop_front().expect("the test did not script an answer to a confirmation")
    }

    fn error(&mut self, title: &str, detail: &str) {
        let mut s = self.0.borrow_mut();
        s.shown.push(title.to_owned());
        s.errors.push(format!("{title}: {detail}"));
    }
}

struct ScriptedPrinting(Rc<RefCell<Script>>);

impl Printing for ScriptedPrinting {
    fn printers(&mut self) -> (Vec<String>, Option<String>) {
        let s = self.0.borrow();
        (s.printers.clone(), s.default_printer.clone())
    }

    fn print(&mut self, pdf: &[u8], job: &PrintJob) -> Result<(), String> {
        self.0.borrow_mut().printed.push((job.clone(), pdf.to_vec()));
        Ok(())
    }
}

struct ScriptedClipboard(Rc<RefCell<Script>>);

impl ClipboardText for ScriptedClipboard {
    fn text(&mut self) -> Option<String> {
        self.0.borrow().clipboard.clone()
    }
}

pub struct E2e {
    pub h: Harness<'static, VectorStudioApp>,
    pub script: Rc<RefCell<Script>>,
    mods: Modifiers,
}

impl E2e {
    pub fn new() -> Self {
        Self::with_startup(Startup::default())
    }

    pub fn with_startup(startup: Startup) -> Self {
        let script = Rc::new(RefCell::new(Script::default()));
        let mut h = Harness::builder()
            .with_size(egui::vec2(1440.0, 900.0))
            // Frames as on a 60 Hz display, so click timing (double-clicks) is realistic.
            .with_step_dt(1.0 / 60.0)
            .with_max_steps(60)
            .wgpu()
            .build_eframe(move |cc| VectorStudioApp::new(cc, startup));
        h.state_mut().state_mut().dialogs = Box::new(ScriptedDialogs(script.clone()));
        h.state_mut().state_mut().printing = Box::new(ScriptedPrinting(script.clone()));
        h.state_mut().state_mut().clipboard_source = Box::new(ScriptedClipboard(script.clone()));
        // Never the real Templates folder: one of its own for each app under test.
        static APPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = APPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let templates = std::env::temp_dir().join(format!("vs-e2e-templates-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&templates);
        h.state_mut().state_mut().templates_dir = Some(templates);
        let mut app = Self { h, script, mods: Modifiers::NONE };
        app.run();
        app
    }

    pub fn run(&mut self) {
        self.h.run();
    }

    pub fn state(&self) -> &AppState {
        self.h.state().state()
    }

    pub fn doc(&self) -> &Document {
        &self.state().doc
    }

    pub fn objects(&self) -> &[Object] {
        &self.doc().active_page().objects
    }

    pub fn ids(&self) -> Vec<ObjectId> {
        self.doc().all_object_ids()
    }

    pub fn bounds(&self, i: usize) -> Rect {
        self.objects()[i].world_bounds()
    }

    pub fn selection(&self) -> Vec<ObjectId> {
        self.state().selection.clone()
    }

    // ---- Widgets, found by accessible name -------------------------------------------

    /// Click the widget named exactly `name`.
    pub fn click(&mut self, name: &str) {
        self.h.get_by_label(name).click();
        self.run();
    }

    /// Double-click the widget named exactly `name`.
    pub fn double_click(&mut self, name: &str) {
        self.wait(1.0); // so an earlier click does not pair with the first one
        self.h.get_by_label(name).click();
        self.h.step();
        self.h.get_by_label(name).click();
        self.run();
    }

    /// Type into the focused field named `name` and press Enter.
    pub fn type_into(&mut self, name: &str, text: &str) {
        self.h.get_by_label(name).type_text(text);
        self.key(Key::Enter);
    }

    /// Right-click the widget named exactly `name`.
    pub fn right_click(&mut self, name: &str) {
        self.h.get_by_label(name).click_secondary();
        self.run();
    }

    /// Click the widget whose name contains `part` (menu items, flyout entries with icons).
    pub fn click_containing(&mut self, part: &str) {
        self.h.get_by_label_contains(part).click();
        self.run();
    }

    pub fn exists(&self, name: &str) -> bool {
        self.h.query_by_label(name).is_some()
    }

    /// Whether the button or toggle named `name` can be used.
    pub fn is_enabled(&self, name: &str) -> bool {
        !self.h.get_by_label(name).accesskit_node().is_disabled()
    }

    /// Whether the in-window menu entry at `path` can be used, e.g. `["Object", "Overprint Fill"]`.
    /// Opens the menus on the way and closes them again.
    pub fn menu_enabled(&mut self, path: &[&str]) -> bool {
        let (item, menus) = path.split_last().expect("a menu path");
        if !menus.is_empty() {
            self.menu(menus);
        }
        let prefix = format!("{item} ");
        let enabled = self
            .h
            .query_all_by(|n| n.label().is_some_and(|l| l == *item || l.starts_with(&prefix)))
            .map(|n| !n.accesskit_node().is_disabled())
            .collect::<Vec<_>>();
        assert!(!enabled.is_empty(), "no menu entry {item:?}");
        // A second click on the menu bar entry closes the menu.
        if let Some(top) = menus.first() {
            self.click_menu_bar(top);
        }
        enabled.into_iter().all(|e| e)
    }

    /// Whether the toggle named `name` shows as on.
    pub fn is_selected(&self, name: &str) -> bool {
        let node = self.h.get_by_label(name).accesskit_node();
        node.toggled() == Some(egui::accesskit::Toggled::True) || node.is_selected() == Some(true)
    }

    /// Type `text` into the field named `name`, replacing what is there, and press Enter.
    pub fn fill(&mut self, name: &str, text: &str) {
        self.h.get_by_label(name).click();
        self.run();
        self.h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        self.h.get_by_label(name).type_text(text);
        self.h.key_press(Key::Enter);
        self.run();
    }

    /// Where the widget named `name` is in the window.
    pub fn rect_of(&self, name: &str) -> egui::Rect {
        self.h.get_by_label(name).rect()
    }

    /// The value shown by the field named `name`.
    pub fn value(&self, name: &str) -> String {
        self.h.get_by_label(name).value().unwrap_or_default()
    }

    /// Open a menu of the in-window menu bar and pick an item, e.g. `["Object", "Group", "Ungroup All"]`.
    /// After the first step, only widgets that appeared with the previous step count, so a
    /// docker section or toolbar button with the same name is never taken for the menu
    /// entry. Menu items with a shortcut are named "Label Shortcut".
    pub fn menu(&mut self, path: &[&str]) {
        let mut before: Option<std::collections::HashSet<String>> = None;
        for (i, part) in path.iter().enumerate() {
            let labelled: Vec<_> = self.h.query_all_by(|n| n.label().is_some()).collect();
            let key = |n: &egui_kittest::Node<'_>| format!("{:?}", n.accesskit_node().id());
            let is_new = |n: &egui_kittest::Node<'_>| before.as_ref().is_none_or(|b| !b.contains(&key(n)));
            let label = |n: &egui_kittest::Node<'_>| n.accesskit_node().label().unwrap_or_default();
            let prefix = format!("{part} ");
            let node = labelled
                .iter()
                .find(|n| is_new(n) && label(n) == *part)
                .or_else(|| labelled.iter().find(|n| is_new(n) && label(n).starts_with(&prefix)))
                .copied()
                .unwrap_or_else(|| panic!("no menu entry {part:?}"));
            before = Some(labelled.iter().map(key).collect());
            // Submenus open on hover; a click would toggle them shut again.
            if i > 0 && i + 1 < path.len() { node.hover() } else { node.click() }
            self.run();
        }
    }

    fn click_menu_bar(&mut self, name: &str) {
        self.h.query_all_by_label(name).next().unwrap_or_else(|| panic!("no menu {name:?}")).click();
        self.run();
    }

    /// Show a docker with its button in the docker strip.
    pub fn docker(&mut self, title: &str) {
        let name = format!("{title} docker");
        if !self.is_selected(&name) {
            self.click(&name);
        }
    }

    /// Screen distance of the selection handles from the selection bounds, in points.
    pub fn handle_gap(&self) -> f64 {
        5.0 / self.state().viewport.zoom()
    }

    /// Pick a tool from its toolbox flyout (right-click opens the flyout). The toolbox
    /// button is the toggle named `shown`, so a menu of the same name is never taken.
    pub fn flyout_tool(&mut self, shown: &str, tool: &str) {
        let button = self.h.query_all_by_label(shown).find(|n| n.accesskit_node().toggled().is_some());
        button.unwrap_or_else(|| panic!("no tool button {shown:?}")).click_secondary();
        self.run();
        self.click_containing(tool);
    }

    /// Let `seconds` of frames pass.
    pub fn wait(&mut self, seconds: f32) {
        self.h.run_steps((seconds * 60.0) as usize);
    }

    // ---- Keyboard ----------------------------------------------------------------

    pub fn key(&mut self, key: Key) {
        self.h.key_press(key);
        self.run();
    }

    pub fn shortcut(&mut self, mods: Modifiers, key: Key) {
        self.h.key_press_modifiers(mods, key);
        self.run();
    }

    /// Type `text` as keyboard input to whatever has focus (the canvas text being edited).
    pub fn type_text(&mut self, text: &str) {
        self.h.event(Event::Text(text.to_owned()));
        self.run();
    }

    /// Hold `mods` for the pointer gestures that follow, until [`E2e::release_mods`].
    pub fn hold(&mut self, mods: Modifiers) {
        self.mods = mods;
        self.h.event(Event::ModifiersChanged(mods));
    }

    pub fn release_mods(&mut self) {
        self.hold(Modifiers::NONE);
    }

    // ---- Canvas, in document coordinates (points) --------------------------------

    /// Window position of document point (`x`, `y`).
    pub fn at(&self, x: f64, y: f64) -> Pos2 {
        let s = self.state();
        let p = s.viewport.doc_to_screen(Point::new(x, y));
        s.canvas_rect.min + egui::vec2(p.x as f32, p.y as f32)
    }

    fn pointer(&mut self, pos: Pos2) {
        self.h.event(Event::PointerMoved(pos));
        self.h.step();
    }

    fn button(&mut self, pos: Pos2, button: PointerButton, pressed: bool) {
        self.h.event(Event::PointerMoved(pos));
        self.h.event(Event::PointerButton { pos, button, pressed, modifiers: self.mods });
        self.h.step();
    }

    /// Move the pointer to document point (`x`, `y`) without pressing.
    pub fn hover_doc(&mut self, x: f64, y: f64) {
        let p = self.at(x, y);
        self.pointer(p);
        self.h.step();
    }

    pub fn click_doc(&mut self, x: f64, y: f64) {
        let p = self.at(x, y);
        self.pointer(p);
        self.button(p, PointerButton::Primary, true);
        self.button(p, PointerButton::Primary, false);
        self.settle();
    }

    pub fn double_click_doc(&mut self, x: f64, y: f64) {
        let p = self.at(x, y);
        self.pointer(p);
        for _ in 0..2 {
            self.button(p, PointerButton::Primary, true);
            self.button(p, PointerButton::Primary, false);
        }
        self.settle();
    }

    /// Press at `from`, move in steps, release at `to`.
    pub fn drag_doc(&mut self, from: (f64, f64), to: (f64, f64)) {
        let (a, b) = (self.at(from.0, from.1), self.at(to.0, to.1));
        self.drag_screen(a, b);
    }

    pub fn drag_screen(&mut self, a: Pos2, b: Pos2) {
        self.pointer(a);
        self.button(a, PointerButton::Primary, true);
        for i in 1..=8 {
            self.pointer(a.lerp(b, i as f32 / 8.0));
        }
        self.button(b, PointerButton::Primary, false);
        self.settle();
    }

    /// Move the pointer through `points` with the button down (freehand strokes).
    pub fn stroke_doc(&mut self, points: &[(f64, f64)]) {
        let screen: Vec<Pos2> = points.iter().map(|(x, y)| self.at(*x, *y)).collect();
        self.pointer(screen[0]);
        self.button(screen[0], PointerButton::Primary, true);
        for p in &screen[1..] {
            self.pointer(*p);
        }
        self.button(*screen.last().unwrap(), PointerButton::Primary, false);
        self.settle();
    }

    /// Press at document point (`x`, `y`), keep the button down for `seconds` of frames
    /// without moving, then release.
    pub fn hold_doc(&mut self, x: f64, y: f64, seconds: f32) {
        let p = self.at(x, y);
        self.pointer(p);
        self.button(p, PointerButton::Primary, true);
        for _ in 0..(seconds * 60.0) as usize {
            self.h.step();
        }
        self.button(p, PointerButton::Primary, false);
        self.settle();
    }

    /// Let double-click timing pass and the app settle.
    fn settle(&mut self) {
        // Moving away and waiting keeps the next click from counting as a double click.
        for _ in 0..30 {
            self.h.step();
        }
        self.run();
    }

    // ---- Rendering ------------------------------------------------------------------

    /// The colour the canvas shows at document point (`x`, `y`), as rendered on the GPU.
    pub fn pixel(&mut self, x: f64, y: f64) -> [u8; 4] {
        let p = self.at(x, y);
        let ppp = self.h.ctx.pixels_per_point();
        let img = self.h.render().expect("render");
        img.get_pixel((p.x * ppp) as u32, (p.y * ppp) as u32).0
    }

    /// Every rendered pixel inside the document rectangle (`x0`, `y0`)–(`x1`, `y1`).
    pub fn pixels_in(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[u8; 4]> {
        let ppp = self.h.ctx.pixels_per_point();
        let (a, b) = (self.at(x0, y0), self.at(x1, y1));
        let img = self.h.render().expect("render");
        let (px0, py0) = ((a.x.min(b.x) * ppp) as u32, (a.y.min(b.y) * ppp) as u32);
        let (px1, py1) = ((a.x.max(b.x) * ppp) as u32, (a.y.max(b.y) * ppp) as u32);
        (py0..py1).flat_map(|y| (px0..px1).map(move |x| (x, y))).map(|(x, y)| img.get_pixel(x, y).0).collect()
    }

    // ---- Drawing helpers ------------------------------------------------------------

    /// Draw a rectangle with the Rectangle tool (F6), then go back to the Pick tool.
    pub fn draw_rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) -> ObjectId {
        self.key(Key::F6);
        self.drag_doc((x0, y0), (x1, y1));
        self.pick_tool();
        *self.ids().last().expect("a rectangle was drawn")
    }

    pub fn pick_tool(&mut self) {
        self.click("Pick");
    }
}

pub fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.05
}

pub fn rect_close(r: Rect, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    close(r.x0, x0) && close(r.y0, y0) && close(r.x1, x1) && close(r.y1, y1)
}

/// A fresh temp directory for one test.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vs-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
