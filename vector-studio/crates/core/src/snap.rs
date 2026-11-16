//! Guidelines and snapping to the grid, guidelines, objects and the page.
//!
//! Snapping works per axis: an x coordinate snaps to the nearest vertical target (grid
//! line, vertical guideline, page edge or centre, object edge or centre) within the
//! threshold, and y likewise.

use kurbo::{Point, Rect, Vec2};
use serde::{Deserialize, Serialize};

use crate::document::{ObjectId, Page};

/// Which way a guideline runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuideAxis {
    /// A horizontal line at y = position.
    Horizontal,
    /// A vertical line at x = position.
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub axis: GuideAxis,
    /// Page points from the top (horizontal) or left (vertical) edge of the page.
    pub position: f64,
}

/// What to snap to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapOptions {
    /// Grid spacing in points, when snapping to the grid.
    pub grid: Option<f64>,
    /// Pixel size in points, when snapping to the pixel grid.
    pub pixels: Option<f64>,
    pub guides: bool,
    pub objects: bool,
    pub page: bool,
}

/// Snap targets on one page, collected once per gesture.
#[derive(Clone, Debug, Default)]
pub struct Snapper {
    xs: Vec<f64>,
    ys: Vec<f64>,
    grid: Option<f64>,
    pixels: Option<f64>,
    threshold: f64,
}

fn edges(r: Rect) -> [f64; 3] {
    [r.x0, r.center().x, r.x1]
}

fn edges_y(r: Rect) -> [f64; 3] {
    [r.y0, r.center().y, r.y1]
}

impl Snapper {
    /// Targets on `page` for `options`, ignoring the objects in `exclude` (the ones being
    /// moved or edited). `threshold` is the snap distance in points.
    pub fn new(page: &Page, options: &SnapOptions, exclude: &[ObjectId], threshold: f64) -> Self {
        let mut s = Self {
            grid: options.grid.filter(|g| *g > 0.0),
            pixels: options.pixels.filter(|g| *g > 0.0),
            threshold,
            ..Self::default()
        };
        if options.guides {
            for g in &page.guides {
                match g.axis {
                    GuideAxis::Vertical => s.xs.push(g.position),
                    GuideAxis::Horizontal => s.ys.push(g.position),
                }
            }
        }
        if options.page {
            s.xs.extend(edges(page.bounds()));
            s.ys.extend(edges_y(page.bounds()));
        }
        if options.objects {
            for obj in page.shown_objects().filter(|o| !exclude.contains(&o.id)) {
                let b = obj.world_bounds();
                s.xs.extend(edges(b));
                s.ys.extend(edges_y(b));
            }
        }
        s
    }

    pub fn is_active(&self) -> bool {
        self.grid.is_some() || self.pixels.is_some() || !self.xs.is_empty() || !self.ys.is_empty()
    }

    /// The nearest target to `v` within the threshold.
    fn nearest(&self, v: f64, targets: &[f64]) -> Option<f64> {
        let on = |g: f64| (v / g).round() * g;
        targets
            .iter()
            .copied()
            .chain(self.grid.map(on))
            .chain(self.pixels.map(on))
            .filter(|t| (t - v).abs() <= self.threshold)
            .min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
    }

    pub fn x(&self, x: f64) -> Option<f64> {
        self.nearest(x, &self.xs)
    }

    pub fn y(&self, y: f64) -> Option<f64> {
        self.nearest(y, &self.ys)
    }

    /// `p` with each coordinate snapped if a target is close enough.
    pub fn point(&self, p: Point) -> Point {
        Point::new(self.x(p.x).unwrap_or(p.x), self.y(p.y).unwrap_or(p.y))
    }

    /// The shift that snaps `bounds` (already moved) by its closest edge or centre on
    /// each axis.
    pub fn delta(&self, bounds: Rect) -> Vec2 {
        let best = |values: [f64; 3], snap: &dyn Fn(f64) -> Option<f64>| {
            values
                .iter()
                .filter_map(|v| snap(*v).map(|t| t - v))
                .min_by(|a, b| a.abs().total_cmp(&b.abs()))
                .unwrap_or(0.0)
        };
        Vec2::new(best(edges(bounds), &|v| self.x(v)), best(edges_y(bounds), &|v| self.y(v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Shape, Style};

    fn page_with_guides() -> Document {
        let mut doc = Document::default();
        doc.active_page_mut().guides.push(Guide { axis: GuideAxis::Vertical, position: 100.0 });
        doc.active_page_mut().guides.push(Guide { axis: GuideAxis::Horizontal, position: 50.0 });
        doc
    }

    #[test]
    fn pixels_snap_to_the_pixel_grid() {
        let doc = Document::default();
        // 300 dpi: a pixel is 0.24 pt.
        let opts = SnapOptions { pixels: Some(72.0 / 300.0), ..Default::default() };
        let s = Snapper::new(doc.active_page(), &opts, &[], 1.0);
        assert!(s.is_active());
        let p = s.point(Point::new(10.05, 3.0));
        assert!((p.x - 10.08).abs() < 1e-9 && (p.y - 3.12).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn points_snap_per_axis_within_the_threshold() {
        let doc = page_with_guides();
        let opts = SnapOptions { guides: true, ..Default::default() };
        let s = Snapper::new(doc.active_page(), &opts, &[], 5.0);
        assert_eq!(s.point(Point::new(97.0, 53.0)), Point::new(100.0, 50.0));
        assert_eq!(s.point(Point::new(90.0, 53.0)), Point::new(90.0, 50.0), "x too far, y snaps");
        let off = Snapper::new(doc.active_page(), &SnapOptions::default(), &[], 5.0);
        assert!(!off.is_active());
        assert_eq!(off.point(Point::new(97.0, 53.0)), Point::new(97.0, 53.0));
    }

    #[test]
    fn grid_snaps_to_the_nearest_line() {
        let doc = Document::default();
        let opts = SnapOptions { grid: Some(10.0), ..Default::default() };
        let s = Snapper::new(doc.active_page(), &opts, &[], 3.0);
        assert_eq!(s.point(Point::new(28.0, 31.0)), Point::new(30.0, 30.0));
        assert_eq!(s.point(Point::new(25.0, 31.0)), Point::new(25.0, 30.0));
    }

    #[test]
    fn moving_bounds_snap_by_the_closest_edge_or_centre() {
        let mut doc = Document::default();
        let other = doc.add_object(Shape::Rectangle { rect: Rect::new(200.0, 200.0, 300.0, 300.0), corner_radius: 0.0 }, Style::default());
        let opts = SnapOptions { objects: true, page: true, ..Default::default() };
        let s = Snapper::new(doc.active_page(), &opts, &[], 4.0);
        // Left edge 2 pt right of the other's right edge; top 3 pt above its centre.
        let d = s.delta(Rect::new(302.0, 247.0, 342.0, 287.0));
        assert_eq!(d, Vec2::new(-2.0, 3.0));
        // Excluded objects are not targets; the page's left edge still is.
        let s = Snapper::new(doc.active_page(), &opts, &[other], 4.0);
        assert_eq!(s.delta(Rect::new(302.0, 247.0, 342.0, 287.0)), Vec2::ZERO);
        assert_eq!(s.delta(Rect::new(1.0, 500.0, 11.0, 510.0)).x, -1.0);
    }
}
