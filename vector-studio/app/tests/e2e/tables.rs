//! Tables: the Table tool, typing into cells, the Table menu's inserts, merges and
//! splits, cell fills, Create New Table and the conversions between text and tables.

use egui::{Key, Modifiers};
use egui_material_icons::icons::ICON_TABLE;
use vs_core::{Cells, Shape};
use vector_studio::toolbox::Tool;

use crate::support::{E2e, temp_dir};

fn grid(app: &E2e) -> vs_core::Table {
    app.objects()[0].table().expect("a table").clone()
}

fn cell_text(app: &E2e, row: usize, col: usize) -> String {
    let obj = &app.objects()[0];
    let k = obj.table().unwrap().cell_at(row, col).unwrap();
    match &obj.cell_text(k).unwrap().shape {
        Shape::Text(t) => t.content.clone(),
        _ => unreachable!(),
    }
}

fn picked(app: &E2e) -> Option<Cells> {
    vector_studio::table_tool::picked(app.state()).map(|p| p.block)
}

fn is_red(px: [u8; 4]) -> bool {
    px[0] > 200 && px[1] < 60 && px[2] < 60
}

#[test]
fn draw_a_table_type_into_it_and_change_its_grid() {
    let mut app = E2e::new();
    app.flyout_tool("Text", &format!("{}Table", <&str>::from(ICON_TABLE)));
    assert_eq!(app.state().tool, Tool::Table);
    assert_eq!(app.value("Table rows"), "3");
    assert_eq!(app.value("Table columns"), "3");

    // A drag draws a 3 × 3 table: columns 100 wide, rows 40 high.
    app.drag_doc((100.0, 100.0), (400.0, 220.0));
    assert_eq!(app.objects().len(), 1);
    let t = grid(&app);
    assert_eq!((t.rows.len(), t.columns.len()), (3, 3));
    assert_eq!(app.selection(), vec![app.ids()[0]]);
    assert_eq!(app.objects()[0].label(), "Table");

    // A click picks a cell; a second click on it types into it.
    app.click_doc(250.0, 160.0);
    assert_eq!(picked(&app), Some(Cells::one(1, 1)));
    app.click_doc(250.0, 160.0);
    assert!(app.state().table.editing.is_some());
    app.type_text("Hi");
    app.key(Key::Escape);
    assert_eq!(cell_text(&app, 1, 1), "Hi");
    assert_eq!(app.objects().len(), 1, "the text went back into the cell");
    assert_eq!(app.selection(), vec![app.ids()[0]]);

    // The Text tool types into the cell it clicks.
    app.key(Key::F8);
    app.click_doc(150.0, 120.0);
    app.type_text("A");
    app.key(Key::Escape);
    assert_eq!(cell_text(&app, 0, 0), "A");

    // Double-clicking a table with the Pick tool picks a cell with the Table tool.
    app.pick_tool();
    app.double_click_doc(350.0, 200.0);
    assert_eq!(app.state().tool, Tool::Table);
    assert_eq!(picked(&app), Some(Cells::one(2, 2)));

    // Insert > Row Below and Column Right add a row and a column; the new ones are picked.
    app.menu(&["Table", "Insert", "Row Below"]);
    assert_eq!(grid(&app).rows.len(), 4);
    assert_eq!(picked(&app), Some(Cells { row: 3, col: 0, rows: 1, cols: 3 }));
    assert!((app.bounds(0).y1 - 260.0).abs() < 0.5, "{:?}", app.bounds(0));
    app.click_doc(350.0, 120.0);
    app.menu(&["Table", "Insert", "Column Right"]);
    assert!(grid(&app).columns.iter().all(|w| (w - 100.0).abs() < 0.01), "{:?}", grid(&app).columns);
    assert_eq!(picked(&app), Some(Cells { row: 0, col: 3, rows: 4, cols: 1 }));
    assert!((app.bounds(0).x1 - 500.0).abs() < 0.5);

    // Shift+click stretches the pick; a palette colour fills just those cells.
    app.click_doc(150.0, 160.0);
    app.hold(Modifiers::SHIFT);
    app.click_doc(250.0, 200.0);
    app.release_mods();
    let block = Cells { row: 1, col: 0, rows: 2, cols: 2 };
    assert_eq!(picked(&app), Some(block));
    app.click("Swatch Red");

    // Merge Cells keeps the texts; Split into Columns cuts the merged cell again.
    app.menu(&["Table", "Merge Cells"]);
    assert_eq!(grid(&app).cells.len(), 13);
    assert_eq!(picked(&app), Some(block));
    assert_eq!(cell_text(&app, 1, 0), "Hi");
    app.menu(&["Table", "Split into Columns…"]);
    app.fill("Number of parts (Split Cells into Columns)", "2");
    app.click("OK (Split Cells into Columns)");
    let t = grid(&app);
    assert_eq!(t.cells.len(), 14);
    assert_eq!(t.cells[t.cell_at(2, 1).unwrap()], Cells { row: 1, col: 1, rows: 2, cols: 1 });
    app.shortcut(Modifiers::COMMAND, Key::Z);
    assert_eq!(grid(&app).cells.len(), 13, "undo merges them again");

    // Only the picked cells were filled, under the text.
    app.pick_tool();
    assert!(is_red(app.pixel(150.0, 205.0)));
    assert!(!is_red(app.pixel(350.0, 120.0)));
    assert!(!is_red(app.pixel(150.0, 240.0)));

    // The table is saved with the drawing.
    let saved = temp_dir("tables").join("table.vstudio");
    vs_io::save_document(app.doc(), &saved).unwrap();
    assert_eq!(vs_io::open_document(&saved).unwrap().active_page().objects[0].table(), Some(&grid(&app)));

    // Convert Table to Text puts tabs between cells, a line per row…
    app.menu(&["Table", "Convert Table to Text…"]);
    app.click("Tabs (Convert Table to Text)");
    app.click("OK (Convert Table to Text)");
    let Shape::Text(text) = &app.objects()[0].shape else { panic!("text") };
    assert_eq!(text.content, "A\t\t\t\nHi\t\t\t\n\t\t\t\n\t\t\t");
    assert!(text.is_paragraph());
    // …and Convert Text to Table makes a table of it again.
    app.menu(&["Table", "Convert Text to Table…"]);
    app.click("User defined (Convert Text to Table)");
    assert!(!app.is_enabled("OK (Convert Text to Table)"), "no separator typed yet");
    app.click("Tabs (Convert Text to Table)");
    app.click("OK (Convert Text to Table)");
    let t = grid(&app);
    assert_eq!((t.rows.len(), t.columns.len()), (4, 4));
    assert_eq!(cell_text(&app, 1, 0), "Hi");

    // Create New Table… makes one in the middle of the page.
    app.menu(&["Table", "Create New Table…"]);
    app.fill("Rows (Create New Table)", "2");
    app.fill("Columns (Create New Table)", "4");
    app.click("OK (Create New Table)");
    assert_eq!(app.objects().len(), 2);
    let obj = &app.objects()[1];
    let t = obj.table().unwrap();
    assert_eq!((t.rows.len(), t.columns.len()), (2, 4));
    assert_eq!(app.selection(), vec![obj.id]);
    let page = app.doc().active_page().bounds().center();
    assert!((obj.world_bounds().center() - page).hypot() < 1.0);
}
