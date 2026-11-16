//! Grid, guidelines (pulled from the rulers, moved, removed) and snapping to the grid,
//! guidelines, objects and the page.

use egui::{Key, Modifiers, pos2};
use vs_core::{Guide, GuideAxis, Unit};

use crate::support::{E2e, close, rect_close};

fn mm(v: f64) -> f64 {
    Unit::Millimeters.to_points(v)
}

fn guides(app: &E2e) -> Vec<Guide> {
    app.doc().active_page().guides.clone()
}

/// Window position on the top ruler above document x.
fn top_ruler(app: &E2e, x: f64) -> egui::Pos2 {
    pos2(app.at(x, 0.0).x, app.state().canvas_rect.min.y - 10.0)
}

#[test]
fn grid_shows_from_the_toolbar_and_snaps_new_shapes() {
    let mut app = E2e::new();
    assert!(!app.state().prefs.show_grid);
    app.click("Grid");
    assert!(app.state().prefs.show_grid);
    // A grid line every 5 mm: the 20th sits at 100 mm.
    let x = mm(100.0);
    let shaded = (0..9).any(|i| {
        let px = app.pixel(x - 1.0 + f64::from(i) * 0.25, 500.0);
        px[..3] != [255, 255, 255]
    });
    assert!(shaded, "a grid line is drawn");

    app.menu(&["View", "Snap To", "Document Grid"]);
    assert!(app.state().prefs.snap_grid);
    // Both corners land 1-2 pt off grid lines (7 × 5 mm and 14 × 5 mm).
    let r = app.draw_rect(100.0, 100.0, 200.0, 199.0);
    let b = app.bounds(app.ids().iter().position(|i| *i == r).unwrap());
    assert!(rect_close(b, mm(35.0), mm(35.0), mm(70.0), mm(70.0)), "{b:?}");

    // Hiding the grid stops snapping to it.
    app.menu(&["View", "Grid"]);
    app.draw_rect(100.0, 300.0, 200.0, 401.0);
    assert!(rect_close(app.bounds(1), 100.0, 300.0, 200.0, 401.0), "{:?}", app.bounds(1));

    // Grid spacing is a document setting in the Properties docker.
    app.click_doc(500.0, 800.0);
    app.docker("Properties");
    app.fill("Grid spacing (document)", "10");
    assert!(close(app.state().prefs.grid_spacing, mm(10.0)));
}

#[test]
fn guidelines_come_from_the_rulers_snap_and_go_back_to_the_ruler() {
    let mut app = E2e::new();
    app.click("Rulers");
    let (a, b) = (top_ruler(&app, 400.0), app.at(400.0, 300.0));
    app.drag_screen(a, b);
    let g = guides(&app);
    assert_eq!(g.len(), 1);
    assert!(g[0].axis == GuideAxis::Horizontal && close(g[0].position, 300.0), "{g:?}");
    let shaded = app.pixel(450.0, 300.0);
    assert!(shaded[2] > shaded[0] + 60, "the guideline is drawn in blue: {shaded:?}");

    // A vertical one from the left ruler.
    let left = pos2(app.state().canvas_rect.min.x - 10.0, app.at(0.0, 600.0).y);
    app.drag_screen(left, app.at(250.0, 600.0));
    let g = guides(&app)[1];
    assert!(g.axis == GuideAxis::Vertical && close(g.position, 250.0), "{g:?}");

    // New shapes and moved objects snap to them.
    let r = app.draw_rect(100.0, 297.0, 200.0, 400.0);
    assert!(close(app.bounds(0).y0, 300.0), "{:?}", app.bounds(0));
    app.drag_doc((100.0, 350.0), (146.0, 380.0));
    let b = app.bounds(0);
    assert!(close(b.x1, 250.0), "the right edge snapped to the vertical guideline: {b:?}");
    assert_eq!(app.selection(), vec![r]);

    // Snap To > Guidelines off: no snapping.
    app.menu(&["View", "Snap To", "Guidelines"]);
    assert!(!app.state().prefs.snap_guidelines);
    app.draw_rect(300.0, 297.0, 360.0, 330.0);
    assert!(close(app.bounds(1).y0, 297.0));
    app.menu(&["View", "Snap To", "Guidelines"]);

    // Drag a guideline with the Pick tool; drop it on the ruler to remove it.
    app.click_doc(500.0, 800.0);
    app.drag_doc((500.0, 300.0), (500.0, 320.0));
    assert!(close(guides(&app)[0].position, 320.0));
    let (a, b) = (app.at(500.0, 320.0), top_ruler(&app, 500.0));
    app.drag_screen(a, b);
    assert_eq!(guides(&app).len(), 1, "dropped on the ruler: removed");
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(guides(&app).len(), 2, "undo brings it back");

    // Hidden guidelines neither show, snap nor pick.
    app.click("Guidelines");
    assert!(!app.state().prefs.show_guidelines);
    assert_eq!(app.pixel(450.0, 600.0)[..3], [255, 255, 255]);
}

#[test]
fn snap_to_objects_and_page() {
    let mut app = E2e::new();
    let a = app.draw_rect(100.0, 100.0, 200.0, 200.0);
    let b = app.draw_rect(300.0, 300.0, 350.0, 350.0);
    app.menu(&["View", "Snap To", "Objects"]);
    assert!(app.state().prefs.snap_objects);
    // Move b so its left edge comes 3 pt right of a's right edge, its top 2 pt below a's top.
    app.click_doc(325.0, 300.0);
    app.drag_doc((325.0, 300.0), (228.0, 102.0));
    let i = app.ids().iter().position(|id| *id == b).unwrap();
    assert!(rect_close(app.bounds(i), 200.0, 100.0, 250.0, 150.0), "{:?}", app.bounds(i));
    let _ = a;

    app.menu(&["View", "Snap To", "Page"]);
    app.key(Key::F7);
    app.drag_doc((3.0, 2.0), (60.0, 60.0));
    let e = app.bounds(2);
    assert!(close(e.x0, 0.0) && close(e.y0, 0.0), "starts at the page corner: {e:?}");
}

#[test]
fn select_all_guidelines_selects_them_in_red_and_delete_removes_them() {
    let mut app = E2e::new();
    app.click("Rulers");
    for x in [300.0, 500.0] {
        let (a, b) = (top_ruler(&app, x), app.at(x, 200.0 + x));
        app.drag_screen(a, b);
    }
    let r = app.draw_rect(100.0, 100.0, 200.0, 150.0);
    assert_eq!(app.selection(), vec![r]);

    app.menu(&["Edit", "Select All", "Guidelines"]);
    assert_eq!(app.state().selected_guides(), vec![0, 1]);
    assert!(app.selection().is_empty(), "objects are deselected");
    let px = app.pixel(450.0, 500.0);
    assert!(px[0] > px[2] + 60, "a selected guideline is red: {px:?}");

    // Clicking one guideline selects only it; clicking empty space deselects.
    app.click_doc(450.0, 700.0);
    assert_eq!(app.state().selected_guides(), vec![1]);
    app.click_doc(450.0, 800.0);
    assert!(app.state().selected_guides().is_empty());

    app.menu(&["Edit", "Select All", "Guidelines"]);
    app.key(Key::Delete);
    assert!(guides(&app).is_empty(), "Delete removes the selected guidelines");
    assert_eq!(app.objects().len(), 1, "and leaves the objects");
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(guides(&app).len(), 2, "undo brings them back");
}
