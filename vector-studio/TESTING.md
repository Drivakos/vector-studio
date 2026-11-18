# Testing

**Rule: every feature and every change ships with an end-to-end test** that uses it the way a person does, through the window. Unit tests in the crates come on top, not instead.

## Kinds of test

| Kind | Where | What it proves |
|---|---|---|
| End-to-end | `app/tests/e2e/` | The whole app works: real title bar, property bar, toolbox, dockers, palette, menus and Vello canvas on a real GPU device, driven by pointer and keyboard events |
| App unit | `app/src/*.rs` (`#[cfg(test)]`) | Tool gestures on a bare canvas, layout file, command table |
| Core, io, render | `crates/*/src` | Document model, geometry, file formats, export, rulers |

```bash
cargo test --workspace            # everything
cargo test -p vector-studio --test e2e            # end-to-end only
cargo test -p vector-studio --test e2e -- groups  # one area
```

The end-to-end tests need a GPU (Metal on macOS). They run in about 20 seconds.

## How the end-to-end tests work

`app/tests/e2e/support.rs` starts `VectorStudioApp` in [egui_kittest](https://docs.rs/egui_kittest) with its wgpu renderer, at 1440 × 900 and 60 frames per second of simulated time.

- **Widgets are found by accessible name**: the name a screen reader reads. Custom-painted widgets report one through `app/src/a11y.rs`. kittest also fails a test if any input widget has no name.
- **Canvas input is in document points**: `click_doc`, `drag_doc`, `stroke_doc` convert to window positions through the live viewport.
- **Rendering is checked on real pixels**: `pixel(x, y)` renders the frame on the GPU and reads the colour at a document point.
- **Native dialogs are scripted**: `app.script` queues the paths and answers that file and message dialogs return, and records what was shown.
- **The native macOS menu is off in tests**, so the in-window menu bar is used: `menu(&["Object", "Group", "Ungroup All"])`.

### Naming rules for accessible names

Names must be unique on screen, or a test cannot tell widgets apart.

| Where | Name |
|---|---|
| Toolbox | The tool's name: `Rectangle` |
| Title bar | The tooltip: `Group (⌘G)`, `Mirror horizontally` |
| Property bar icon buttons | Tooltip + ` (property bar)`: `Rotate 90° left (property bar)` |
| Property bar fields | The tooltip: `X: object position`, `Page width` |
| Docker icon buttons | Tooltip + ` (docker)`: `To front (docker)` |
| Docker fields | Label + where: `Width (transform)`, `Page width (docker)` |
| Docker strip | Title + ` docker`: `Objects docker` |
| Objects docker rows | `Object` + label: `Object Rectangle` |
| Layer rows | `Layer` + name: `Layer Layer 1`; toggles `Visible: Layer 1`, `Locked: Layer 1` |
| Palette | `Swatch` + colour: `Swatch Red` |
| Page navigator | Tooltip, and `Page tab` + page name |
| Outline choices | Group + choice: `Corners Round`, `Caps Square` |
| Window buttons | Label + window title: `OK (Paste Special)`, `Go (Go to Page)` |

## Coverage

| Feature | Test module (`app/tests/e2e/`) |
|---|---|
| Rectangle, Ellipse, Polygon, Star, Freehand, Bézier, Pen; Esc behaviour | `drawing.rs` |
| 2-Point Line (with 15° snap), Polyline, B-Spline, 3-Point Curve, 3-Point Rectangle and Ellipse (Esc drops the first edge), Spiral turns | `lines.rs` |
| Graph Paper (columns and rows on the property bar, one group of rectangles) | `drawing.rs` |
| Common Shapes (default triangle, shape picked on the property bar, arrow direction) | `drawing.rs` |
| Pick (select, Shift, marquee, move, Cmd axis, scale, Shift from centre, rotate, double-click to Shape tool); Shape (nodes, handles, node marquee, rounded corners) | `editing.rs` |
| Paragraph text resized with a Pick handle and the W field: the frame grows, the glyphs keep their size, fewer lines, undo | `text.rs` |
| Freehand Pick: a loop selects only what lies wholly inside it (not its bounding box), Shift adds, an empty loop clears, a click still picks | `editing.rs` |
| Free Transform: rotation, reflection, scale and skew about the press point from the property bar's modes, undo, a click deselects or selects | `transform.rs` |
| Palette fill and outline, outline width / style / corners / caps / behind fill / scale with object, multi-object edits, behind fill on the GPU canvas | `style.rs` |
| X, Y, W, H, rotate, mirror (property bar, toolbar, Transform docker) | `transform.rs` |
| Align keys and menus, Align docker, Distribute (docker and menu), order keys / toolbar / docker, Reverse Order, Convert to Curves | `arrange.rs` |
| Cmd+Q Convert to Curves (groups too); Shape tool on ellipses (pie, arc, Change direction), polygons and stars | `nodes.rs` |
| Undo / redo, copy (SVG on the clipboard), cut, paste, duplicate offset, delete, select all, nudge, Step and Repeat docker (spacing and offset), Copy Properties From (choices, pick, Esc), Select All Nodes (menu and Cmd+A in the Shape tool) | `edit.rs` |
| Select All Text | `text.rs` |
| Repeat: a drag, a rotate button, Duplicate | `edit.rs` |
| Paste Special: empty clipboard, our objects in place, SVG as curves centred, plain text as artistic text, undo (the system clipboard is scripted) | `edit.rs` |
| Clone: linked copy, master's fill shows on the clone (pixels), clone's own fill breaks the link | `edit.rs` |
| Objects docker: select, Shift-select, rename, order, delete | `objects.rs` |
| Layers: add, hide, lock, move objects, rename, delete, undo | `layers.rs` |
| PowerClip: place inside (Esc cancels), clipped pixels, edit and finish, move with frame, extract; Create Empty Frame (the X in pixels, drop an object inside, undo) | `powerclip.rs` |
| Symmetry: Create New Symmetry (mirror in pixels while editing and after a move, Lines on the property bar, Finish), Edit Symmetry, Remove Symmetry keeps the copies, undo; Symbols: New Symbol in place, Objects docker name, Symbol Manager (use count, Insert in the middle of the page, rename, Delete only when unused, Close), Edit Symbol (others wait, then all change on Finish), Revert to Objects | `symbols.rs` |
| Perspective: Add Perspective (Shape tool, corners dragged, narrowed top in pixels, missed where cut away), Copy Effect > Perspective From, Clear Effect and undo, Convert to Curves bakes it; Draw in Perspective (shapes under the pointer on the plane, one undo step, plane corners, folding refused, Text tool flat, off again) | `perspective.rs` |
| Overprint Fill and Overprint Outline (menu state, groups, undo, saved), Simulate Overprints on the GPU canvas, Proof Colors on vector colours and bitmaps | `proofing.rs` |
| Tables: Table tool (drag, property bar), picking a cell and typing into it (Table tool and Text tool), double-click with the Pick tool, Insert Row Below and Column Right, Shift+click block, palette fill of picked cells (pixels), Merge Cells, Split into Columns… and undo, saved with the drawing, Convert Table to Text and Text to Table (separator choices), Create New Table… | `tables.rs` |
| Page navigator, Pages docker, page size / orientation / width, Layout menu, PageUp / PageDown | `pages.rs` |
| Rename Page (Layout menu, in the Pages docker), Go to Page (window, limited to the page count, Cancel), Insert Page Number (placement, renumbering, typing makes it plain text), Page Background (canvas pixels, SVG with and without Print and export, undo steps) | `pages.rs` |
| Document Options (Cancel, OK, all pages, grid, nudge, units converting the fields) | `pages.rs` |
| Save, Save As, Open, command-line file, damaged file, unsaved-changes prompt, Export SVG / PNG / PDF, Import SVG (Cmd+I and menu, placement, undo, bad file), Import PNG (bitmap drawn on the canvas, selection, fit to page) | `files.rs` |
| Revert (cancel, confirm, only after changes to a saved file), Close and Close All (unsaved prompt, new drawing) | `files.rs` |
| Document Properties (fields, SVG metadata, saved in the drawing, undo) | `files.rs` |
| Save as Template (name, replace prompt), New from Template (list, Open, Browse…, untitled result) | `files.rs` |
| Export For Web (72 dpi PNG, pixel size, extension added), Print-Ready PDF (trim and bleed boxes, cancel keeps the window) | `files.rs` |
| Print (no printer, default printer, copies, page range and current page, the PDF sent; the print queue is scripted) | `files.rs` |
| Print Merge (import errors and data, record stepping, a field at the caret or as new text, Remove Data and undo, data saved with the drawing, Print… of the merged pages, Merge to New Document: unsaved prompt, filled pages, unsaved result) | `print_merge.rs` |
| Group, Ungroup, Ungroup All, Lock, Unlock, Unlock All | `groups.rs` |
| Join Curves (Object menu) closing two nearly-touching paths, Convert Outline to Object (Cmd+Shift+Q) and how it renders | `curves.rs` |
| Crop tool (Enter, Esc, double-click on the selection, canvas result, undo), Knife tool, Virtual Segment Delete (the part between crossings, a miss, undo), Eraser tool (size from the property bar, canvas result) | `cutting.rs` |
| Shaping (Weld, Trim, Intersect, Simplify, Front/Back Minus, Boundary: property bar and Object > Shaping), the target rule (marquee vs Shift-click), Combine (Cmd+L) with even-odd holes, Break Apart (Cmd+K) | `shaping.rs` |
| Text tool: typing, lines, caret keys, plain keys not running shortcuts, Esc, one undo step, empty text removed, clicking and Pick double-click to edit, font size and alignment for new and selected text, move, save/open, Convert to Curves, character shifts with the Shape tool (undo, typing before them), Align to Baseline, Straighten Text clearing shifts | `text.rs` |
| Paragraph text: drag a frame, wrap, convert to artistic and back in place, frame height and overflow, empty frames stay | `text.rs` |
| Edit Text window: Cmd+Shift+T and the Text menu, Cancel, OK, undo | `text.rs` |
| Change Case (selected text, Cancel, while typing) | `text.rs` |
| Text Statistics (whole drawing, selected text, Close) | `text.rs` |
| Text Properties (brings the Properties docker forward) | `text.rs` |
| Show Nonprinting Characters (off by default, dot in a space, ¶ at the end) | `text.rs` |
| Columns (flow into the next column, gutter, no overflow, undo) | `text.rs` |
| Bullets and Numbering (bullet drawn, words indented, numbers, none, undo) | `text.rs` |
| Drop Cap (lines beside it, letter drawn large, off again) | `text.rs` |
| Tabs (Tab key types a tab, half-inch default stop, Add tab and set its position, glyph drawn past the stop, Delete, undo) | `text.rs` |
| Glyphs (Insert off until a character is picked, picked cell, new text centred on the page, double-click types at the caret, window stays open) | `text.rs` |
| Find and Replace: Cmd+F, Enter, Find next across pages and round, Match case, not found, Replace one, Replace all, undo | `find.rs` |
| Fit Text to Path: along an ellipse, pixels on the curve, edit in Properties, Straighten Text | `text.rs` |
| Grid (toolbar, menu, drawn on the canvas, spacing in the docker), guidelines (dragged from both rulers, drawn, moved with Pick, removed on the ruler, undo, hidden, Select All Guidelines, click and Shift-click selection, Delete), snapping to pixels, grid, guidelines, objects and page, alignment guides (move and draw), dynamic guides (draw), Snap To on/off | `snapping.rs` |
| Fountain fill from the docker (linear, elliptical, rendered colours), uniform / no fill, transparency (docker and Transparency tool, rendered), Interactive Fill drag and end handles, property bar fill type, fill moving with the object, Color Eyedropper pick/apply to fill and outline, Attributes Eyedropper | `fills.rs` |
| Smart Fill (overlap of two shapes, fill colour drawn, outside does nothing, outline option) | `fills.rs` |
| Smooth (corner rounded off under the brush, far corner kept, fitted back into curves, undo) Roughen (picks the object under the stroke, spikes all along the stroke and nowhere else, spike size), Smudge (stroke out across an edge carries it along, brush size only on the property bar), Smear (pulled out to the pointer, one undo step, pushed in for an indent), Twirl (held still: counterclockwise swirl under the brush only, one undo step, Clockwise) and Attract and Repel (pulled up towards the brush and not past it, Repel pushes the edge inwards) | `brushes.rs` |
| Parallel Dimension (line alongside at the pulled distance, label in the display unit, one undo step) Horizontal or Vertical Dimension (above: horizontal, beside: vertical, Esc after the drag), Angular Dimension (90° inside the arms, 270° with the arc outside, one undo step), Segment Dimension (a click off every segment does nothing, bottom and right sides of a rectangle) and 3-Point Callout (leader points, text typed past its end and level with it, right-aligned when bent back left, leader kept without text) | `dimensions.rs` |
| Insert QR Code (size, centred, finder square pixels, undo) and Insert Barcode (Code 128 width and height, text under the bars, refused characters, without text) | `codes.rs` |
| Convert to Bitmap (two objects into one bitmap in place, size at a resolution, white background, undo), Resample (proportions kept, then free, place kept), bitmap effects (Grayscale, Brightness, Gaussian Blur, Pixelate, Add Noise, Unsharp Mask, Cancel, undo), Tone Curve (preview follows the settings, Green channel only, node dragged on the curve, level through the curve, undo), Bitmap Color Mask (colour picked on the preview, hidden, outline kept, typed colour, Show colors), Straighten Image (same pixel size and place, turned within its frame), Image Adjustment Lab (eight settings start at 0, preview, result as the core effect, undo), Remove Background then Crop Bitmap (white edge gone, red kept, place kept until cropped, cropped to the red at the same resolution, nothing more to crop, undo) | `bitmaps.rs` |
| Trace Bitmap: items need a selected bitmap; Quick Trace (grouped curves just above the bitmap, selected, one curve per colour over its block, background left out, one undo step); Outline Trace window (starts at Clipart, preview follows the settings, Type of image brings its colours, Colors typed, Delete original image, the canvas shows the curves, settings remembered); Cancel traces nothing; Quick Trace uses the Outline Trace settings last confirmed (background kept); Centerline Trace (Technical illustration: one unfilled line down the middle of a stroke, as wide as the stroke, no Remove object overlap); a blank bitmap says Nothing to trace | `tracing.rs` |
| The 13 units in the property bar and the Properties docker | `units.rs` |
| Zoom (keys, toolbar, Zoom tool), Pan tool, rulers, dockers, palette, status bar, Normal view (no anti-aliased pixels, updates on edits, View Modes list) | `view.rs` |
| Pixels view mode: one colour per export-resolution pixel on screen, back to Enhanced | `view.rs` |
| Full-Screen Preview: F9 and the View menu, page pixels on black, no chrome, a key or a click ends it without acting | `view.rs` |
| Page Sorter View: thumbnails (pixels), click to pick, drag to move a page, undo, double-click back to drawing | `pages.rs` |
| View modes: Wireframe from the View menu (pixels: no fill, grey outline), Enhanced from the View Modes dropdown | `view.rs` |

Not covered end to end:

- **Window title:** the title bar is drawn as text without an accessible node. The tests check the unsaved state through `history.is_dirty()` instead.
- **Restoring the layout on restart:** eframe storage is not available in the test harness. Unit tests in `dockers.rs` cover it.
- **The native macOS menu bar:** it is a system menu outside the egui window. It uses the same command table as the in-window menu that the tests drive. So its Quit item (no shortcut, which leaves Cmd+Q to Convert to Curves) is not covered by a test.

## Adding a feature

1. Give every new control an accessible name that follows the table above.
2. Add an end-to-end test that uses the feature the way a person would, in the module for its area.
3. Add a row to the Coverage table.
4. Run `cargo test --workspace` and `cargo clippy --workspace --all-targets`.
