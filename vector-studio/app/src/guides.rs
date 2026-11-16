//! Drawing the document grid and guidelines over the canvas, and pulling guidelines out
//! of the rulers.

use egui::{Color32, Pos2, Rect, Stroke};
use vs_core::GuideAxis;
use vs_core::kurbo::Point;

use crate::app::AppState;

const GRID: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 0x30);
pub const GUIDE: Color32 = Color32::from_rgb(0x00, 0x8C, 0xFF);
/// A selected guideline.
pub const GUIDE_SELECTED: Color32 = Color32::from_rgb(0xE5, 0x1C, 0x23);
/// Grid lines closer than this on screen are thinned out.
const MIN_GRID_GAP: f64 = 8.0;

fn screen(state: &AppState, canvas: Rect, p: Point) -> Pos2 {
    let s = state.viewport.doc_to_screen(p);
    canvas.min + egui::vec2(s.x as f32, s.y as f32)
}

/// Grid lines (over the page) and guidelines (across the whole window).
pub fn draw(painter: &egui::Painter, state: &AppState, canvas: Rect) {
    let page = state.doc.active_page();
    let zoom = state.viewport.zoom();
    if state.prefs.show_grid && state.prefs.grid_spacing > 0.0 {
        // Every line, every 2nd, 5th, 10th, … so lines stay at least MIN_GRID_GAP apart.
        let base = state.prefs.grid_spacing;
        let step = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0]
            .into_iter()
            .map(|k| k * base)
            .find(|s| s * zoom >= MIN_GRID_GAP);
        if let Some(step) = step {
            let size = page.size;
            let stroke = Stroke::new(1.0, GRID);
            let mut x = 0.0;
            while x <= size.width + 1e-6 {
                painter.line_segment([screen(state, canvas, Point::new(x, 0.0)), screen(state, canvas, Point::new(x, size.height))], stroke);
                x += step;
            }
            let mut y = 0.0;
            while y <= size.height + 1e-6 {
                painter.line_segment([screen(state, canvas, Point::new(0.0, y)), screen(state, canvas, Point::new(size.width, y))], stroke);
                y += step;
            }
        }
    }
    if state.prefs.show_guidelines {
        let selected = state.selected_guides();
        for (i, g) in page.guides.iter().enumerate() {
            let stroke = Stroke::new(1.0, if selected.contains(&i) { GUIDE_SELECTED } else { GUIDE });
            match g.axis {
                GuideAxis::Horizontal => {
                    let y = screen(state, canvas, Point::new(0.0, g.position)).y;
                    painter.hline(canvas.x_range(), y, stroke);
                }
                GuideAxis::Vertical => {
                    let x = screen(state, canvas, Point::new(g.position, 0.0)).x;
                    painter.vline(x, canvas.y_range(), stroke);
                }
            }
        }
    }
}

/// Dragging from the top ruler makes a horizontal guideline, from the left ruler a
/// vertical one. Turns the guidelines on if they were hidden.
pub fn ruler_drag(ui: &egui::Ui, state: &mut AppState, top: Rect, left: Rect, canvas: Rect) {
    for (rect, axis, name) in [(top, GuideAxis::Horizontal, "Horizontal ruler"), (left, GuideAxis::Vertical, "Vertical ruler")] {
        let resp = ui.interact(rect, ui.scope_id().with(name), egui::Sense::drag());
        crate::a11y::name(&resp, name);
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let local = Point::new((p.x - canvas.min.x) as f64, (p.y - canvas.min.y) as f64);
            state.prefs.show_guidelines = true;
            crate::tools::start_guide(state, axis, state.viewport.screen_to_doc(local));
        }
    }
}
