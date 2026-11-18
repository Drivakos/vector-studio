//! Canvas tools: Pick, Shape, Rectangle, Ellipse, Polygon, Star, Freehand, Bézier and Pen.
//!
//! Positions come in three spaces: `Pos2` (egui window points), "local" screen points
//! relative to the canvas origin (what `Viewport` maps), and document points.

use egui::{Color32, Pos2, Rect, Stroke};
use vs_core::kurbo::{self, Affine, BezPath, PathEl, Point, Shape as _, Vec2};
use vs_core::controls::{self, Control};
use vs_core::nodes::{self, HandleRef};
use vs_core::{ObjectId, Shape, Style};

use crate::app::AppState;
use crate::free_transform::FreeMode;
use crate::toolbox::Tool;

/// Pick distance for objects, nodes and handles, in screen points.
const HIT: f64 = 5.0;
/// Pointer travel that turns a press into a drag, in screen points.
const DRAG_START: f64 = 3.0;
const HANDLE_SIZE: f32 = 7.0;
/// Snap distance, in screen points.
const SNAP: f64 = 6.0;
/// Handles sit this far outside the selection bounds, in screen points.
const HANDLE_GAP: f64 = 5.0;
const ACCENT: Color32 = Color32::from_rgb(0x1E, 0x6F, 0xD9);
/// 5 mm, in points.
pub const DEFAULT_ERASER_SIZE: f64 = 72.0 / 25.4 * 5.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PickMode {
    #[default]
    Scale,
    Rotate,
}

/// Settings the property bar edits for the shape and text tools.
#[derive(Clone, Debug)]
pub struct ToolSettings {
    pub polygon_sides: u32,
    pub star_points: u32,
    /// Star sharpness as inner radius / outer radius.
    pub star_inner_ratio: f64,
    /// Font, size and alignment for new text (its content is unused).
    pub text: vs_core::Text,
    /// Eraser tip diameter, in points.
    pub eraser_size: f64,
    /// How many times a new spiral turns.
    pub spiral_turns: u32,
    /// Graph Paper: cells across and down.
    pub graph_columns: u32,
    pub graph_rows: u32,
    /// Table: rows and columns of new tables.
    pub table_rows: u32,
    pub table_columns: u32,
    /// The outline the Common Shapes tool draws.
    pub common_shape: vs_core::construct::CommonShape,
    /// Smart Fill: the fill colour of new areas, and whether they get the default outline.
    pub smart_fill: vs_core::Rgba,
    pub smart_fill_outline: bool,
    /// Shape-edit brushes: brush diameter in points.
    pub brush_size: f64,
    /// Smooth: how far each dab eases the curve, 0 to 100.
    pub smoothness: f64,
    /// Roughen: spike height in points.
    pub spike_size: f64,
    /// Twirl, Attract and Repel: how fast holding the brush reshapes, 1 to 100.
    pub brush_speed: f64,
    /// Attract and Repel: whether the brush pushes away instead of pulling in.
    pub repel: bool,
    /// Twirl: whether it turns clockwise instead of counterclockwise.
    pub twirl_clockwise: bool,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self { polygon_sides: 5, star_points: 5, star_inner_ratio: 0.5, text: vs_core::Text::new(""), eraser_size: DEFAULT_ERASER_SIZE, spiral_turns: 4, graph_columns: 4, graph_rows: 3, table_rows: crate::table_tool::DEFAULT_ROWS, table_columns: crate::table_tool::DEFAULT_COLUMNS, common_shape: Default::default(), smart_fill: vs_core::Rgba::rgb(0xC0, 0xC0, 0xC0), smart_fill_outline: false, brush_size: 20.0, smoothness: 50.0, spike_size: 3.0, brush_speed: 50.0, repel: false, twirl_clockwise: false }
    }
}

type Originals = Vec<(ObjectId, Affine)>;

enum Drag {
    Marquee { start: Point, add: bool },
    /// Freehand Pick: the loop drawn so far.
    Lasso { points: Vec<Point>, add: bool },
    NodeMarquee { start: Point, add: bool },
    Move { press: Point, start: Point, originals: Originals, moved: bool, was_selected: bool },
    /// `grab` is where the pointer took the handle, relative to the bounds point it controls.
    /// `frames` are the paragraph text frames as they were, `last` the scale made so far.
    Scale { handle: usize, bounds: kurbo::Rect, grab: Vec2, originals: Originals, frames: Vec<(ObjectId, kurbo::Size)>, last: Affine },
    Rotate { center: Point, start_angle: f64, originals: Originals },
    /// Free Transform: about `anchor`; `start` is where the pointer was once the drag began,
    /// `size` the selection's width and height.
    Free { anchor: Point, start: Option<Point>, size: Vec2, originals: Originals },
    Create { start: Point },
    Freehand { points: Vec<Point> },
    PenHandle { node: Point },
    Nodes { object: ObjectId, start: Point, original: BezPath },
    Handle { object: ObjectId, handle: HandleRef },
    Corner { object: ObjectId, corner: Point },
    /// A perspective corner, 0 top-left then clockwise.
    Perspective { object: ObjectId, corner: usize },
    /// An ellipse, polygon or star node; `shape` and `transform` are as they were on press.
    Control { object: ObjectId, control: Control, shape: Shape, transform: Affine },
    /// A guideline of the active page, by index.
    Guide { index: usize },
    /// Interactive Fill: dragging out a fountain fill on `object` from `start`.
    FillVector { object: ObjectId, start: Point },
    /// Interactive Fill: a fountain fill's start or end handle.
    FillHandle { object: ObjectId, end: bool },
    /// Crop tool: dragging out the area to keep.
    Crop { start: Point },
    /// Knife tool: dragging the cutting line.
    Knife { start: Point },
    /// Eraser tool: the path of the eraser so far.
    Erase { points: Vec<Point> },
    /// Smooth: where the last dab was.
    Brush { last: Point },
    /// Roughen: the objects as they were at the press, and the stroke so far. Each move
    /// roughens the originals along the whole stroke again, so spikes alternate evenly.
    Roughen { originals: Vec<vs_core::Object>, stroke: Vec<Point> },
    /// Twirl, Attract and Repel: a dab every frame while the button is down.
    Warp,
    /// Smudge: where the last dab was; each dab carries the outline on to the pointer.
    Smudge { last: Point },
    /// Smear: the objects as they were at the press, and where it was. Each move pulls
    /// the outline that was under the press out to the pointer again.
    Smear { originals: Vec<vs_core::Object>, press: Point },
    /// Text tool: dragging out a paragraph text frame (a click makes artistic text).
    TextFrame { start: Point },
    /// 2-Point Line tool: dragging the line.
    Line { start: Point },
    /// 3-point tools: dragging the first edge (or axis); a click then sets the third point.
    FirstEdge { start: Point },
    /// Spiral tool: dragging out its box.
    Spiral { start: Point },
    /// Graph Paper tool: dragging out the grid's box.
    GraphPaper { start: Point },
    /// Table tool: dragging out a new table's box.
    Table { start: Point },
    /// Table tool: picking cells. `again` while the press was on the one cell already
    /// picked and the pick has not moved: releasing then types into it.
    TableCells { again: bool },
}

/// A Bézier or Pen path being drawn, in document space.
struct PenPath {
    /// The tool drawing it: Bézier and Pen make the path as drawn, Polyline straight
    /// segments, B-Spline a spline with these nodes as control points.
    kind: Tool,
    path: BezPath,
    /// Outgoing handle of the last node.
    out_handle: Option<Point>,
}

/// Transient tool state (not saved).
#[derive(Default)]
pub struct ToolState {
    pub pick_mode: PickMode,
    pub free_mode: crate::free_transform::FreeMode,
    /// Selected nodes (path element indices) of the object being shape-edited.
    pub nodes: Vec<usize>,
    pub settings: ToolSettings,
    /// Text being typed into with the Text tool.
    pub text: Option<crate::text_tool::TextEdit>,
    /// Crop tool: the area waiting to be cropped to (Enter or double-click applies it).
    pub crop: Option<kurbo::Rect>,
    pub eyedropper: crate::paint_tools::Eyedropper,
    drag: Option<Drag>,
    pen: Option<PenPath>,
    /// 3-point tools: the first edge, waiting for the click that sets the third point.
    first_edge: Option<(Point, Point)>,
    /// Angular Dimension: the second arm's point, waiting for the click that places the arc.
    arm: Option<Point>,
    pointer: Option<Point>,
    /// Alignment and dynamic guides to draw this frame.
    guide_lines: Vec<vs_core::align_guides::GuideLine>,
    /// Time and place of the last press, to spot double-clicks at press time.
    last_press: Option<(f64, Pos2)>,
    /// Seconds since the last frame, for brushes that work while held.
    frame_dt: f64,
}

impl ToolState {
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn has_open_path(&self) -> bool {
        self.pen.is_some()
    }

    /// A 3-point tool is waiting for its third point.
    pub fn has_first_edge(&self) -> bool {
        self.first_edge.is_some()
    }
}

/// Maps between the canvas' screen space and the document.
#[derive(Clone, Copy)]
struct View {
    origin: Pos2,
    canvas: Rect,
    to_screen: Affine,
    zoom: f64,
}

impl View {
    fn new(state: &AppState, canvas: Rect) -> Self {
        Self { origin: canvas.min, canvas, to_screen: state.viewport.transform(), zoom: state.viewport.zoom() }
    }
    fn doc(&self, p: Pos2) -> Point {
        self.to_screen.inverse() * Point::new((p.x - self.origin.x) as f64, (p.y - self.origin.y) as f64)
    }
    fn screen(&self, p: Point) -> Pos2 {
        let s = self.to_screen * p;
        Pos2::new(self.origin.x + s.x as f32, self.origin.y + s.y as f32)
    }
    /// Screen distance → document distance.
    fn doc_len(&self, screen: f64) -> f64 {
        screen / self.zoom
    }
    fn screen_dist(&self, a: Point, b: Point) -> f64 {
        (a - b).hypot() * self.zoom
    }
}

fn is_shape_tool(tool: Tool) -> bool {
    matches!(tool, Tool::Rectangle | Tool::Ellipse | Tool::Polygon | Tool::Star | Tool::CommonShapes)
}

pub fn is_implemented(tool: Tool) -> bool {
    is_shape_tool(tool)
        || matches!(tool, Tool::Pick | Tool::FreehandPick | Tool::FreeTransform | Tool::Shape | Tool::Zoom | Tool::Pan | Tool::Freehand | Tool::Bezier | Tool::Pen | Tool::Text)
        || matches!(tool, Tool::InteractiveFill | Tool::SmartFill | Tool::Transparency | Tool::ColorEyedropper | Tool::AttributesEyedropper)
        || matches!(tool, Tool::Crop | Tool::Knife | Tool::VirtualSegmentDelete | Tool::Eraser)
        || is_brush(tool)
        || matches!(tool, Tool::TwoPointLine | Tool::Polyline | Tool::BSpline | Tool::Spiral | Tool::GraphPaper | Tool::Table)
        || is_three_point(tool)
}

/// Tools that take a drag for one edge, then a click for the third point (dimensions
/// included: the click places the dimension line).
fn is_three_point(tool: Tool) -> bool {
    matches!(tool, Tool::ThreePointCurve | Tool::ThreePointRectangle | Tool::ThreePointEllipse | Tool::AngularDimension | Tool::Callout)
        || crate::dimensions::kind(tool).is_some()
}

pub fn cursor(tool: Tool) -> egui::CursorIcon {
    match tool {
        Tool::Pick | Tool::FreehandPick => egui::CursorIcon::Default,
        Tool::Shape => egui::CursorIcon::PointingHand,
        Tool::Text => egui::CursorIcon::Text,
        Tool::ColorEyedropper | Tool::AttributesEyedropper | Tool::Transparency | Tool::SmartFill => egui::CursorIcon::PointingHand,
        _ => egui::CursorIcon::Crosshair,
    }
}

// ---------------------------------------------------------------------------------------
// Input

/// Pointer input for the active tool. `response` is the canvas widget's response.
pub fn tool_input(ui: &egui::Ui, state: &mut AppState, response: &egui::Response, canvas: Rect) {
    let view = View::new(state, canvas);
    let (pressed, released, pos, mods, time, dt) = ui.input(|i| {
        (i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.latest_pos(), i.modifiers, i.time, i.stable_dt)
    });
    state.tools.frame_dt = f64::from(dt).clamp(0.0, 0.1);
    if matches!(state.tools.drag, Some(Drag::Warp)) {
        // Holding the brush still keeps working.
        ui.ctx().request_repaint();
    }
    state.tools.guide_lines.clear();
    let Some(pos) = pos else {
        state.tools.pointer = None;
        return;
    };
    let mut p = view.doc(pos);
    if snaps_pointer(state.tool) {
        p = snapper(state, &view, &[]).point(p);
        p = guide_point(state, &view, p);
    }
    state.tools.pointer = Some(p);

    if pressed && response.hovered() {
        let double = state.tools.last_press.is_some_and(|(t, at)| time - t < 0.4 && at.distance(pos) < 4.0);
        state.tools.last_press = if double { None } else { Some((time, pos)) };
        if !crate::edit_tools::press_copy_from(state, view.doc(pos), view.doc_len(HIT))
            && !crate::edit_tools::press_place_inside(state, view.doc(pos), view.doc_len(HIT))
            && !crate::edit_tools::press_copy_perspective(state, view.doc(pos), view.doc_len(HIT))
        {
            press(state, &view, p, mods, double);
        }
    } else if state.tools.drag.is_some() {
        drag_to(state, &view, p, mods);
        if released {
            release(state, &view, p, mods);
        }
    }
}

/// `p` lined up with other objects by the alignment guides, or else pulled onto a
/// dynamic guide, when those are on; the guides it follows are drawn this frame.
fn guide_point(state: &mut AppState, view: &View, p: Point) -> Point {
    let page = state.doc.active_page();
    let reach = view.doc_len(SNAP);
    if state.prefs.alignment_guides {
        let (shift, lines) = vs_core::align_guides::alignment(page, kurbo::Rect::from_points(p, p), &[], reach);
        if !lines.is_empty() {
            state.tools.guide_lines = lines;
            return p + shift;
        }
    }
    if state.prefs.dynamic_guides
        && let Some((on, line)) = vs_core::align_guides::dynamic_guide(page, p, &[], reach)
    {
        state.tools.guide_lines = vec![line];
        return on;
    }
    p
}

/// Moved objects lined up with others by the alignment guides, on the axes snapping
/// left alone (`snapped`) and the move is free to go (`constrained` keeps it to one).
fn align_moved(state: &mut AppState, view: &View, originals: &Originals, delta: Vec2, snapped: Vec2, constrained: bool) {
    if !state.prefs.alignment_guides {
        return;
    }
    let ids: Vec<ObjectId> = originals.iter().map(|o| o.0).collect();
    let Some(bounds) = state.doc.active_page().bounds_of(&ids) else { return };
    let (mut shift, _) = vs_core::align_guides::alignment(state.doc.active_page(), bounds, &ids, view.doc_len(SNAP));
    if snapped.x != 0.0 || (constrained && delta.x == 0.0) {
        shift.x = 0.0;
    }
    if snapped.y != 0.0 || (constrained && delta.x != 0.0) {
        shift.y = 0.0;
    }
    if shift != Vec2::ZERO {
        apply(state, originals, Affine::translate(delta + snapped + shift));
    }
    if let Some(bounds) = state.doc.active_page().bounds_of(&ids) {
        state.tools.guide_lines = vs_core::align_guides::alignment(state.doc.active_page(), bounds, &ids, 0.0).1;
    }
}

/// Snap targets for a gesture that moves or edits `exclude`.
fn snapper(state: &AppState, view: &View, exclude: &[ObjectId]) -> vs_core::Snapper {
    vs_core::Snapper::new(state.doc.active_page(), &state.snap_options(), exclude, view.doc_len(SNAP))
}

/// Tools whose every click and drag position snaps.
fn snaps_pointer(tool: Tool) -> bool {
    is_shape_tool(tool) || is_three_point(tool) || matches!(tool, Tool::Bezier | Tool::Pen | Tool::Text | Tool::TwoPointLine | Tool::Polyline | Tool::BSpline | Tool::Spiral | Tool::GraphPaper)
        || matches!(tool, Tool::InteractiveFill | Tool::Transparency | Tool::ColorEyedropper | Tool::AttributesEyedropper)
}

fn constrain(mods: egui::Modifiers) -> bool {
    mods.command || mods.ctrl
}

fn selection_originals(state: &AppState) -> Originals {
    let page = state.doc.active_page();
    // Locked objects stay put.
    state.selection.iter().filter_map(|id| page.object(*id).filter(|o| !o.locked).map(|o| (*id, o.transform))).collect()
}

fn apply(state: &mut AppState, originals: &Originals, t: Affine) {
    let page = state.doc.active_page_mut();
    for (id, original) in originals {
        if let Some(obj) = page.object_mut(*id) {
            obj.transform = t * *original;
        }
    }
}

fn press(state: &mut AppState, view: &View, p: Point, mods: egui::Modifiers, double: bool) {
    match state.tool {
        Tool::Pick => press_pick(state, view, p, mods, double),
        // Like Pick on objects and handles; on empty space a drag draws a loop.
        Tool::FreehandPick => {
            press_pick(state, view, p, mods, double);
            if let Some(Drag::Marquee { add, .. }) = state.tools.drag {
                state.tools.drag = Some(Drag::Lasso { points: vec![p], add });
            }
        }
        Tool::FreeTransform => press_free(state, view, p, mods),
        Tool::Shape => press_shape(state, view, p, mods, double),
        Tool::Freehand => state.tools.drag = Some(Drag::Freehand { points: vec![p] }),
        Tool::Bezier | Tool::Pen => press_pen(state, view, p),
        // Clicks only: no curve handles.
        Tool::Polyline | Tool::BSpline => {
            press_pen(state, view, p);
            if matches!(state.tools.drag, Some(Drag::PenHandle { .. })) {
                state.tools.drag = None;
            }
        }
        Tool::TwoPointLine => state.tools.drag = Some(Drag::Line { start: p }),
        Tool::Spiral => state.tools.drag = Some(Drag::Spiral { start: p }),
        Tool::GraphPaper => state.tools.drag = Some(Drag::GraphPaper { start: p }),
        // On a table: pick cells. Elsewhere: drag out a new table.
        Tool::Table => {
            state.tools.drag = Some(match crate::table_tool::press(state, p, mods.shift) {
                Some(again) => Drag::TableCells { again },
                None => Drag::Table { start: p },
            });
        }
        // In a table cell: type into it.
        Tool::Text if crate::table_tool::place_at(state, p).is_some() => crate::table_tool::type_at(state, p),
        // A click on a segment measures it; the next click places the dimension line.
        Tool::SegmentDimension if state.tools.first_edge.is_none() => {
            state.tools.first_edge = vs_core::dimension::segment_at(state.doc.active_page(), p, view.doc_len(HIT));
        }
        Tool::AngularDimension if state.tools.first_edge.is_some() => match (state.tools.first_edge, state.tools.arm.take()) {
            (Some((vertex, a)), Some(b)) => {
                state.tools.first_edge = None;
                crate::dimensions::finish_angular(state, vertex, a, b, p);
            }
            _ => state.tools.arm = Some(p),
        },
        t if is_three_point(t) => match state.tools.first_edge.take() {
            Some((a, b)) => finish_three_point(state, a, b, p),
            None => state.tools.drag = Some(Drag::FirstEdge { start: p }),
        },
        // On text: edit it. Elsewhere: a drag makes paragraph text, a click artistic text.
        Tool::Text if crate::text_tool::text_at(state, p, view.doc_len(HIT)).is_some() => crate::text_tool::press(state, p, view.doc_len(HIT)),
        Tool::Text => {
            crate::text_tool::finish(state);
            state.tools.drag = Some(Drag::TextFrame { start: p });
        }
        Tool::InteractiveFill => press_fill(state, view, p),
        Tool::Transparency => {
            crate::paint_tools::select_at(state, p, view.doc_len(HIT));
        }
        Tool::ColorEyedropper | Tool::AttributesEyedropper => crate::paint_tools::press_eyedropper(state, p, view.doc_len(HIT)),
        // A drag marks a new area; a click keeps the one there is.
        Tool::Crop => match state.tools.crop {
            Some(area) if double && area.contains(p) => apply_crop(state),
            _ => state.tools.drag = Some(Drag::Crop { start: p }),
        },
        Tool::Knife => state.tools.drag = Some(Drag::Knife { start: p }),
        Tool::VirtualSegmentDelete => {
            if state.doc.delete_virtual_segment(p, view.doc_len(HIT)) {
                let page = state.doc.active_page();
                let gone: Vec<ObjectId> = state.selection.iter().copied().filter(|id| page.object(*id).is_none()).collect();
                state.selection.retain(|id| !gone.contains(id));
                state.marquee_selection = None;
            }
        }
        Tool::Eraser => state.tools.drag = Some(Drag::Erase { points: vec![p] }),
        t if is_brush(t) => press_brush(state, view, p),
        Tool::SmartFill => {
            let s = &state.tools.settings;
            let mut style = vs_core::Style::filled(s.smart_fill);
            if s.smart_fill_outline {
                style.outline = Style::new_object().outline;
            }
            if let Some(id) = state.doc.smart_fill(p, style) {
                state.selection = vec![id];
                state.marquee_selection = None;
            }
        }
        t if is_shape_tool(t) => state.tools.drag = Some(Drag::Create { start: p }),
        _ => {}
    }
}

/// Interactive Fill: grab a fountain handle of the selected object, or select the object
/// under the pointer and drag out a new fountain on it.
fn press_fill(state: &mut AppState, view: &View, p: Point) {
    if let Some((object, g, t)) = crate::paint_tools::selected_gradient(state) {
        for (end, at) in [(true, g.end), (false, g.start)] {
            if view.screen_dist(t * at, p) <= HIT {
                state.tools.drag = Some(Drag::FillHandle { object, end });
                return;
            }
        }
    }
    if let Some(object) = crate::paint_tools::select_at(state, p, view.doc_len(HIT)) {
        state.tools.drag = Some(Drag::FillVector { object, start: p });
    }
}

/// Handle positions around `bounds`, clockwise from top-left, in document space.
fn handle_points(bounds: kurbo::Rect, view: &View) -> [Point; 8] {
    let g = view.doc_len(HANDLE_GAP);
    let r = bounds.inflate(g, g);
    let c = r.center();
    [
        Point::new(r.x0, r.y0),
        Point::new(c.x, r.y0),
        Point::new(r.x1, r.y0),
        Point::new(r.x1, c.y),
        Point::new(r.x1, r.y1),
        Point::new(c.x, r.y1),
        Point::new(r.x0, r.y1),
        Point::new(r.x0, c.y),
    ]
}

fn is_corner(handle: usize) -> bool {
    handle.is_multiple_of(2)
}

fn press_pick(state: &mut AppState, view: &View, p: Point, mods: egui::Modifiers, double: bool) {
    let page = state.doc.active_page();
    if let Some(bounds) = page.bounds_of(&state.selection) {
        let near = handle_points(bounds, view)
            .iter()
            .position(|h| view.screen_dist(*h, p) <= HIT)
            .filter(|&h| state.tools.pick_mode == PickMode::Scale || is_corner(h));
        if let Some(handle) = near {
            let originals = selection_originals(state);
            state.tools.drag = Some(match state.tools.pick_mode {
                PickMode::Scale => {
                    let frames = text_frames(state, &originals);
                    Drag::Scale { handle, bounds, grab: p - edge_points(bounds)[handle], originals, frames, last: Affine::IDENTITY }
                }
                PickMode::Rotate => {
                    let center = bounds.center();
                    Drag::Rotate { center, start_angle: (p - center).atan2(), originals }
                }
            });
            return;
        }
    }
    if let Some(index) = guide_at(state, view, p) {
        select_guide(state, index, mods.shift);
        state.tools.drag = Some(Drag::Guide { index });
        return;
    }
    state.guide_selection = None;
    match page.hit_test(p, view.doc_len(HIT)) {
        Some(hit) if mods.shift => {
            state.marquee_selection = None;
            if let Some(i) = state.selection.iter().position(|id| *id == hit) {
                state.selection.remove(i);
            } else {
                state.selection.push(hit);
            }
        }
        // Double-clicking text types into it with the Text tool.
        Some(hit) if double && page.object(hit).is_some_and(|o| !o.locked && matches!(&o.shape, Shape::Text(t) if t.on_path.is_none())) => {
            state.tool = Tool::Text;
            state.toolbox.remember(Tool::Text);
            crate::text_tool::press(state, p, view.doc_len(HIT));
        }
        // Double-clicking a table picks the cell there with the Table tool.
        Some(hit) if double && state.doc.is_table(hit) => {
            state.tool = Tool::Table;
            state.toolbox.remember(Tool::Table);
            crate::table_tool::press(state, p, false);
        }
        // Double-clicking an object switches to the Shape tool on it.
        Some(hit) if double => {
            state.selection = vec![hit];
            state.marquee_selection = None;
            state.tools.pick_mode = PickMode::Scale;
            state.tools.nodes.clear();
            state.tool = Tool::Shape;
            state.toolbox.remember(Tool::Shape);
        }
        Some(hit) => {
            let was_selected = state.selection.contains(&hit);
            if !was_selected {
                state.selection = vec![hit];
                state.marquee_selection = None;
                state.tools.pick_mode = PickMode::Scale;
            }
            let originals = selection_originals(state);
            state.tools.drag = Some(Drag::Move { press: p, start: p, originals, moved: false, was_selected });
        }
        None => state.tools.drag = Some(Drag::Marquee { start: p, add: mods.shift }),
    }
}

/// Free Transform: an unselected object is selected (Shift adds it); anywhere else, with
/// something selected, a drag transforms the selection about the press point.
fn press_free(state: &mut AppState, view: &View, p: Point, mods: egui::Modifiers) {
    let hit = state.doc.active_page().hit_test(p, view.doc_len(HIT)).filter(|id| !state.selection.contains(id));
    if let Some(hit) = hit {
        if !mods.shift {
            state.selection.clear();
        }
        state.selection.push(hit);
        state.marquee_selection = None;
    } else if let Some(b) = state.doc.active_page().bounds_of(&state.selection) {
        let size = Vec2::new(b.width(), b.height());
        state.tools.drag = Some(Drag::Free { anchor: p, start: None, size, originals: selection_originals(state) });
    } else {
        state.tools.drag = Some(Drag::Marquee { start: p, add: mods.shift });
    }
}

/// The object the Shape tool edits: the single selected object, if it is not locked.
fn shape_target(state: &AppState) -> Option<ObjectId> {
    match state.selection.as_slice() {
        [id] if state.doc.active_page().object(*id).is_some_and(|o| !o.locked) => Some(*id),
        _ => None,
    }
}

fn press_shape(state: &mut AppState, view: &View, p: Point, mods: egui::Modifiers, double: bool) {
    let tol = view.doc_len(HIT);
    if let Some(id) = shape_target(state)
        && let Some(obj) = state.doc.active_page().object(id)
    {
        let t = obj.transform;
        if let Some(corners) = obj.perspective_corners() {
            // With a perspective the Shape tool moves its corners, not the nodes.
            if let Some(corner) = corners.iter().position(|c| view.screen_dist(*c, p) <= HIT) {
                state.tools.drag = Some(Drag::Perspective { object: id, corner });
                return;
            }
        } else {
        match &obj.shape {
            Shape::Curve { path } => {
                let node_list = nodes::nodes(path);
                // Handles of selected nodes first: they sit on top.
                for n in node_list.iter().filter(|n| state.tools.nodes.contains(&n.elem)) {
                    for (h, hp) in [n.handle_in, n.handle_out].into_iter().flatten() {
                        if view.screen_dist(t * hp, p) <= HIT {
                            state.tools.drag = Some(Drag::Handle { object: id, handle: h });
                            return;
                        }
                    }
                }
                if let Some(n) = node_list.iter().find(|n| view.screen_dist(t * n.point, p) <= HIT) {
                    let path = path.clone();
                    if double {
                        let mut edited = path;
                        if nodes::delete_node(&mut edited, n.elem) {
                            set_curve(state, id, edited);
                            state.tools.nodes.clear();
                        }
                        return;
                    }
                    let sel = &mut state.tools.nodes;
                    if mods.shift {
                        if let Some(i) = sel.iter().position(|e| *e == n.elem) {
                            sel.remove(i);
                            return;
                        }
                        sel.push(n.elem);
                    } else if !sel.contains(&n.elem) {
                        *sel = vec![n.elem];
                    }
                    state.tools.drag = Some(Drag::Nodes { object: id, start: p, original: path });
                    return;
                }
                if double {
                    let mut world = path.clone();
                    world.apply_affine(t);
                    if let Some((elem, at, d)) = nodes::nearest_segment(&world, p)
                        && d <= tol
                    {
                        let mut edited = path.clone();
                        if let Some(new) = nodes::insert_node(&mut edited, elem, at) {
                            set_curve(state, id, edited);
                            state.tools.nodes = vec![new];
                        }
                    }
                    return;
                }
                if obj.hit(p, tol) {
                    return;
                }
            }
            Shape::Rectangle { rect, .. } => {
                let corners = [rect.origin(), Point::new(rect.x1, rect.y0), Point::new(rect.x1, rect.y1), Point::new(rect.x0, rect.y1)];
                if let Some(c) = corners.iter().find(|c| view.screen_dist(t * **c, p) <= HIT) {
                    state.tools.drag = Some(Drag::Corner { object: id, corner: *c });
                    return;
                }
            }
            shape => {
                if let Some((control, _)) = controls::controls(shape).into_iter().find(|(_, at)| view.screen_dist(t * *at, p) <= HIT) {
                    state.tools.drag = Some(Drag::Control { object: id, control, shape: shape.clone(), transform: t });
                    return;
                }
            }
        }
        }
    }
    match state.doc.active_page().hit_test(p, tol) {
        Some(hit) => {
            if state.selection != [hit] {
                state.selection = vec![hit];
                state.tools.nodes.clear();
            }
        }
        None if shape_target(state).is_some_and(|id| {
            state.doc.active_page().object(id).is_some_and(|o| o.perspective.is_none() && matches!(o.shape, Shape::Curve { .. }))
        }) =>
        {
            state.tools.drag = Some(Drag::NodeMarquee { start: p, add: mods.shift });
        }
        None => {
            state.selection.clear();
            state.tools.nodes.clear();
        }
    }
}

fn set_curve(state: &mut AppState, id: ObjectId, path: BezPath) {
    if let Some(obj) = state.doc.active_page_mut().object_mut(id) {
        obj.shape = Shape::Curve { path };
    }
}

fn pen_node_count(path: &BezPath) -> usize {
    path.elements().iter().filter(|e| !matches!(e, PathEl::ClosePath)).count()
}

fn press_pen(state: &mut AppState, view: &View, p: Point) {
    let Some(pen) = state.tools.pen.as_mut() else {
        let mut path = BezPath::new();
        path.move_to(p);
        state.tools.pen = Some(PenPath { kind: state.tool, path, out_handle: None });
        state.tools.drag = Some(Drag::PenHandle { node: p });
        return;
    };
    let els = pen.path.elements();
    let first = match els.first() {
        Some(PathEl::MoveTo(f)) => *f,
        _ => p,
    };
    let last = els.last().and_then(|e| e.end_point()).unwrap_or(first);
    // Clicking the last node again (or the second click of a double-click) finishes.
    if view.screen_dist(last, p) <= HIT {
        finish_pen(state);
        return;
    }
    if pen_node_count(&pen.path) >= 2 && view.screen_dist(first, p) <= HIT {
        // Close onto the first node, mirroring its outgoing handle.
        let first_in = match els.get(1) {
            Some(PathEl::CurveTo(c1, _, _)) => Some(first + (first - *c1)),
            _ => None,
        };
        match (pen.out_handle, first_in) {
            (None, None) => {}
            (out, inn) => pen.path.curve_to(out.unwrap_or(last), inn.unwrap_or(first), first),
        }
        pen.path.close_path();
        pen.out_handle = None;
        finish_pen(state);
        return;
    }
    match pen.out_handle.take() {
        Some(h) => pen.path.curve_to(h, p, p),
        None => pen.path.line_to(p),
    }
    state.tools.drag = Some(Drag::PenHandle { node: p });
}

/// Turn the path being drawn into an object (if it has a segment) and stop drawing.
pub fn finish_pen(state: &mut AppState) {
    let Some(pen) = state.tools.pen.take() else { return };
    if matches!(state.tools.drag, Some(Drag::PenHandle { .. })) {
        state.tools.drag = None;
    }
    if pen_node_count(&pen.path) >= 2 {
        let path = if pen.kind == Tool::BSpline {
            let nodes: Vec<Point> = pen.path.elements().iter().filter_map(|e| match e {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
                _ => None,
            }).collect();
            let closed = pen.path.elements().iter().any(|e| matches!(e, PathEl::ClosePath));
            vs_core::construct::bspline(&nodes, closed)
        } else {
            pen.path
        };
        let id = state.doc.add_object(Shape::Curve { path }, Style::new_object());
        state.selection = vec![id];
    }
}

fn is_brush(tool: Tool) -> bool {
    matches!(tool, Tool::Smooth | Tool::RoughenBrush | Tool::Twirl | Tool::AttractRepel | Tool::Smudge | Tool::SmearBrush)
}

/// The brushes work on the selection; with nothing they can reshape selected, a
/// press selects the object under the pointer first.
fn press_brush(state: &mut AppState, view: &View, p: Point) {
    let page = state.doc.active_page();
    if !state.selected_objects().any(vs_core::Object::can_brush) {
        let hit = page.hit_test(p, view.doc_len(HIT)).filter(|id| page.object(*id).is_some_and(vs_core::Object::can_brush));
        state.selection = hit.into_iter().collect();
        state.marquee_selection = None;
    }
    if !state.selected_objects().any(vs_core::Object::can_brush) {
        return;
    }
    if state.tool == Tool::RoughenBrush {
        let originals: Vec<vs_core::Object> = state.selected_objects().filter(|o| o.can_brush()).cloned().collect();
        let stroke = vec![p];
        roughen(state, &originals, &stroke);
        state.tools.drag = Some(Drag::Roughen { originals, stroke });
    } else if matches!(state.tool, Tool::Twirl | Tool::AttractRepel) {
        warp_at(state, p);
        state.tools.drag = Some(Drag::Warp);
    } else if state.tool == Tool::Smudge {
        state.tools.drag = Some(Drag::Smudge { last: p });
    } else if state.tool == Tool::SmearBrush {
        let originals: Vec<vs_core::Object> = state.selected_objects().filter(|o| o.can_brush()).cloned().collect();
        state.tools.drag = Some(Drag::Smear { originals, press: p });
    } else {
        smooth_at(state, p);
        state.tools.drag = Some(Drag::Brush { last: p });
    }
}

fn smooth_at(state: &mut AppState, p: Point) {
    let ids = state.selection.clone();
    let s = &state.tools.settings;
    state.doc.smooth_at(&ids, p, s.brush_size / 2.0, s.smoothness / 100.0);
}

/// At full speed, at the brush's centre: Attract and Repel move points this fraction of
/// the way per second, Twirl turns them this many radians per second (a full turn).
const ATTRACT_RATE: f64 = 4.0;
const TWIRL_RATE: f64 = std::f64::consts::TAU;

/// One frame's worth of Twirl, Attract or Repel at `p`.
fn warp_at(state: &mut AppState, p: Point) {
    let ids = state.selection.clone();
    let s = &state.tools.settings;
    let amount = s.brush_speed / 100.0 * state.tools.frame_dt;
    let how = match state.tool {
        Tool::Twirl => vs_core::brush::Warp::Twirl(amount * TWIRL_RATE * if s.twirl_clockwise { 1.0 } else { -1.0 }),
        _ if s.repel => vs_core::brush::Warp::Repel(amount * ATTRACT_RATE),
        _ => vs_core::brush::Warp::Attract(amount * ATTRACT_RATE),
    };
    state.doc.warp_at(&ids, p, s.brush_size / 2.0, how);
}

/// Move the outline under the brush at `at` by `by`.
fn push_at(state: &mut AppState, at: Point, by: Vec2) {
    let ids = state.selection.clone();
    let radius = state.tools.settings.brush_size / 2.0;
    state.doc.warp_at(&ids, at, radius, vs_core::brush::Warp::Push(by));
}

/// Put the brushed objects back as they were at the press.
fn restore(state: &mut AppState, originals: &[vs_core::Object]) {
    let page = state.doc.active_page_mut();
    for original in originals {
        if let Some(obj) = page.object_mut(original.id) {
            *obj = original.clone();
        }
    }
}

/// Put `originals` back, then roughen them along `stroke`.
fn roughen(state: &mut AppState, originals: &[vs_core::Object], stroke: &[Point]) {
    restore(state, originals);
    let ids: Vec<ObjectId> = originals.iter().map(|o| o.id).collect();
    let s = &state.tools.settings;
    state.doc.roughen_along(&ids, stroke, s.brush_size / 2.0, s.spike_size);
}

/// A 3-point tool's third click: make the curve, rectangle or ellipse on edge `a` → `b`.
fn finish_three_point(state: &mut AppState, a: Point, b: Point, c: Point) {
    if let Some(kind) = crate::dimensions::kind(state.tool) {
        crate::dimensions::finish(state, kind, a, b, c);
        return;
    }
    if state.tool == Tool::Callout {
        crate::dimensions::finish_callout(state, a, b, c);
        return;
    }
    let Some((shape, transform)) = three_point_shape(state.tool, a, b, c) else { return };
    let id = state.doc.add_object(shape, Style::new_object());
    if let Some(obj) = state.doc.active_page_mut().object_mut(id) {
        obj.transform = transform;
    }
    state.selection = vec![id];
    state.marquee_selection = None;
}

/// The shape a 3-point tool makes from edge `a` → `b` and third point `c`, with its
/// transform. Rectangles and ellipses keep their kind, turned along the edge.
fn three_point_shape(tool: Tool, a: Point, b: Point, c: Point) -> Option<(Shape, Affine)> {
    let w = (b - a).hypot();
    let place = Affine::translate(a.to_vec2()) * Affine::rotate((b - a).atan2());
    let h = vs_core::construct::side_distance(a, b, c);
    if w < 1e-9 {
        return None;
    }
    Some(match tool {
        Tool::ThreePointCurve => (Shape::Curve { path: vs_core::construct::three_point_curve(a, b, c) }, Affine::IDENTITY),
        Tool::ThreePointRectangle if h.abs() > 1e-9 => (Shape::Rectangle { rect: kurbo::Rect::new(0.0, h.min(0.0), w, h.max(0.0)), corner_radius: 0.0 }, place),
        Tool::ThreePointEllipse if h.abs() > 1e-9 => (Shape::Ellipse { rect: kurbo::Rect::new(0.0, -h.abs(), w, h.abs()), arc: None }, place),
        _ => return None,
    })
}

fn drag_to(state: &mut AppState, view: &View, p: Point, mods: egui::Modifiers) {
    let Some(drag) = state.tools.drag.take() else { return };
    let drag = match drag {
        Drag::Move { press, start, originals, moved, was_selected } => {
            let moved = moved || view.screen_dist(press, p) > DRAG_START;
            if moved {
                let mut delta = p - start;
                if constrain(mods) {
                    delta = vs_core::geometry::constrain_to_axis(delta);
                }
                apply(state, &originals, Affine::translate(delta));
                let ids: Vec<ObjectId> = originals.iter().map(|o| o.0).collect();
                let mut snap = Vec2::ZERO;
                if let Some(bounds) = state.doc.active_page().bounds_of(&ids) {
                    snap = snapper(state, view, &state.selection).delta(bounds);
                    if constrain(mods) {
                        // Stay on the axis the move is constrained to.
                        snap = if delta.x == 0.0 { Vec2::new(0.0, snap.y) } else { Vec2::new(snap.x, 0.0) };
                    }
                    if snap != Vec2::ZERO {
                        apply(state, &originals, Affine::translate(delta + snap));
                    }
                }
                align_moved(state, view, &originals, delta, snap, constrain(mods));
            }
            Drag::Move { press, start, originals, moved, was_selected }
        }
        Drag::Scale { handle, bounds, grab, originals, frames, .. } => {
            let at = snapper(state, view, &state.selection).point(p - grab);
            let t = scale_transform(handle, bounds, at, mods);
            apply(state, &originals, t);
            // Paragraph text rewraps in a resized frame instead of stretching.
            let page = state.doc.active_page_mut();
            for (id, frame) in &frames {
                let before = originals.iter().find(|o| o.0 == *id).map(|o| o.1);
                if let Some(before) = before
                    && let Some(obj) = page.object_mut(*id)
                    && let Shape::Text(text) = &mut obj.shape
                {
                    text.frame = Some(*frame);
                    obj.reflow_text(before);
                }
            }
            Drag::Scale { handle, bounds, grab, originals, frames, last: t }
        }
        Drag::Rotate { center, start_angle, originals } => {
            let mut angle = (p - center).atan2() - start_angle;
            if constrain(mods) {
                let step = 15f64.to_radians();
                angle = (angle / step).round() * step;
            }
            apply(state, &originals, Affine::rotate_about(angle, center));
            Drag::Rotate { center, start_angle, originals }
        }
        Drag::Free { anchor, mut start, size, originals } => {
            if start.is_none() && view.screen_dist(anchor, p) > DRAG_START {
                start = Some(p);
            }
            if let Some(s) = start {
                apply(state, &originals, crate::free_transform::transform(state.tools.free_mode, anchor, s, p, size, constrain(mods)));
            }
            Drag::Free { anchor, start, size, originals }
        }
        Drag::Freehand { mut points } => {
            if points.last().is_none_or(|l| view.screen_dist(*l, p) > 1.5) {
                points.push(p);
            }
            Drag::Freehand { points }
        }
        Drag::Erase { mut points } => {
            if points.last().is_none_or(|l| view.screen_dist(*l, p) > 1.5) {
                points.push(p);
            }
            Drag::Erase { points }
        }
        Drag::Brush { last } => {
            // A dab every eighth of the brush, so a quick stroke still covers its path.
            let step = state.tools.settings.brush_size / 8.0;
            let distance = (p - last).hypot();
            if distance < step {
                Drag::Brush { last }
            } else {
                let n = (distance / step).floor() as usize;
                for i in 1..=n {
                    smooth_at(state, last.lerp(p, i as f64 / n as f64));
                }
                Drag::Brush { last: p }
            }
        }
        Drag::Warp => {
            warp_at(state, p);
            Drag::Warp
        }
        Drag::Smudge { last } => {
            // A dab every eighth of the brush, each carrying the outline one step on.
            let step = state.tools.settings.brush_size / 8.0;
            let distance = (p - last).hypot();
            if distance < step {
                Drag::Smudge { last }
            } else {
                let n = (distance / step).floor() as usize;
                for i in 0..n {
                    let (from, to) = (last.lerp(p, i as f64 / n as f64), last.lerp(p, (i + 1) as f64 / n as f64));
                    push_at(state, from, to - from);
                }
                Drag::Smudge { last: p }
            }
        }
        Drag::Smear { originals, press } => {
            restore(state, &originals);
            push_at(state, press, p - press);
            Drag::Smear { originals, press }
        }
        Drag::Roughen { originals, mut stroke } => {
            if stroke.last().is_none_or(|l| view.screen_dist(*l, p) > 1.5) {
                stroke.push(p);
                roughen(state, &originals, &stroke);
            }
            Drag::Roughen { originals, stroke }
        }
        Drag::Lasso { mut points, add } => {
            if points.last().is_none_or(|l| view.screen_dist(*l, p) > 1.5) {
                points.push(p);
            }
            Drag::Lasso { points, add }
        }
        Drag::PenHandle { node } => {
            if view.screen_dist(node, p) > DRAG_START
                && let Some(pen) = state.tools.pen.as_mut()
            {
                let mirrored = node + (node - p);
                let n = pen.path.elements().len();
                let prev = n.checked_sub(2).and_then(|i| pen.path.elements()[i].end_point());
                if let Some(last) = pen.path.elements_mut().last_mut() {
                    match last {
                        PathEl::LineTo(end) => *last = PathEl::CurveTo(prev.unwrap_or(*end), mirrored, *end),
                        PathEl::CurveTo(_, c2, _) => *c2 = mirrored,
                        _ => {}
                    }
                }
                pen.out_handle = Some(p);
            }
            Drag::PenHandle { node }
        }
        Drag::Nodes { object, start, original } => {
            if let Some(obj) = state.doc.active_page().object(object) {
                let inv = obj.transform.inverse();
                let mut delta = p - start;
                if constrain(mods) {
                    delta = vs_core::geometry::constrain_to_axis(delta);
                } else if let Some(first) = state.tools.nodes.first()
                    && let Some(node) = nodes::nodes(&original).into_iter().find(|n| n.elem == *first)
                {
                    // The first selected node snaps; the others keep their distance to it.
                    let at = obj.transform * node.point + delta;
                    delta += snapper(state, view, &[object]).point(at) - at;
                }
                let local = (inv * (start + delta)) - (inv * start);
                let mut path = original.clone();
                for elem in &state.tools.nodes {
                    nodes::move_node(&mut path, *elem, local);
                }
                set_curve(state, object, path);
            }
            Drag::Nodes { object, start, original }
        }
        Drag::Handle { object, handle } => {
            let p = snapper(state, view, &[object]).point(p);
            if let Some(obj) = state.doc.active_page_mut().object_mut(object) {
                let local = obj.transform.inverse() * p;
                if let Shape::Curve { path } = &mut obj.shape {
                    nodes::set_handle(path, handle, local);
                }
            }
            Drag::Handle { object, handle }
        }
        Drag::Corner { object, corner } => {
            if let Some(obj) = state.doc.active_page_mut().object_mut(object) {
                let local = obj.transform.inverse() * p;
                if let Shape::Rectangle { rect, corner_radius } = &mut obj.shape {
                    let sx = if corner.x == rect.x0 { 1.0 } else { -1.0 };
                    let sy = if corner.y == rect.y0 { 1.0 } else { -1.0 };
                    let r = ((local.x - corner.x) * sx).max((local.y - corner.y) * sy).max(0.0);
                    *corner_radius = r.min(rect.width().min(rect.height()) / 2.0);
                }
            }
            Drag::Corner { object, corner }
        }
        Drag::Perspective { object, corner } => {
            let p = snapper(state, view, &[object]).point(p);
            // A corner that would fold the shape stays where it was.
            state.doc.set_perspective_corner(object, corner, p);
            Drag::Perspective { object, corner }
        }
        Drag::Control { object, control, shape, transform } => {
            let local = transform.inverse() * snapper(state, view, &[object]).point(p);
            if let Some((edited, turn)) = controls::drag(&shape, control, local, constrain(mods))
                && let Some(obj) = state.doc.active_page_mut().object_mut(object)
            {
                obj.shape = edited;
                obj.transform = transform * turn;
            }
            Drag::Control { object, control, shape, transform }
        }
        Drag::FillVector { object, start } => {
            if view.screen_dist(start, p) > DRAG_START {
                let end = snapper(state, view, &[]).point(p);
                crate::paint_tools::drag_fountain(state, object, start, end);
            }
            Drag::FillVector { object, start }
        }
        Drag::FillHandle { object, end } => {
            let at = snapper(state, view, &[]).point(p);
            crate::paint_tools::move_fountain_handle(state, object, end, at);
            Drag::FillHandle { object, end }
        }
        Drag::Guide { index } => {
            let mut options = state.snap_options();
            options.guides = false;
            let snap = vs_core::Snapper::new(state.doc.active_page(), &options, &[], view.doc_len(SNAP)).point(p);
            if let Some(g) = state.doc.active_page_mut().guides.get_mut(index) {
                g.position = match g.axis {
                    vs_core::GuideAxis::Horizontal => snap.y,
                    vs_core::GuideAxis::Vertical => snap.x,
                };
            }
            Drag::Guide { index }
        }
        Drag::TableCells { again } => Drag::TableCells { again: !crate::table_tool::drag_to(state, p) && again },
        other => other,
    };
    state.tools.drag = Some(drag);
}

/// Corners and edge midpoints of `bounds`, in handle order.
fn edge_points(bounds: kurbo::Rect) -> [Point; 8] {
    let c = bounds.center();
    [
        Point::new(bounds.x0, bounds.y0),
        Point::new(c.x, bounds.y0),
        Point::new(bounds.x1, bounds.y0),
        Point::new(bounds.x1, c.y),
        Point::new(bounds.x1, bounds.y1),
        Point::new(c.x, bounds.y1),
        Point::new(bounds.x0, bounds.y1),
        Point::new(bounds.x0, c.y),
    ]
}

fn scale_transform(handle: usize, bounds: kurbo::Rect, p: Point, mods: egui::Modifiers) -> Affine {
    let c = bounds.center();
    let edge = edge_points(bounds);
    let h = edge[handle];
    let anchor = if mods.shift { c } else { edge[(handle + 4) % 8] };
    let ratio = |num: f64, den: f64| if den.abs() < 1e-9 { 1.0 } else { num / den };
    let mut sx = ratio(p.x - anchor.x, h.x - anchor.x);
    let mut sy = ratio(p.y - anchor.y, h.y - anchor.y);
    match handle {
        1 | 5 => sx = 1.0,
        3 | 7 => sy = 1.0,
        // Corners keep proportions unless Cmd/Ctrl is held.
        _ if !constrain(mods) => {
            let d = h - anchor;
            let s = if d.hypot2() < 1e-18 { 1.0 } else { (p - anchor).dot(d) / d.hypot2() };
            sx = s;
            sy = s;
        }
        _ => {}
    }
    // Never collapse to zero: the inverse would not exist.
    let guard = |s: f64| if s.abs() < 1e-4 { 1e-4f64.copysign(s) } else { s };
    Affine::translate(anchor.to_vec2()) * Affine::scale_non_uniform(guard(sx), guard(sy)) * Affine::translate(-anchor.to_vec2())
}

/// Rectangle for the shape tools: Cmd/Ctrl = square, Shift = from the centre.
fn creation_rect(start: Point, p: Point, mods: egui::Modifiers) -> kurbo::Rect {
    let mut d = p - start;
    if constrain(mods) {
        let m = d.x.abs().max(d.y.abs());
        d = Vec2::new(m.copysign(d.x), m.copysign(d.y));
    }
    if mods.shift {
        kurbo::Rect::from_points(start - d, start + d)
    } else {
        kurbo::Rect::from_points(start, start + d)
    }
}

/// The paragraph text frames among `originals`, in the same order.
fn text_frames(state: &AppState, originals: &Originals) -> Vec<(ObjectId, kurbo::Size)> {
    let page = state.doc.active_page();
    originals
        .iter()
        .filter_map(|(id, _)| match &page.object(*id)?.shape {
            Shape::Text(t) if t.on_path.is_none() => t.frame.map(|f| (*id, f)),
            _ => None,
        })
        .collect()
}

/// Remember `t`, the transform a finished drag made, for Repeat.
fn remember_transform(state: &mut AppState, originals: &Originals, t: Affine) {
    let page = state.doc.active_page();
    let before = originals
        .iter()
        .filter_map(|(id, o)| page.object(*id).map(|obj| vs_core::Object { transform: *o, ..obj.clone() }.world_bounds()))
        .reduce(|a, b| a.union(b));
    if let Some(before) = before {
        state.remember_transform(t, before.center());
    }
}

/// Remember the transform a finished move, scale or rotate drag did, for Repeat.
fn remember_drag(state: &mut AppState, originals: &Originals) {
    let page = state.doc.active_page();
    let Some((t, before)) = originals.first().and_then(|(id, original)| {
        let now = page.object(*id)?.transform;
        let before = originals
            .iter()
            .filter_map(|(id, o)| page.object(*id).map(|obj| vs_core::Object { transform: *o, ..obj.clone() }.world_bounds()))
            .reduce(|a, b| a.union(b))?;
        Some((now * original.inverse(), before))
    }) else {
        return;
    };
    state.remember_transform(t, before.center());
}

fn creation_shape(tool: Tool, rect: kurbo::Rect, settings: &ToolSettings) -> Option<Shape> {
    Some(match tool {
        Tool::Rectangle => Shape::Rectangle { rect, corner_radius: 0.0 },
        Tool::Ellipse => Shape::Ellipse { rect, arc: None },
        Tool::Polygon => Shape::Polygon { rect, sides: settings.polygon_sides },
        Tool::Star => Shape::Star { rect, points: settings.star_points, inner_ratio: settings.star_inner_ratio },
        Tool::CommonShapes => Shape::Curve { path: vs_core::construct::common_shape(settings.common_shape, rect) },
        _ => return None,
    })
}

fn release(state: &mut AppState, view: &View, p: Point, mods: egui::Modifiers) {
    let Some(drag) = state.tools.drag.take() else { return };
    match drag {
        Drag::Marquee { start, add } => {
            let rect = kurbo::Rect::from_points(start, p);
            if view.screen_dist(start, p) <= DRAG_START {
                if !add {
                    state.selection.clear();
                }
            } else {
                let inside = state.doc.active_page().objects_inside(rect);
                if !add {
                    state.selection.clear();
                }
                for id in inside {
                    if !state.selection.contains(&id) {
                        state.selection.push(id);
                    }
                }
                state.marquee_selection = Some(state.selection.clone());
            }
            state.tools.pick_mode = PickMode::Scale;
        }
        Drag::Lasso { mut points, add } => {
            points.push(p);
            if !add {
                state.selection.clear();
            }
            let travelled = points.iter().any(|q| view.screen_dist(points[0], *q) > DRAG_START);
            if travelled {
                for id in state.doc.active_page().objects_in_lasso(&points) {
                    if !state.selection.contains(&id) {
                        state.selection.push(id);
                    }
                }
                state.marquee_selection = Some(state.selection.clone());
            }
            state.tools.pick_mode = PickMode::Scale;
        }
        Drag::NodeMarquee { start, add } => {
            let rect = kurbo::Rect::from_points(start, p);
            if !add {
                state.tools.nodes.clear();
            }
            if let Some(id) = shape_target(state)
                && let Some(obj) = state.doc.active_page().object(id)
                && let Shape::Curve { path } = &obj.shape
            {
                for n in nodes::nodes(path) {
                    if rect.contains(obj.transform * n.point) && !state.tools.nodes.contains(&n.elem) {
                        state.tools.nodes.push(n.elem);
                    }
                }
            }
        }
        // Dropped on a PowerClip frame made with Create Empty Frame: inside it.
        Drag::Move { moved: true, originals, .. } => {
            let ids: Vec<ObjectId> = originals.iter().map(|o| o.0).collect();
            match state.doc.active_page().frame_at(p, &ids) {
                Some(frame) if state.doc.place_inside(&ids, frame) => {
                    state.selection = vec![frame];
                    state.marquee_selection = None;
                }
                _ => remember_drag(state, &originals),
            }
        }
        Drag::Scale { originals, last, .. } => remember_transform(state, &originals, last),
        Drag::Rotate { originals, .. } => remember_drag(state, &originals),
        Drag::Free { start: Some(_), originals, .. } => remember_drag(state, &originals),
        // A click on empty space clears the selection.
        Drag::Free { anchor, start: None, .. } => {
            if state.doc.active_page().hit_test(anchor, view.doc_len(HIT)).is_none() {
                state.selection.clear();
            }
        }
        Drag::Move { moved: false, was_selected: true, .. } => {
            state.tools.pick_mode = match state.tools.pick_mode {
                PickMode::Scale => PickMode::Rotate,
                PickMode::Rotate => PickMode::Scale,
            };
        }
        Drag::Create { start } => {
            let rect = creation_rect(start, p, mods);
            if rect.width() * view.zoom >= 2.0
                && rect.height() * view.zoom >= 2.0
                && let Some(shape) = creation_shape(state.tool, rect, &state.tools.settings)
            {
                let id = state.doc.add_object(shape, Style::new_object());
                state.selection = vec![id];
            }
        }
        Drag::Freehand { mut points } => {
            points.push(p);
            if points.len() >= 2 {
                let closed = points.len() > 2 && view.screen_dist(points[0], p) <= HIT;
                let mut line = BezPath::new();
                line.move_to(points[0]);
                for q in &points[1..] {
                    line.line_to(*q);
                }
                let options = kurbo::simplify::SimplifyOptions::default();
                let mut path = kurbo::simplify::simplify_bezpath(line, view.doc_len(1.0), &options);
                if closed {
                    path.close_path();
                }
                if path.bounding_box().size().max_side() * view.zoom >= 2.0 {
                    let id = state.doc.add_object(Shape::Curve { path }, Style::new_object());
                    state.selection = vec![id];
                }
            }
        }
        Drag::Line { start } => {
            if view.screen_dist(start, p) > DRAG_START {
                let end = if constrain(mods) { start + snap_angle(p - start) } else { p };
                let mut path = BezPath::new();
                path.move_to(start);
                path.line_to(end);
                let id = state.doc.add_object(Shape::Curve { path }, Style::new_object());
                state.selection = vec![id];
            }
        }
        Drag::FirstEdge { start } => {
            if view.screen_dist(start, p) > DRAG_START {
                state.tools.first_edge = Some((start, p));
            }
        }
        Drag::Spiral { start } => {
            let rect = creation_rect(start, p, mods);
            if rect.width() * view.zoom >= 2.0 && rect.height() * view.zoom >= 2.0 {
                let path = vs_core::construct::spiral(rect, state.tools.settings.spiral_turns);
                let id = state.doc.add_object(Shape::Curve { path }, Style::new_object());
                state.selection = vec![id];
            }
        }
        Drag::GraphPaper { start } => {
            let rect = creation_rect(start, p, mods);
            if rect.width() * view.zoom >= 2.0 && rect.height() * view.zoom >= 2.0 {
                let s = &state.tools.settings;
                let cells = vs_core::construct::graph_paper(rect, s.graph_columns, s.graph_rows);
                let ids: Vec<ObjectId> = cells
                    .into_iter()
                    .map(|rect| state.doc.add_object(Shape::Rectangle { rect, corner_radius: 0.0 }, Style::new_object()))
                    .collect();
                state.selection = match state.doc.group_objects(&ids) {
                    Some(group) => vec![group],
                    None => ids,
                };
            }
        }
        Drag::Table { start } => {
            let rect = creation_rect(start, p, mods);
            if rect.width() * view.zoom >= 2.0 && rect.height() * view.zoom >= 2.0 {
                let s = &state.tools.settings;
                let (rows, columns, font) = (s.table_rows as usize, s.table_columns as usize, s.text.clone());
                let id = state.doc.create_table(rect, rows, columns, &font);
                state.selection = vec![id];
                state.marquee_selection = None;
                state.table.cells = None;
            }
        }
        Drag::TableCells { again } => crate::table_tool::release(state, again),
        Drag::TextFrame { start } => {
            let frame = kurbo::Rect::from_points(start, p);
            if frame.width() * view.zoom > DRAG_START && frame.height() * view.zoom > DRAG_START {
                crate::text_tool::start_paragraph(state, frame);
            } else {
                crate::text_tool::press(state, start, view.doc_len(HIT));
            }
        }
        Drag::Crop { start } => {
            let area = kurbo::Rect::from_points(start, p);
            if area.width() * view.zoom >= 2.0 && area.height() * view.zoom >= 2.0 {
                state.tools.crop = Some(area);
            }
        }
        Drag::Knife { start } => {
            if view.screen_dist(start, p) > DRAG_START {
                let pieces = state.doc.knife(start, p);
                if !pieces.is_empty() {
                    state.selection = pieces;
                    state.marquee_selection = None;
                }
            }
        }
        Drag::Brush { .. } | Drag::Warp | Drag::Smudge { .. } | Drag::Smear { .. } => {
            let ids = state.selection.clone();
            state.doc.refit_brushed(&ids, state.tools.settings.brush_size / 2.0);
        }
        Drag::Roughen { .. } => {}
        Drag::Erase { mut points } => {
            points.push(p);
            let ids: Vec<ObjectId> = state.selection.clone();
            state.doc.erase(&ids, &points, state.tools.settings.eraser_size);
        }
        Drag::PenHandle { .. } => {}
        Drag::Guide { index } => {
            // Dropped back on a ruler or outside the drawing window: the guideline goes.
            if !view.canvas.contains(view.screen(p)) {
                let guides = &mut state.doc.active_page_mut().guides;
                if index < guides.len() {
                    guides.remove(index);
                }
                state.guide_selection = None;
            }
        }
        _ => {}
    }
}

/// Select guideline `index` of the active page; with `add` (Shift), add it to the
/// selected guidelines or take it out. Selecting a guideline deselects objects.
fn select_guide(state: &mut AppState, index: usize, add: bool) {
    let page = state.doc.active_page().id;
    let mut indices = if add { state.selected_guides() } else { Vec::new() };
    if let Some(i) = indices.iter().position(|g| *g == index) {
        indices.remove(i);
    } else {
        indices.push(index);
    }
    state.guide_selection = Some((page, indices));
    state.selection.clear();
    state.marquee_selection = None;
}

/// The guideline within reach of `p`, topmost (last added) first.
fn guide_at(state: &AppState, view: &View, p: Point) -> Option<usize> {
    if !state.prefs.show_guidelines {
        return None;
    }
    let reach = view.doc_len(HIT);
    state.doc.active_page().guides.iter().rposition(|g| match g.axis {
        vs_core::GuideAxis::Horizontal => (g.position - p.y).abs() <= reach,
        vs_core::GuideAxis::Vertical => (g.position - p.x).abs() <= reach,
    })
}

/// Start dragging a new guideline out of a ruler.
pub fn start_guide(state: &mut AppState, axis: vs_core::GuideAxis, at: Point) {
    let position = match axis {
        vs_core::GuideAxis::Horizontal => at.y,
        vs_core::GuideAxis::Vertical => at.x,
    };
    let guides = &mut state.doc.active_page_mut().guides;
    guides.push(vs_core::Guide { axis, position });
    state.tools.drag = Some(Drag::Guide { index: guides.len() - 1 });
}

// ---------------------------------------------------------------------------------------
// Keyboard

/// Delete the selected nodes in the Shape tool. Returns false if there were none.
pub fn delete_selected_nodes(state: &mut AppState) -> bool {
    if state.tool != Tool::Shape || state.tools.nodes.is_empty() {
        return false;
    }
    let Some(id) = shape_target(state) else { return false };
    let Some(Shape::Curve { path }) = state.doc.active_page().object(id).map(|o| o.shape.clone()) else {
        return false;
    };
    let mut path = path;
    let mut elems = state.tools.nodes.clone();
    elems.sort_unstable_by(|a, b| b.cmp(a));
    for e in elems {
        nodes::delete_node(&mut path, e);
    }
    set_curve(state, id, path);
    state.tools.nodes.clear();
    true
}

/// Crop to the pending crop area: the selected objects, or everything on the page.
pub fn apply_crop(state: &mut AppState) {
    let Some(area) = state.tools.crop.take() else { return };
    let ids = state.selection.clone();
    let kept = state.doc.crop(&ids, area);
    state.selection = if ids.is_empty() { Vec::new() } else { kept };
    state.marquee_selection = None;
}

/// The crop area: dimmed outside, a frame around it.
fn crop_frame(painter: &egui::Painter, view: &View, area: kurbo::Rect) {
    let r = Rect::from_two_pos(view.screen(Point::new(area.x0, area.y0)), view.screen(Point::new(area.x1, area.y1)));
    let shade = Color32::from_black_alpha(0x50);
    let c = view.canvas;
    for band in [
        Rect::from_min_max(c.min, egui::pos2(c.max.x, r.min.y)),
        Rect::from_min_max(egui::pos2(c.min.x, r.max.y), c.max),
        Rect::from_min_max(egui::pos2(c.min.x, r.min.y), egui::pos2(r.min.x, r.max.y)),
        Rect::from_min_max(egui::pos2(r.max.x, r.min.y), egui::pos2(c.max.x, r.max.y)),
    ] {
        if band.is_positive() {
            painter.rect_filled(band, 0.0, shade);
        }
    }
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
}

pub fn cancel(state: &mut AppState) {
    state.tools.drag = None;
    state.tools.first_edge = None;
    state.tools.arm = None;
}

/// `d` turned to the nearest multiple of 15°, same length.
fn snap_angle(d: Vec2) -> Vec2 {
    let step = 15f64.to_radians();
    let angle = (d.atan2() / step).round() * step;
    Vec2::from_angle(angle) * d.hypot()
}

// ---------------------------------------------------------------------------------------
// Overlay

fn path_to_screen(painter: &egui::Painter, path: &BezPath, view: &View, stroke: Stroke) {
    let mut world = path.clone();
    world.apply_affine(Affine::translate((view.origin.x as f64, view.origin.y as f64)) * view.to_screen);
    let mut line: Vec<Pos2> = Vec::new();
    let flush = |line: &mut Vec<Pos2>, closed: bool| {
        if line.len() >= 2 {
            let pts = std::mem::take(line);
            painter.add(if closed { egui::Shape::closed_line(pts, stroke) } else { egui::Shape::line(pts, stroke) });
        }
        line.clear();
    };
    kurbo::flatten(world, 0.25, |el| match el {
        PathEl::MoveTo(p) => {
            flush(&mut line, false);
            line.push(Pos2::new(p.x as f32, p.y as f32));
        }
        PathEl::LineTo(p) => line.push(Pos2::new(p.x as f32, p.y as f32)),
        PathEl::ClosePath => flush(&mut line, true),
        _ => {}
    });
    flush(&mut line, false);
}

fn square(painter: &egui::Painter, at: Pos2, filled: bool) {
    let r = Rect::from_center_size(at, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
    painter.rect_filled(r, 0.0, if filled { ACCENT } else { Color32::WHITE });
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, if filled { Color32::WHITE } else { Color32::BLACK }), egui::StrokeKind::Inside);
}

fn handle_line(painter: &egui::Painter, from: Pos2, to: Pos2) {
    painter.line_segment([from, to], Stroke::new(1.0, ACCENT));
    painter.circle(to, 3.5, ACCENT, Stroke::new(1.0, Color32::WHITE));
}

/// Alignment and dynamic guides while drawing or moving.
pub const GUIDE_LINE: Color32 = Color32::from_rgb(0x00, 0xA6, 0x51);

pub fn draw_overlay(painter: &egui::Painter, state: &AppState, canvas: Rect) {
    let view = View::new(state, canvas);
    let page = state.doc.active_page();
    let thin = Stroke::new(1.0, ACCENT);
    crate::text_tool::draw_frames(painter, state, |p| view.screen(p));
    crate::text_tool::draw_nonprinting(painter, state, |p| view.screen(p));
    crate::text_tool::draw_caret(painter, state, |p| view.screen(p));
    crate::table_tool::draw_picked(painter, state, |p| view.screen(p));
    for line in &state.tools.guide_lines {
        let points = [view.screen(line.from), view.screen(line.to)];
        painter.extend(egui::Shape::dashed_line(&points, Stroke::new(1.0, GUIDE_LINE), 4.0, 3.0));
    }
    // An empty PowerClip frame shows an X across it.
    for obj in page.objects.iter().filter(|o| o.frame && o.contents.is_empty()) {
        let b = obj.world_bounds();
        let s = Stroke::new(1.0, Color32::from_gray(0x90));
        painter.line_segment([view.screen(Point::new(b.x0, b.y0)), view.screen(Point::new(b.x1, b.y1))], s);
        painter.line_segment([view.screen(Point::new(b.x1, b.y0)), view.screen(Point::new(b.x0, b.y1))], s);
    }
    if state.tool == Tool::InteractiveFill
        && let Some((_, g, t)) = crate::paint_tools::selected_gradient(state)
    {
        let (a, b) = (view.screen(t * g.start), view.screen(t * g.end));
        painter.line_segment([a, b], Stroke::new(1.0, ACCENT));
        square(painter, a, false);
        square(painter, b, true);
    }

    if state.tool == Tool::Shape {
        if let Some(obj) = shape_target(state).and_then(|id| page.object(id)) {
            path_to_screen(painter, &obj.world_path(), &view, thin);
            let t = obj.transform;
            if let Some(corners) = obj.perspective_corners() {
                perspective_grid(painter, obj, &corners, &view);
            } else {
            match &obj.shape {
                Shape::Curve { path } => {
                    for n in nodes::nodes(path) {
                        let at = view.screen(t * n.point);
                        let selected = state.tools.nodes.contains(&n.elem);
                        if selected {
                            for (_, h) in [n.handle_in, n.handle_out].into_iter().flatten() {
                                handle_line(painter, at, view.screen(t * h));
                            }
                        }
                        square(painter, at, selected);
                    }
                }
                Shape::Rectangle { rect, .. } => {
                    for c in [rect.origin(), Point::new(rect.x1, rect.y0), Point::new(rect.x1, rect.y1), Point::new(rect.x0, rect.y1)] {
                        square(painter, view.screen(t * c), false);
                    }
                }
                shape => {
                    for (_, at) in controls::controls(shape) {
                        square(painter, view.screen(t * at), false);
                    }
                }
            }
            }
        }
    } else if let Some(bounds) = page.bounds_of(&state.selection) {
        for id in &state.selection {
            if let Some(obj) = page.object(*id)
                && state.selection.len() > 1
            {
                let b = obj.world_bounds();
                painter.rect_stroke(
                    Rect::from_two_pos(view.screen(b.origin()), view.screen(Point::new(b.x1, b.y1))),
                    0.0,
                    Stroke::new(1.0, ACCENT.gamma_multiply(0.4)),
                    egui::StrokeKind::Middle,
                );
            }
        }
        let handles = handle_points(bounds, &view);
        let rotate = matches!(state.tool, Tool::Pick | Tool::FreehandPick) && state.tools.pick_mode == PickMode::Rotate;
        let all_locked = state.selection.iter().all(|id| page.object(*id).is_some_and(|o| o.locked));
        for (i, h) in handles.iter().enumerate() {
            let at = view.screen(*h);
            if all_locked {
                // Locked: small padlocks instead of handles.
                let icon = egui_material_icons::icons::ICON_LOCK;
                painter.text(at, egui::Align2::CENTER_CENTER, <&str>::from(icon), egui::FontId::new(11.0, icon.font_family()), Color32::BLACK);
            } else if rotate {
                if is_corner(i) {
                    painter.circle(at, 4.5, Color32::WHITE, Stroke::new(1.5, Color32::BLACK));
                }
            } else {
                painter.rect_filled(Rect::from_center_size(at, egui::vec2(HANDLE_SIZE - 1.0, HANDLE_SIZE - 1.0)), 0.0, Color32::BLACK);
            }
        }
        let c = view.screen(bounds.center());
        if rotate {
            painter.circle_stroke(c, 5.0, Stroke::new(1.5, Color32::BLACK));
            painter.circle_filled(c, 1.5, Color32::BLACK);
        } else {
            let s = Stroke::new(1.0, Color32::BLACK);
            painter.line_segment([c - egui::vec2(3.0, 3.0), c + egui::vec2(3.0, 3.0)], s);
            painter.line_segment([c - egui::vec2(3.0, -3.0), c + egui::vec2(3.0, -3.0)], s);
        }
    }

    match &state.tools.drag {
        Some(Drag::Marquee { start, .. } | Drag::NodeMarquee { start, .. }) => {
            if let Some(p) = state.tools.pointer {
                let r = Rect::from_two_pos(view.screen(*start), view.screen(p));
                painter.rect_filled(r, 0.0, ACCENT.gamma_multiply(0.08));
                painter.rect_stroke(r, 0.0, thin, egui::StrokeKind::Inside);
            }
        }
        Some(Drag::Create { start }) => {
            if let Some(p) = state.tools.pointer {
                let mods = painter.ctx().input(|i| i.modifiers);
                let rect = creation_rect(*start, p, mods);
                if let Some(shape) = creation_shape(state.tool, rect, &state.tools.settings) {
                    path_to_screen(painter, &shape.to_path(), &view, thin);
                }
            }
        }
        Some(Drag::Freehand { points }) => {
            let pts: Vec<Pos2> = points.iter().map(|p| view.screen(*p)).collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.0, Color32::BLACK)));
        }
        Some(Drag::Free { anchor, .. }) => {
            let a = view.screen(*anchor);
            painter.circle_stroke(a, 4.0, Stroke::new(1.5, ACCENT));
            if let Some(p) = state.tools.pointer {
                painter.line_segment([a, view.screen(p)], thin);
            }
        }
        Some(Drag::Lasso { points, .. }) => {
            let pts: Vec<Pos2> = points.iter().map(|p| view.screen(*p)).collect();
            painter.add(egui::Shape::closed_line(pts, thin));
        }
        Some(Drag::Crop { start }) => {
            if let Some(p) = state.tools.pointer {
                crop_frame(painter, &view, kurbo::Rect::from_points(*start, p));
            }
        }
        Some(Drag::Line { start } | Drag::FirstEdge { start }) => {
            if let Some(p) = state.tools.pointer {
                painter.line_segment([view.screen(*start), view.screen(p)], thin);
            }
        }
        Some(Drag::Spiral { start }) => {
            if let Some(p) = state.tools.pointer {
                let mods = painter.ctx().input(|i| i.modifiers);
                let path = vs_core::construct::spiral(creation_rect(*start, p, mods), state.tools.settings.spiral_turns);
                path_to_screen(painter, &path, &view, thin);
            }
        }
        Some(Drag::GraphPaper { start }) => {
            if let Some(p) = state.tools.pointer {
                let mods = painter.ctx().input(|i| i.modifiers);
                let s = &state.tools.settings;
                for cell in vs_core::construct::graph_paper(creation_rect(*start, p, mods), s.graph_columns, s.graph_rows) {
                    painter.rect_stroke(Rect::from_two_pos(view.screen(cell.origin()), view.screen(Point::new(cell.x1, cell.y1))), 0.0, thin, egui::StrokeKind::Middle);
                }
            }
        }
        Some(Drag::Table { start }) => {
            if let Some(p) = state.tools.pointer {
                let mods = painter.ctx().input(|i| i.modifiers);
                let s = &state.tools.settings;
                for cell in vs_core::construct::graph_paper(creation_rect(*start, p, mods), s.table_columns, s.table_rows) {
                    painter.rect_stroke(Rect::from_two_pos(view.screen(cell.origin()), view.screen(Point::new(cell.x1, cell.y1))), 0.0, thin, egui::StrokeKind::Middle);
                }
            }
        }
        Some(Drag::TextFrame { start }) => {
            if let Some(p) = state.tools.pointer {
                let r = Rect::from_two_pos(view.screen(*start), view.screen(p));
                painter.rect_stroke(r, 0.0, thin, egui::StrokeKind::Inside);
            }
        }
        Some(Drag::Knife { start }) => {
            if let Some(p) = state.tools.pointer {
                painter.line_segment([view.screen(*start), view.screen(p)], Stroke::new(1.5, Color32::from_rgb(0xD0, 0x20, 0x20)));
            }
        }
        _ if is_brush(state.tool) => {
            if let Some(p) = state.tools.pointer {
                let r = (state.tools.settings.brush_size / 2.0 * view.zoom) as f32;
                painter.circle_stroke(view.screen(p), r, thin);
            }
        }
        Some(Drag::Erase { points }) => {
            let width = (state.tools.settings.eraser_size * view.zoom) as f32;
            let pts: Vec<Pos2> = points.iter().map(|p| view.screen(*p)).collect();
            let wash = Color32::from_rgba_unmultiplied(0x1E, 0x6F, 0xD9, 0x50);
            if let [only] = pts.as_slice() {
                painter.circle_filled(*only, width / 2.0, wash);
            } else {
                painter.add(egui::Shape::line(pts, Stroke::new(width, wash)));
            }
        }
        _ => {}
    }
    if state.tool == Tool::Crop
        && let Some(area) = state.tools.crop
    {
        crop_frame(painter, &view, area);
    }

    if state.tool == Tool::SegmentDimension
        && state.tools.first_edge.is_none()
        && let Some(p) = state.tools.pointer
        && let Some((a, b)) = vs_core::dimension::segment_at(state.doc.active_page(), p, view.doc_len(HIT))
    {
        painter.line_segment([view.screen(a), view.screen(b)], Stroke::new(2.0, ACCENT));
    }
    if let (Some((a, b)), Some(c)) = (state.tools.first_edge, state.tools.pointer) {
        painter.line_segment([view.screen(a), view.screen(b)], thin);
        if state.tool == Tool::AngularDimension {
            match state.tools.arm {
                Some(arm) => {
                    if let Some(d) = vs_core::dimension::angular_dimension(a, b, arm, c) {
                        path_to_screen(painter, &d.lines, &view, thin);
                        path_to_screen(painter, &d.arrows, &view, thin);
                        let label = crate::dimensions::degrees(d.angle);
                        painter.text(view.screen(d.label_at), egui::Align2::CENTER_BOTTOM, label, egui::FontId::proportional(12.0), Color32::BLACK);
                    }
                }
                None => {
                    painter.line_segment([view.screen(a), view.screen(c)], thin);
                }
            }
        } else if state.tool == Tool::Callout {
            painter.line_segment([view.screen(b), view.screen(c)], thin);
        } else if let Some((shape, t)) = three_point_shape(state.tool, a, b, c) {
            let mut path = shape.to_path();
            path.apply_affine(t);
            path_to_screen(painter, &path, &view, thin);
        } else if let Some(d) = crate::dimensions::kind(state.tool).and_then(|k| vs_core::dimension::dimension(k, a, b, c)) {
            path_to_screen(painter, &d.lines, &view, thin);
            path_to_screen(painter, &d.arrows, &view, thin);
            let at = view.screen(d.label_at);
            painter.text(at, egui::Align2::CENTER_BOTTOM, crate::dimensions::label(state, d.length), egui::FontId::proportional(12.0), Color32::BLACK);
        }
    }

    if let Some(pen) = &state.tools.pen {
        let line = if pen.kind == Tool::BSpline { thin } else { Stroke::new(1.0, Color32::BLACK) };
        path_to_screen(painter, &pen.path, &view, line);
        let els = pen.path.elements();
        let last = els.last().and_then(|e| e.end_point());
        if pen.kind == Tool::BSpline
            && let Some(cursor) = state.tools.pointer
        {
            // The spline the points make so far, with the pointer as the next one.
            let mut nodes: Vec<Point> = els.iter().filter_map(|e| match e {
                PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
                _ => None,
            }).collect();
            nodes.push(cursor);
            path_to_screen(painter, &vs_core::construct::bspline(&nodes, false), &view, Stroke::new(1.5, ACCENT));
        }
        if let (Some(last), Some(cursor)) = (last, state.tools.pointer)
            && matches!(pen.kind, Tool::Pen | Tool::Polyline | Tool::BSpline)
            && state.tools.drag.is_none()
        {
            let mut preview = BezPath::new();
            preview.move_to(last);
            match pen.out_handle {
                Some(h) => preview.curve_to(h, cursor, cursor),
                None => preview.line_to(cursor),
            }
            path_to_screen(painter, &preview, &view, thin);
        }
        for el in els {
            if let Some(p) = el.end_point() {
                square(painter, view.screen(p), false);
            }
        }
        if let (Some(last), Some(h)) = (last, pen.out_handle) {
            let at = view.screen(last);
            handle_line(painter, at, view.screen(h));
            handle_line(painter, at, view.screen(last + (last - h)));
        }
    }
}

/// Perspective grid: the object's bounds cut in four each way, seen through its
/// perspective, with a handle on each corner.
fn perspective_grid(painter: &egui::Painter, obj: &vs_core::Object, corners: &[Point; 4], view: &View) {
    const PERSPECTIVE: Color32 = Color32::from_gray(0x30);
    let Some(h) = obj.perspective else { return };
    let f = obj.perspective_frame();
    let at = |u: f64, v: f64| view.screen(obj.transform * h.apply(Point::new(f.x0 + u * f.width(), f.y0 + v * f.height())));
    for k in 0..=4 {
        let s = f64::from(k) / 4.0;
        for line in [[at(s, 0.0), at(s, 1.0)], [at(0.0, s), at(1.0, s)]] {
            painter.extend(egui::Shape::dashed_line(&line, Stroke::new(1.0, PERSPECTIVE), 3.0, 3.0));
        }
    }
    for c in corners {
        square(painter, view.screen(*c), false);
    }
}

/// One-line hint for the status bar.
pub fn hint(state: &AppState) -> String {
    if state.copy_properties.picking {
        return "Click the object to copy properties from · Esc to cancel".into();
    }
    if state.powerclip.picking {
        return "Click the shape to place the selection inside · Esc to cancel".into();
    }
    if state.copy_perspective {
        return "Click the object to copy the perspective from · Esc to cancel".into();
    }
    if state.powerclip.editing.is_some() {
        return "Editing PowerClip contents · Finish editing PowerClip in the property bar puts them back".into();
    }
    if let Some(edit) = &state.group_edit {
        let what = match edit.kind {
            crate::symbol_menu::GroupEditKind::Symmetry => "symmetry: the mirror follows each change",
            crate::symbol_menu::GroupEditKind::Symbol => "symbol: every instance changes on Finish",
        };
        return format!("Editing {what} · {} in the property bar puts the objects back", edit.finish_label());
    }
    let n = state.selection.len();
    let selected = match n {
        0 => "Nothing selected".to_string(),
        1 => state
            .doc
            .active_page()
            .object(state.selection[0])
            .map(|o| format!("{} selected", o.label()))
            .unwrap_or_default(),
        n => format!("{n} objects selected"),
    };
    match state.tool {
        Tool::Pick if n > 0 && state.tools.pick_mode == PickMode::Rotate => {
            format!("{selected} · drag a corner to rotate (Cmd for 15° steps) · click the object to scale again")
        }
        Tool::Pick if n > 0 => {
            format!("{selected} · click again to rotate · double-click to edit nodes · Shift-click to add or remove")
        }
        Tool::Pick => "Click an object to select it, or drag a box around objects".into(),
        Tool::FreehandPick if n > 0 => format!("{selected} · drag a loop around objects to select them · Shift adds"),
        Tool::FreehandPick => "Drag a loop around objects to select them, or click an object".into(),
        Tool::FreeTransform if n > 0 => {
            let what = match state.tools.free_mode {
                FreeMode::Rotate => "drag around the point you press to rotate (Cmd for 15° steps)",
                FreeMode::Reflect => "drag a line to reflect across (Cmd for 15° steps)",
                FreeMode::Scale => "drag away from or towards the point you press to scale (Cmd frees the proportions)",
                FreeMode::Skew => "drag across the point you press to skew",
            };
            format!("{selected} · {what}")
        }
        Tool::FreeTransform => "Click an object to transform it, or drag a box around objects".into(),
        Tool::Shape => match shape_target(state).and_then(|id| state.doc.active_page().object(id)) {
            Some(o) if o.perspective.is_some() => "Drag a corner to change the perspective · Object > Clear Effect removes it".into(),
            Some(o) if matches!(o.shape, Shape::Curve { .. }) => {
                "Drag nodes or handles · double-click a line to add a node, a node to delete it".into()
            }
            Some(o) if matches!(o.shape, Shape::Rectangle { .. }) => "Drag a corner to round it · Cmd+Q converts to curves".into(),
            Some(o) if matches!(o.shape, Shape::Ellipse { .. }) => {
                "Drag the node inside the ellipse for a pie, outside for an arc · Cmd for 15° steps · Cmd+Q converts to curves".into()
            }
            Some(o) if matches!(o.shape, Shape::Star { .. }) => {
                "Drag an outer node to resize and turn all points, an inner node to change the sharpness · Cmd keeps the angle".into()
            }
            Some(o) if matches!(o.shape, Shape::Polygon { .. }) => {
                "Drag a node to resize and turn all sides · Cmd keeps the angle · Cmd+Q converts to curves".into()
            }
            Some(_) => "Ungroup to edit the nodes of each object".into(),
            None => "Click an object to edit its nodes".into(),
        },
        Tool::CommonShapes => "Drag to draw the shape chosen on the property bar · Ctrl/Cmd keeps it in a square · Shift from the centre".into(),
        Tool::Rectangle | Tool::Ellipse | Tool::Polygon | Tool::Star => {
            "Drag to draw · Cmd constrains to equal sides · Shift draws from the centre".into()
        }
        Tool::Freehand => "Drag to draw · end on the start point to close".into(),
        Tool::Bezier | Tool::Pen => {
            "Click for corners, drag for curves · click the first node to close · Enter or click the last node to finish".into()
        }
        Tool::TwoPointLine => "Drag to draw a straight line · Ctrl/Cmd keeps it to 15° steps".into(),
        Tool::Polyline => "Click to add straight segments · double-click, Enter or the first node finishes".into(),
        Tool::BSpline => "Click to place control points · double-click or Enter finishes; clicking the first point closes it".into(),
        Tool::Spiral => "Drag to draw a spiral · turns are on the property bar".into(),
        Tool::Table if crate::table_tool::picked(state).is_some() => "Drag across cells or Shift+click to pick more · click the picked cell again to type in it · Table menu inserts, merges and splits".into(),
        Tool::Table => "Drag to draw a table · rows and columns are on the property bar · click a table to pick a cell".into(),
        Tool::GraphPaper => "Drag to draw a grid of rectangles · columns and rows are on the property bar · Ctrl/Cmd keeps the grid square".into(),
        Tool::AngularDimension if state.tools.arm.is_some() => "Click where the arc goes; past the arms measures the angle outside them · Esc cancels".into(),
        Tool::AngularDimension if state.tools.first_edge.is_some() => "Click a point on the second arm · Esc cancels".into(),
        Tool::SegmentDimension if state.tools.first_edge.is_some() => "Click where the dimension line goes · Esc cancels".into(),
        Tool::Callout if state.tools.first_edge.is_some() => "Click where the text goes · Esc cancels".into(),
        t if is_three_point(t) && state.tools.first_edge.is_some() => "Click to set the third point · Esc cancels".into(),
        Tool::ThreePointCurve => "Drag from start to end, then click to set the curve".into(),
        Tool::ThreePointRectangle => "Drag one side, then click to set the height".into(),
        Tool::ThreePointEllipse => "Drag one axis, then click to set the other".into(),
        Tool::Smooth => "Select a curve or shape, then drag along its outline to smooth it · brush size and smoothness are on the property bar".into(),
        Tool::RoughenBrush => "Select a curve or shape, then drag along its outline to add spikes · brush and spike size are on the property bar".into(),
        Tool::Smudge => "Select a curve or shape, then drag across its outline to carry it along · brush size is on the property bar".into(),
        Tool::SmearBrush => "Select a curve or shape, then drag from its outline to pull it out or push it in · brush size is on the property bar".into(),
        Tool::Twirl => "Select a curve or shape, then hold the brush over its outline to swirl it · size, speed and direction are on the property bar".into(),
        Tool::AttractRepel if state.tools.settings.repel => "Select a curve or shape, then hold the brush near its outline to push it away · size and speed are on the property bar".into(),
        Tool::AttractRepel => "Select a curve or shape, then hold the brush near its outline to pull it in · size and speed are on the property bar".into(),
        Tool::ParallelDimension => "Drag between the two points to measure, then click where the dimension line goes".into(),
        Tool::HorizontalVerticalDimension => "Drag between the two points, then click above or below for the horizontal distance, beside for the vertical".into(),
        Tool::AngularDimension => "Drag from the vertex along the first arm, click the second arm, then click where the arc goes".into(),
        Tool::SegmentDimension => "Click a segment of an object to measure it, then click where the dimension line goes".into(),
        Tool::Callout => "Drag from the point to the bend of the leader, click where the text goes, then type".into(),
        Tool::Text if state.tools.text.is_some() => "Type the text · Enter starts a new line · Esc or another tool finishes".into(),
        Tool::Text => "Click to add artistic text, drag a frame for paragraph text, or click text to edit it".into(),
        Tool::InteractiveFill => "Click an object, then drag across it for a fountain fill · drag the ends to move it".into(),
        Tool::Transparency => "Click an object, then set its transparency in the property bar".into(),
        Tool::ColorEyedropper if state.tools.eyedropper.applying => {
            "Click an object's fill or outline to apply the color · Select color picks another".into()
        }
        Tool::ColorEyedropper => "Click a color on the page to pick it".into(),
        Tool::SmartFill => "Click inside an area that outlines enclose to fill it as a new object · colour and outline are on the property bar".into(),
        Tool::AttributesEyedropper if state.tools.eyedropper.applying => "Click an object to give it the picked fill, outline and transparency".into(),
        Tool::AttributesEyedropper => "Click an object to pick its fill, outline and transparency".into(),
        Tool::Crop if state.tools.crop.is_some() => "Enter or double-click inside to crop · Esc clears the area · drag again to redraw it".into(),
        Tool::Crop if state.has_selection() => "Drag the area to keep; the selected objects are cropped".into(),
        Tool::Crop => "Drag the area to keep; everything on the page is cropped".into(),
        Tool::Knife => "Drag a line across objects to cut them in two".into(),
        Tool::VirtualSegmentDelete => "Click a line to delete it up to where other lines cross it".into(),
        Tool::Eraser if state.has_selection() => "Drag over the selected objects to erase · size in the property bar".into(),
        Tool::Eraser => "Drag over objects to erase · size in the property bar".into(),
        Tool::Zoom => "Click to zoom in; Shift-click to zoom out".into(),
        Tool::Pan => "Drag to move the view".into(),
        _ => "This tool is not available yet".into(),
    }
}

#[cfg(test)]
mod tests {
    //! Drives the tools with simulated pointer input through a real egui context.
    use super::*;
    use egui::{Event, PointerButton, RawInput, Sense, pos2, vec2};
    use vs_core::{Document, Rgba};

    use crate::app::{AppState, Prefs};
    use crate::toolbox::ToolboxState;

    struct Harness {
        ctx: egui::Context,
        state: AppState,
        time: f64,
        mods: egui::Modifiers,
    }

    impl Harness {
        fn new(tool: Tool) -> Self {
            let mut state = AppState::new(Document::new("Test"), ToolboxState::default(), Prefs::default(), Default::default());
            state.tool = tool;
            Self { ctx: egui::Context::default(), state, time: 0.0, mods: egui::Modifiers::NONE }
        }

        /// Screen position of a document point (default viewport: 96/72 zoom, no offset).
        fn at(&self, x: f64, y: f64) -> Pos2 {
            let s = self.state.viewport.doc_to_screen(Point::new(x, y));
            pos2(s.x as f32, s.y as f32)
        }

        fn frame(&mut self, mut events: Vec<Event>) {
            events.insert(0, Event::ModifiersChanged(self.mods));
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 1200.0))),
                events,
                time: Some(self.time),
                ..Default::default()
            };
            self.time += 1.0 / 60.0;
            let state = &mut self.state;
            let mut output = self.ctx.run_ui(input, |ui| {
                let rect = ui.max_rect();
                let resp = ui.interact(rect, ui.scope_id().with("canvas"), Sense::click_and_drag());
                tool_input(ui, state, &resp, rect);
            });
            // No renderer here: discard texture uploads explicitly.
            output.textures_delta.clear();
        }

        fn button(&self, p: Pos2, pressed: bool) -> Event {
            Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: self.mods }
        }

        fn press(&mut self, p: Pos2) {
            let b = self.button(p, true);
            self.frame(vec![Event::PointerMoved(p), b]);
        }

        fn release(&mut self, p: Pos2) {
            let b = self.button(p, false);
            self.frame(vec![Event::PointerMoved(p), b]);
        }

        fn drag(&mut self, from: Pos2, to: Pos2) {
            self.frame(vec![Event::PointerMoved(from)]);
            self.press(from);
            for i in 1..=8 {
                self.frame(vec![Event::PointerMoved(from.lerp(to, i as f32 / 8.0))]);
            }
            self.release(to);
            self.time += 1.0;
        }

        fn click(&mut self, p: Pos2) {
            self.frame(vec![Event::PointerMoved(p)]);
            self.press(p);
            self.release(p);
            self.time += 1.0;
        }

        fn double_click(&mut self, p: Pos2) {
            self.frame(vec![Event::PointerMoved(p)]);
            self.press(p);
            self.release(p);
            self.press(p);
            self.release(p);
            self.time += 1.0;
        }

        fn objects(&self) -> &[vs_core::Object] {
            &self.state.doc.active_page().objects
        }

        fn bounds(&self, i: usize) -> kurbo::Rect {
            self.objects()[i].world_bounds()
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    fn rect_eq(r: kurbo::Rect, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
        close(r.x0, x0) && close(r.y0, y0) && close(r.x1, x1) && close(r.y1, y1)
    }

    fn draw_rect(h: &mut Harness, x0: f64, y0: f64, x1: f64, y1: f64) {
        let tool = h.state.tool;
        h.state.tool = Tool::Rectangle;
        let (a, b) = (h.at(x0, y0), h.at(x1, y1));
        h.drag(a, b);
        h.state.tool = tool;
    }

    #[test]
    fn rectangle_tool_draws_and_selects() {
        let mut h = Harness::new(Tool::Rectangle);
        let (a, b) = (h.at(100.0, 100.0), h.at(200.0, 150.0));
        h.drag(a, b);
        assert_eq!(h.objects().len(), 1);
        assert!(rect_eq(h.bounds(0), 100.0, 100.0, 200.0, 150.0), "{:?}", h.bounds(0));
        assert_eq!(h.state.selection, vec![h.objects()[0].id]);
        assert_eq!(h.objects()[0].style, Style::new_object());
    }

    #[test]
    fn shape_tools_honour_constrain_and_centre() {
        let mut h = Harness::new(Tool::Ellipse);
        h.mods = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        let (a, b) = (h.at(300.0, 300.0), h.at(340.0, 310.0));
        h.drag(a, b);
        assert!(rect_eq(h.bounds(0), 260.0, 260.0, 340.0, 340.0), "{:?}", h.bounds(0));

        let mut h = Harness::new(Tool::Star);
        h.state.tools.settings.star_points = 7;
        let (a, b) = (h.at(10.0, 10.0), h.at(110.0, 110.0));
        h.drag(a, b);
        assert!(matches!(h.objects()[0].shape, Shape::Star { points: 7, .. }));
    }

    #[test]
    fn a_click_with_a_shape_tool_draws_nothing() {
        let mut h = Harness::new(Tool::Rectangle);
        let p = h.at(50.0, 50.0);
        h.click(p);
        assert!(h.objects().is_empty());
    }

    #[test]
    fn pick_moves_a_group_as_one_and_leaves_locked_objects() {
        let mut h = Harness::new(Tool::Pick);
        draw_rect(&mut h, 100.0, 100.0, 150.0, 150.0);
        draw_rect(&mut h, 200.0, 100.0, 250.0, 150.0);
        let ids = h.state.doc.all_object_ids();
        let group = h.state.doc.group_objects(&ids).unwrap();
        h.state.selection.clear();

        // Clicking one member selects the whole group; dragging moves both members.
        let (a, b) = (h.at(100.0, 125.0), h.at(110.0, 135.0));
        h.drag(a, b);
        assert_eq!(h.state.selection, vec![group]);
        assert!(rect_eq(h.bounds(0), 110.0, 110.0, 260.0, 160.0), "{:?}", h.bounds(0));

        h.state.doc.set_locked(&[group], true);
        let (a, b) = (h.at(110.0, 135.0), h.at(150.0, 175.0));
        h.drag(a, b);
        assert_eq!(h.state.selection, vec![group], "locked objects can still be selected");
        assert!(rect_eq(h.bounds(0), 110.0, 110.0, 260.0, 160.0), "locked group did not move");
    }

    #[test]
    fn pick_selects_moves_and_toggles_rotate_mode() {
        let mut h = Harness::new(Tool::Pick);
        draw_rect(&mut h, 100.0, 100.0, 200.0, 200.0);
        h.state.selection.clear();
        // Unfilled rectangle: the inside is not a hit, the outline is.
        let inside = h.at(150.0, 150.0);
        h.click(inside);
        assert!(h.state.selection.is_empty());
        let edge = h.at(100.0, 150.0);
        h.click(edge);
        assert_eq!(h.state.selection.len(), 1);
        assert_eq!(h.state.tools.pick_mode, PickMode::Scale);
        h.click(edge);
        assert_eq!(h.state.tools.pick_mode, PickMode::Rotate, "second click on the selection");

        let (from, to) = (h.at(100.0, 150.0), h.at(130.0, 170.0));
        h.drag(from, to);
        assert!(rect_eq(h.bounds(0), 130.0, 120.0, 230.0, 220.0), "{:?}", h.bounds(0));
    }

    #[test]
    fn pick_double_click_switches_to_the_shape_tool_on_that_object() {
        let mut h = Harness::new(Tool::Pick);
        draw_rect(&mut h, 100.0, 100.0, 200.0, 200.0);
        h.state.selection.clear();
        let empty = h.at(400.0, 400.0);
        h.double_click(empty);
        assert_eq!(h.state.tool, Tool::Pick, "empty page: no switch");

        let edge = h.at(100.0, 150.0);
        h.double_click(edge);
        assert_eq!(h.state.tool, Tool::Shape);
        assert_eq!(h.state.selection, vec![h.objects()[0].id]);
        assert_eq!(h.state.tools.pick_mode, PickMode::Scale, "the first click must not leave rotate mode on");
    }

    #[test]
    fn marquee_selects_enclosed_and_empty_click_clears() {
        let mut h = Harness::new(Tool::Pick);
        draw_rect(&mut h, 100.0, 100.0, 150.0, 150.0);
        draw_rect(&mut h, 300.0, 300.0, 350.0, 350.0);
        let (a, b) = (h.at(50.0, 50.0), h.at(200.0, 200.0));
        h.drag(a, b);
        assert_eq!(h.state.selection, vec![h.objects()[0].id]);
        let (a, b) = (h.at(50.0, 50.0), h.at(400.0, 400.0));
        h.drag(a, b);
        assert_eq!(h.state.selection.len(), 2);
        let empty = h.at(500.0, 100.0);
        h.click(empty);
        assert!(h.state.selection.is_empty());
    }

    #[test]
    fn corner_handle_scales_proportionally_and_side_handle_one_axis() {
        let mut h = Harness::new(Tool::Pick);
        draw_rect(&mut h, 100.0, 100.0, 200.0, 150.0);
        let gap = HANDLE_GAP / h.state.viewport.zoom();
        // Bottom-right handle, dragged 100 pt right along the diagonal: scale ×2 about top-left.
        let start = h.at(200.0 + gap, 150.0 + gap);
        let end = h.at(300.0 + gap, 200.0 + gap);
        h.drag(start, end);
        let b = h.bounds(0);
        assert!(close(b.x0, 100.0) && close(b.y0, 100.0), "{b:?}");
        assert!((b.width() / b.height() - 2.0).abs() < 0.01, "proportions kept: {b:?}");

        // Right side handle: width only.
        let b = h.bounds(0);
        let start = h.at(b.x1 + gap, b.center().y);
        let end = h.at(b.x1 + gap + 50.0, b.center().y + 30.0);
        let height = b.height();
        h.drag(start, end);
        assert!(close(h.bounds(0).height(), height));
        assert!(close(h.bounds(0).width(), b.width() + 50.0), "{:?}", h.bounds(0));
    }

    #[test]
    fn freehand_draws_a_curve() {
        let mut h = Harness::new(Tool::Freehand);
        let (a, b) = (h.at(50.0, 50.0), h.at(250.0, 120.0));
        h.drag(a, b);
        assert_eq!(h.objects().len(), 1);
        let Shape::Curve { path } = &h.objects()[0].shape else { panic!("curve") };
        assert!(!h.objects()[0].shape.is_closed());
        let b = path.bounding_box();
        assert!(close(b.x0, 50.0) && close(b.x1, 250.0), "{b:?}");
    }

    #[test]
    fn pen_clicks_and_drags_then_closes_on_first_node() {
        let mut h = Harness::new(Tool::Pen);
        let p0 = h.at(100.0, 100.0);
        let p1 = h.at(200.0, 100.0);
        h.click(p0);
        h.click(p1);
        // Drag at the third node to pull out a smooth handle.
        let (p2, handle) = (h.at(200.0, 200.0), h.at(240.0, 200.0));
        h.drag(p2, handle);
        assert!(h.objects().is_empty(), "still drawing");
        h.click(p0);
        assert_eq!(h.objects().len(), 1);
        let obj = &h.objects()[0];
        assert!(obj.shape.is_closed());
        let Shape::Curve { path } = &obj.shape else { panic!("curve") };
        assert_eq!(nodes::nodes(path).len(), 3);
        assert!(path.elements().iter().any(|e| matches!(e, PathEl::CurveTo(..))));
        assert!(h.state.tools.pen.is_none());
    }

    #[test]
    fn bezier_finishes_open_with_enter_equivalent() {
        let mut h = Harness::new(Tool::Bezier);
        let (a, b) = (h.at(10.0, 10.0), h.at(60.0, 10.0));
        h.click(a);
        h.click(b);
        finish_pen(&mut h.state);
        assert_eq!(h.objects().len(), 1);
        assert!(!h.objects()[0].shape.is_closed());
    }

    #[test]
    fn shape_tool_moves_nodes_and_adds_with_double_click() {
        let mut h = Harness::new(Tool::Shape);
        draw_rect(&mut h, 100.0, 100.0, 200.0, 200.0);
        let id = h.objects()[0].id;
        h.state.doc.convert_to_curves(&[id]);
        h.state.selection = vec![id];

        let (from, to) = (h.at(200.0, 200.0), h.at(250.0, 260.0));
        h.drag(from, to);
        assert_eq!(h.state.tools.nodes.len(), 1);
        let b = h.bounds(0);
        assert!(close(b.x1, 250.0) && close(b.y1, 260.0), "{b:?}");

        let Shape::Curve { path } = &h.objects()[0].shape else { panic!() };
        let before = nodes::nodes(path).len();
        let mid_top = h.at(150.0, 100.0);
        h.double_click(mid_top);
        let Shape::Curve { path } = &h.objects()[0].shape else { panic!() };
        assert_eq!(nodes::nodes(path).len(), before + 1);
    }

    #[test]
    fn shape_tool_rounds_rectangle_corners() {
        let mut h = Harness::new(Tool::Shape);
        draw_rect(&mut h, 100.0, 100.0, 200.0, 200.0);
        let (from, to) = (h.at(100.0, 100.0), h.at(120.0, 105.0));
        h.drag(from, to);
        let Shape::Rectangle { corner_radius, .. } = h.objects()[0].shape else { panic!() };
        assert!(close(corner_radius, 20.0), "{corner_radius}");
    }

    #[test]
    fn filled_objects_hit_inside() {
        let mut h = Harness::new(Tool::Pick);
        draw_rect(&mut h, 100.0, 100.0, 200.0, 200.0);
        h.state.doc.active_page_mut().objects[0].style.fill = Some(Rgba::BLACK.into());
        h.state.selection.clear();
        let inside = h.at(150.0, 150.0);
        h.click(inside);
        assert_eq!(h.state.selection.len(), 1);
    }
}
