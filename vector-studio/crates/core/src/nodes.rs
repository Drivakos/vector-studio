//! Node editing on Bézier paths (Shape tool).
//!
//! A node is addressed by the index of the path element that ends on it. In a closed
//! subpath whose last segment ends back on the start point, that end point and the
//! `MoveTo` are one node, addressed by the `MoveTo`.

use kurbo::{BezPath, CubicBez, Line, ParamCurve, ParamCurveNearest, PathEl, Point, QuadBez, Vec2};

const SAME_POINT: f64 = 1e-9;

/// A Bézier control point: element `elem`, control `index` (0 = first, 1 = second).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandleRef {
    pub elem: usize,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub elem: usize,
    pub point: Point,
    pub handle_in: Option<(HandleRef, Point)>,
    pub handle_out: Option<(HandleRef, Point)>,
}

fn end_point(el: PathEl) -> Option<Point> {
    match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => Some(p),
        PathEl::ClosePath => None,
    }
}

/// For each element: (point before it, start of its subpath).
fn walk(els: &[PathEl]) -> Vec<(Point, Point)> {
    let mut out = Vec::with_capacity(els.len());
    let (mut cur, mut start) = (Point::ZERO, Point::ZERO);
    for el in els {
        out.push((cur, start));
        match *el {
            PathEl::MoveTo(p) => {
                cur = p;
                start = p;
            }
            PathEl::ClosePath => cur = start,
            other => cur = end_point(other).unwrap_or(cur),
        }
    }
    out
}

/// Index of the segment that ends back on this subpath's `MoveTo` at `move_elem`, if any.
fn closing_duplicate(els: &[PathEl], move_elem: usize) -> Option<usize> {
    let PathEl::MoveTo(start) = els[move_elem] else { return None };
    let close = (move_elem + 1..els.len())
        .take_while(|&i| !matches!(els[i], PathEl::MoveTo(_)))
        .find(|&i| matches!(els[i], PathEl::ClosePath))?;
    let last = close.checked_sub(1).filter(|&j| j > move_elem)?;
    let p = end_point(els[last])?;
    ((p - start).hypot() < SAME_POINT).then_some(last)
}

fn is_closing_duplicate(els: &[PathEl], elem: usize) -> bool {
    if !matches!(els.get(elem + 1), Some(PathEl::ClosePath)) || matches!(els[elem], PathEl::MoveTo(_)) {
        return false;
    }
    let mv = (0..elem).rev().find(|&i| matches!(els[i], PathEl::MoveTo(_)));
    mv.and_then(|m| closing_duplicate(els, m)) == Some(elem)
}

pub fn nodes(path: &BezPath) -> Vec<Node> {
    let els = path.elements();
    let mut out = Vec::new();
    for (i, el) in els.iter().enumerate() {
        let Some(point) = end_point(*el) else { continue };
        if is_closing_duplicate(els, i) {
            continue;
        }
        let in_elem = if matches!(el, PathEl::MoveTo(_)) { closing_duplicate(els, i) } else { Some(i) };
        let handle_in = in_elem.and_then(|j| match els[j] {
            PathEl::CurveTo(_, c2, _) => Some((HandleRef { elem: j, index: 1 }, c2)),
            _ => None,
        });
        let handle_out = match els.get(i + 1) {
            Some(PathEl::CurveTo(c1, _, _)) => Some((HandleRef { elem: i + 1, index: 0 }, *c1)),
            _ => None,
        };
        out.push(Node { elem: i, point, handle_in, handle_out });
    }
    out
}

fn shift_end(el: &mut PathEl, delta: Vec2) {
    match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) | PathEl::QuadTo(_, p) => *p += delta,
        PathEl::CurveTo(_, c2, p) => {
            *c2 += delta;
            *p += delta;
        }
        PathEl::ClosePath => {}
    }
}

/// Move a node and its handles by `delta`.
pub fn move_node(path: &mut BezPath, elem: usize, delta: Vec2) {
    let dup = closing_duplicate(path.elements(), elem);
    let els = path.elements_mut();
    if elem >= els.len() {
        return;
    }
    shift_end(&mut els[elem], delta);
    if let Some(j) = dup {
        shift_end(&mut els[j], delta);
    }
    if let Some(PathEl::CurveTo(c1, _, _)) = els.get_mut(elem + 1) {
        *c1 += delta;
    }
}

pub fn set_handle(path: &mut BezPath, handle: HandleRef, to: Point) {
    if let Some(PathEl::CurveTo(c1, c2, _)) = path.elements_mut().get_mut(handle.elem) {
        if handle.index == 0 { *c1 = to } else { *c2 = to }
    }
}

/// The segment nearest `point`: (element index, curve parameter, distance).
/// For an implicit closing line the element is the `ClosePath`.
pub fn nearest_segment(path: &BezPath, point: Point) -> Option<(usize, f64, f64)> {
    let els = path.elements();
    let mut best: Option<(usize, f64, f64)> = None;
    for (i, (el, (prev, start))) in els.iter().zip(walk(els)).enumerate() {
        let nearest = match *el {
            PathEl::LineTo(p) => Line::new(prev, p).nearest(point, 1e-6),
            PathEl::QuadTo(c, p) => QuadBez::new(prev, c, p).nearest(point, 1e-6),
            PathEl::CurveTo(c1, c2, p) => CubicBez::new(prev, c1, c2, p).nearest(point, 1e-6),
            PathEl::ClosePath if (prev - start).hypot() > SAME_POINT => {
                Line::new(prev, start).nearest(point, 1e-6)
            }
            _ => continue,
        };
        let d = nearest.distance_sq.sqrt();
        if best.is_none_or(|b| d < b.2) {
            best = Some((i, nearest.t, d));
        }
    }
    best
}

/// Split the segment at `elem` at parameter `t`. Returns the new node's element index.
pub fn insert_node(path: &mut BezPath, elem: usize, t: f64) -> Option<usize> {
    let els = path.elements().to_vec();
    let (prev, start) = *walk(&els).get(elem)?;
    let t = t.clamp(0.0, 1.0);
    let replacement: Vec<PathEl> = match els[elem] {
        PathEl::LineTo(p) => {
            let mid = Line::new(prev, p).eval(t);
            vec![PathEl::LineTo(mid), PathEl::LineTo(p)]
        }
        PathEl::QuadTo(c, p) => {
            let q = QuadBez::new(prev, c, p);
            let (a, b) = (q.subsegment(0.0..t), q.subsegment(t..1.0));
            vec![PathEl::QuadTo(a.p1, a.p2), PathEl::QuadTo(b.p1, b.p2)]
        }
        PathEl::CurveTo(c1, c2, p) => {
            let c = CubicBez::new(prev, c1, c2, p);
            let (a, b) = (c.subsegment(0.0..t), c.subsegment(t..1.0));
            vec![PathEl::CurveTo(a.p1, a.p2, a.p3), PathEl::CurveTo(b.p1, b.p2, b.p3)]
        }
        PathEl::ClosePath => {
            let mid = Line::new(prev, start).eval(t);
            vec![PathEl::LineTo(mid), PathEl::ClosePath]
        }
        PathEl::MoveTo(_) => return None,
    };
    let mut new_els = els[..elem].to_vec();
    new_els.extend(replacement);
    new_els.extend_from_slice(&els[elem + 1..]);
    *path = BezPath::from_vec(new_els);
    Some(elem)
}

/// Remove a node, joining its neighbours. Refuses if its subpath would drop below 2 nodes.
pub fn delete_node(path: &mut BezPath, elem: usize) -> bool {
    let mut els = path.elements().to_vec();
    if elem >= els.len() || end_point(els[elem]).is_none() || is_closing_duplicate(&els, elem) {
        return false;
    }
    let sub_start = (0..=elem).rev().find(|&i| matches!(els[i], PathEl::MoveTo(_))).unwrap_or(0);
    let sub_end = (sub_start + 1..els.len())
        .find(|&i| matches!(els[i], PathEl::MoveTo(_)))
        .unwrap_or(els.len());
    let node_count = (sub_start..sub_end)
        .filter(|&i| end_point(els[i]).is_some() && !is_closing_duplicate(&els, i))
        .count();
    if node_count <= 2 {
        return false;
    }
    if matches!(els[elem], PathEl::MoveTo(_)) {
        let dup = closing_duplicate(&els, elem);
        let Some(next) = end_point(els[elem + 1]) else { return false };
        els[elem] = PathEl::MoveTo(next);
        els.remove(elem + 1);
        // The closing segment now has to end on the new start point.
        if let Some(j) = dup.map(|j| j - 1) {
            match &mut els[j] {
                PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => *p = next,
                _ => {}
            }
        }
    } else {
        els.remove(elem);
    }
    *path = BezPath::from_vec(els);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> BezPath {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.line_to((10.0, 10.0));
        p.close_path();
        p
    }

    fn closed_curve() -> BezPath {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.curve_to((5.0, -5.0), (15.0, -5.0), (20.0, 0.0));
        p.curve_to((15.0, 5.0), (5.0, 5.0), (0.0, 0.0));
        p.close_path();
        p
    }

    #[test]
    fn closing_duplicate_is_one_node() {
        assert_eq!(nodes(&triangle()).len(), 3);
        let n = nodes(&closed_curve());
        assert_eq!(n.len(), 2);
        assert_eq!(n[0].handle_in.map(|h| h.1), Some(Point::new(5.0, 5.0)));
        assert_eq!(n[0].handle_out.map(|h| h.1), Some(Point::new(5.0, -5.0)));
    }

    #[test]
    fn moving_start_node_moves_its_duplicate() {
        let mut p = closed_curve();
        move_node(&mut p, 0, Vec2::new(1.0, 2.0));
        assert_eq!(p.elements()[0], PathEl::MoveTo(Point::new(1.0, 2.0)));
        assert_eq!(p.elements()[2], PathEl::CurveTo((15.0, 5.0).into(), (6.0, 7.0).into(), (1.0, 2.0).into()));
        assert_eq!(p.elements()[1], PathEl::CurveTo((6.0, -3.0).into(), (15.0, -5.0).into(), (20.0, 0.0).into()));
    }

    #[test]
    fn insert_splits_curve_at_t() {
        let mut p = closed_curve();
        let before = p.segments().next().unwrap().eval(0.5);
        insert_node(&mut p, 1, 0.5).unwrap();
        assert_eq!(nodes(&p).len(), 3);
        assert!((nodes(&p)[1].point - before).hypot() < 1e-9);
    }

    #[test]
    fn insert_on_implicit_closing_line() {
        let mut p = triangle();
        let (elem, t, _) = nearest_segment(&p, Point::new(5.0, 5.5)).unwrap();
        assert_eq!(elem, 3);
        insert_node(&mut p, elem, t).unwrap();
        assert_eq!(nodes(&p).len(), 4);
    }

    #[test]
    fn delete_node_rules() {
        let mut p = triangle();
        assert!(delete_node(&mut p, 1));
        assert_eq!(nodes(&p).len(), 2);
        assert!(!delete_node(&mut p, 1), "a subpath keeps at least two nodes");

        let mut p = triangle();
        assert!(delete_node(&mut p, 0));
        assert_eq!(p.elements()[0], PathEl::MoveTo(Point::new(10.0, 0.0)));
    }
}
