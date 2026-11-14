//! Maps document space (points, y down, origin at the page's top-left) to
//! canvas space (logical screen points, origin at the canvas's top-left).

use kurbo::{Affine, Point, Rect, Size, Vec2};

/// Screen points per document point at 100% zoom (96 DPI screen, 72 pt per inch).
pub const ACTUAL_SIZE_ZOOM: f64 = 96.0 / 72.0;
pub const MIN_ZOOM_PERCENT: f64 = 1.0;
pub const MAX_ZOOM_PERCENT: f64 = 25_600.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Screen points per document point.
    zoom: f64,
    /// Canvas position of the document origin.
    offset: Vec2,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: ACTUAL_SIZE_ZOOM,
            offset: Vec2::ZERO,
        }
    }
}

impl Viewport {
    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn offset(&self) -> Vec2 {
        self.offset
    }

    pub fn zoom_percent(&self) -> f64 {
        self.zoom / ACTUAL_SIZE_ZOOM * 100.0
    }

    pub fn transform(&self) -> Affine {
        Affine::translate(self.offset) * Affine::scale(self.zoom)
    }

    pub fn doc_to_screen(&self, p: Point) -> Point {
        (p.to_vec2() * self.zoom + self.offset).to_point()
    }

    pub fn screen_to_doc(&self, p: Point) -> Point {
        ((p.to_vec2() - self.offset) / self.zoom).to_point()
    }

    pub fn pan(&mut self, delta: Vec2) {
        self.offset += delta;
    }

    /// Multiply the zoom by `factor`, keeping the document point under `anchor` fixed.
    pub fn zoom_at(&mut self, anchor: Point, factor: f64) {
        let doc_anchor = self.screen_to_doc(anchor);
        self.zoom = clamp_zoom(self.zoom * factor);
        self.offset = anchor.to_vec2() - doc_anchor.to_vec2() * self.zoom;
    }

    /// Set an absolute zoom percentage, keeping `anchor` fixed.
    pub fn set_zoom_percent(&mut self, percent: f64, anchor: Point) {
        let target = percent / 100.0 * ACTUAL_SIZE_ZOOM;
        self.zoom_at(anchor, target / self.zoom);
    }

    /// Zoom and centre so `doc_rect` fills `view` with `margin` screen points around it.
    pub fn fit(&mut self, doc_rect: Rect, view: Size, margin: f64) {
        let avail_w = (view.width - 2.0 * margin).max(1.0);
        let avail_h = (view.height - 2.0 * margin).max(1.0);
        if doc_rect.width() <= 0.0 || doc_rect.height() <= 0.0 {
            return;
        }
        self.zoom = clamp_zoom((avail_w / doc_rect.width()).min(avail_h / doc_rect.height()));
        let view_center = Vec2::new(view.width / 2.0, view.height / 2.0);
        self.offset = view_center - doc_rect.center().to_vec2() * self.zoom;
    }
}

fn clamp_zoom(zoom: f64) -> f64 {
    let min = MIN_ZOOM_PERCENT / 100.0 * ACTUAL_SIZE_ZOOM;
    let max = MAX_ZOOM_PERCENT / 100.0 * ACTUAL_SIZE_ZOOM;
    zoom.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-9
    }

    #[test]
    fn screen_and_doc_round_trip() {
        let mut v = Viewport::default();
        v.pan(Vec2::new(40.0, -12.0));
        v.zoom_at(Point::new(100.0, 100.0), 3.0);
        let p = Point::new(123.4, 56.7);
        assert!(close(v.screen_to_doc(v.doc_to_screen(p)), p));
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut v = Viewport::default();
        let anchor = Point::new(250.0, 180.0);
        let before = v.screen_to_doc(anchor);
        v.zoom_at(anchor, 2.5);
        assert!(close(v.screen_to_doc(anchor), before));
        assert!((v.zoom_percent() - 250.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = Viewport::default();
        v.zoom_at(Point::ZERO, 1e9);
        assert_eq!(v.zoom_percent(), MAX_ZOOM_PERCENT);
        v.zoom_at(Point::ZERO, 1e-12);
        assert!((v.zoom_percent() - MIN_ZOOM_PERCENT).abs() < 1e-9);
    }

    #[test]
    fn fit_centres_page_inside_margin() {
        let mut v = Viewport::default();
        let page = Rect::new(0.0, 0.0, 595.0, 842.0);
        let view = Size::new(800.0, 600.0);
        v.fit(page, view, 20.0);
        let tl = v.doc_to_screen(Point::new(page.x0, page.y0));
        let br = v.doc_to_screen(Point::new(page.x1, page.y1));
        // Height is the limiting side: exactly 20 pt margin top and bottom.
        assert!((tl.y - 20.0).abs() < 1e-9 && (br.y - 580.0).abs() < 1e-9);
        // Horizontally centred.
        assert!(((tl.x + br.x) / 2.0 - 400.0).abs() < 1e-9);
    }

    #[test]
    fn set_zoom_percent_is_absolute() {
        let mut v = Viewport::default();
        v.zoom_at(Point::ZERO, 7.0);
        v.set_zoom_percent(50.0, Point::ZERO);
        assert!((v.zoom_percent() - 50.0).abs() < 1e-9);
    }
}
