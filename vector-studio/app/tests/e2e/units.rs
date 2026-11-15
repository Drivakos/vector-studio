//! Units: the 13 units, the Units lists in the property bar and the Properties docker.

use vs_core::Unit;

use crate::support::E2e;

#[test]
fn every_unit_is_offered_and_meters_drive_the_fields() {
    let mut app = E2e::new();
    app.click("Units (property bar)");
    for unit in Unit::ALL {
        assert!(app.exists(unit.label()), "{} is in the list", unit.label());
    }
    app.click("meters");
    assert_eq!(app.doc().display_unit, Unit::Meters);
    // A4 is 0.21 m wide.
    assert!(app.value("Page width").starts_with("0.21"), "{}", app.value("Page width"));
    app.fill("Page width", "0.5");
    let w = app.doc().active_page().size.width;
    assert!((w - Unit::Meters.to_points(0.5)).abs() < 0.01, "{w}");
}

#[test]
fn document_units_in_the_properties_docker() {
    let mut app = E2e::new();
    app.docker("Properties");
    app.click("Units (document)");
    app.click("kilometers");
    assert_eq!(app.doc().display_unit, Unit::Kilometers);
    assert!(app.value("Page width (docker)").contains("0.00021"), "{}", app.value("Page width (docker)"));
}
