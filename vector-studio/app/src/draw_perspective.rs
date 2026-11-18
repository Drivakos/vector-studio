//! Object > Perspective > Draw in Perspective: a plane laid over the page as if seen at an
//! angle. While it is on, the drawing tools draw on the plane: what they draw goes
//! through the plane's perspective and keeps it. The Pick tool drags the plane's corners.

use egui::{Color32, Pos2, Stroke};
use vs_core::kurbo::{Point, Rect};
use vs_core::{ObjectId, PageId, Perspective};

use crate::app::AppState;
use crate::toolbox::Tool;

/// Where a new plane's corners go, as fractions of the page (top-left, top-right,
/// bottom-right, bottom-left): a floor that narrows towards the top. This app's choice.
pub const DEFAULT_CORNERS: [(f64, f64); 4] = [(0.25, 0.3), (0.75, 0.3), (1.0, 1.0), (0.0, 1.0)];

/// Lines of the plane's grid each way.
const GRID: usize = 8;
const PLANE: Color32 = Color32::from_rgb(0xD9, 0x6A, 0x1E);

#[derive(Clone, Debug, PartialEq)]
pub struct DrawPerspective {
    /// The plane's corners as fractions of the active page, so it fits any page size.
    pub corners: [Point; 4],
    /// The page and the highest object id seen there: objects with higher ids are new.
    seen: (PageId, u64),
}

/// The drawing tools that draw on the plane.
pub fn draws_on_plane(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Rectangle
            | Tool::Ellipse
            | Tool::Polygon
            | Tool::Star
            | Tool::CommonShapes
            | Tool::Spiral
            | Tool::GraphPaper
            | Tool::Freehand
            | Tool::TwoPointLine
            | Tool::Bezier
            | Tool::Pen
            | Tool::Polyline
            | Tool::BSpline
            | Tool::ThreePointCurve
            | Tool::ThreePointRectangle
            | Tool::ThreePointEllipse
    )
}

fn highest_id(state: &AppState) -> u64 {
    state.doc.active_page().objects.iter().map(|o| o.id.0).max().unwrap_or(0)
}

/// Draw in Perspective on or off.
pub fn toggle(state: &mut AppState) {
    state.draw_perspective = match state.draw_perspective {
        Some(_) => None,
        None => Some(DrawPerspective {
            corners: DEFAULT_CORNERS.map(|(x, y)| Point::new(x, y)),
            seen: (state.doc.active_page().id, highest_id(state)),
        }),
    };
}

impl DrawPerspective {
    /// The plane's corners on `page` (page space).
    pub fn quad(&self, page: Rect) -> [Point; 4] {
        self.corners.map(|c| Point::new(page.x0 + c.x * page.width(), page.y0 + c.y * page.height()))
    }

    /// The plane's map: page space as drawn on the plane → page space as shown.
    pub fn map(&self, page: Rect) -> Option<Perspective> {
        Perspective::rect_to_quad(page, self.quad(page))
    }
}

/// The plane's map while the active tool draws on it.
pub fn plane(state: &AppState) -> Option<Perspective> {
    let dp = state.draw_perspective.as_ref().filter(|_| draws_on_plane(state.tool))?;
    dp.map(state.doc.active_page().bounds())
}

/// The plane corner at `p` (page space), within `tolerance`.
pub fn corner_at(state: &AppState, p: Point, tolerance: f64) -> Option<usize> {
    let dp = state.draw_perspective.as_ref()?;
    dp.quad(state.doc.active_page().bounds()).iter().position(|c| (*c - p).hypot() <= tolerance)
}

/// Move plane corner `index` to `to` (page space). A corner that would fold the plane
/// stays where it was.
pub fn move_corner(state: &mut AppState, index: usize, to: Point) {
    let page = state.doc.active_page().bounds();
    let Some(dp) = state.draw_perspective.as_mut() else { return };
    let mut quad = dp.quad(page);
    quad[index] = to;
    if Perspective::rect_to_quad(page, quad).is_some() {
        dp.corners[index] = Point::new((to.x - page.x0) / page.width(), (to.y - page.y0) / page.height());
    }
}

/// Once a frame: objects a drawing tool has just made go onto the plane.
pub fn sync(state: &mut AppState) {
    let Some(dp) = &state.draw_perspective else { return };
    let page_id = state.doc.active_page().id;
    let (seen_page, seen_id) = dp.seen;
    let plane = plane(state);
    let page = state.doc.active_page_mut();
    let new: Vec<ObjectId> = page.objects.iter().filter(|o| o.id.0 > seen_id && o.perspective.is_none()).map(|o| o.id).collect();
    if seen_page == page_id
        && let Some(h) = plane
    {
        for id in new {
            if let Some(obj) = page.object_mut(id) {
                obj.perspective = Some(h.seen_from(obj.transform));
            }
        }
    }
    let highest = highest_id(state);
    if let Some(dp) = &mut state.draw_perspective {
        dp.seen = (page_id, highest);
    }
}

/// The plane: its outline, a grid, and a handle on each corner.
pub fn draw(painter: &egui::Painter, state: &AppState, screen: impl Fn(Point) -> Pos2) {
    let Some(dp) = &state.draw_perspective else { return };
    let page = state.doc.active_page().bounds();
    let Some(h) = dp.map(page) else { return };
    let thin = Stroke::new(1.0, PLANE.gamma_multiply(0.6));
    for k in 0..=GRID {
        let t = k as f64 / GRID as f64;
        let (x, y) = (page.x0 + t * page.width(), page.y0 + t * page.height());
        let stroke = if k == 0 || k == GRID { Stroke::new(1.5, PLANE) } else { thin };
        painter.line_segment([screen(h.apply(Point::new(x, page.y0))), screen(h.apply(Point::new(x, page.y1)))], stroke);
        painter.line_segment([screen(h.apply(Point::new(page.x0, y))), screen(h.apply(Point::new(page.x1, y)))], stroke);
    }
    for c in dp.quad(page) {
        let r = egui::Rect::from_center_size(screen(c), egui::vec2(8.0, 8.0));
        painter.rect_filled(r, 0.0, Color32::WHITE);
        painter.rect_stroke(r, 0.0, Stroke::new(1.5, PLANE), egui::StrokeKind::Inside);
    }
}
