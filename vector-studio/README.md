# Vector Studio

Native vector illustration app in Rust. Phase 1: draw, edit, save, reopen and export.

## Run

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cargo run -p vector-studio
cargo run -p vector-studio -- path/to/drawing.vstudio
cargo test --workspace
cargo run -p vs-io --example sample_logo -- /some/dir   # builds, saves, reopens and exports a sample
```

## Layout

| Crate | What it holds |
|---|---|
| `crates/core` (`vs-core`) | Document, pages, objects, transforms, hit-testing, align/order ops, node editing, undo history. No UI or GPU code |
| `crates/io` (`vs-io`) | Native `.vstudio` format (versioned JSON), SVG, PNG (resvg) and PDF (krilla) export |
| `crates/render` (`vs-render`) | Viewport maths, ruler ticks, Vello scene building |
| `app` | eframe/egui shell: native menu (macOS), title bar and toolbar, property bar, toolbox, dockers, palette, GPU canvas. A library (`src/lib.rs`) with a thin binary (`src/main.rs`), so the end-to-end tests can start it. File dialogs go through a `Dialogs` trait; custom widgets report accessible names (`src/a11y.rs`) |
| `app/tests/e2e` | End-to-end tests: the whole app driven through its window (see [TESTING.md](TESTING.md)) |
| `app/layout/workspace.json` | Every menu item. Ids without a `Command` render disabled |

## What works

Phase 0 shell: native macOS menu bar from `workspace.json`, toolbox (17 groups, 60 tools, flyouts), per-tool property bar, dockable dockers, Vello canvas with rulers, zoom and pan, pages, layout restore.

### Window layout

Dark theme, laid out as follows:

| Area | What it is |
|---|---|
| Title bar + toolbar | Drawn by the app under a transparent title bar. Items with captions: Zoom, View Modes, Group, Ungroup, Lock, Unlock, Alignment, Mirror, Arrange, View (Rulers, Grid, Guidelines), Snap To, and a Dockers toggle. AI Generate and Get More are left out (online services) |
| Property bar | Stacked fields with steppers. Pick tool, nothing selected: page size, W/H, orientation, units, nudge, duplicate distance |
| Toolbox | 17 groups with separators |
| Dockers | One docker shown at a time: header with close button and title, vertical icon strip to switch, ••• for the full list |
| Palette | Vertical, no-colour swatch on top, scroll chevron at the bottom |
| Canvas | Grey desk (#5a5a5a), white page without border or shadow; page navigator with + on both sides and page tabs |
| Off by default | Rulers and status bar; View toolbar and View menu turn them on |

Not yet available (shown disabled): Snap To Pixels, view modes other than Enhanced. Icons are Material Symbols (outline).

Phase 1 drawing:

| Area | What you can do |
|---|---|
| Pick | Click, Shift-click, marquee select. Drag to move (Cmd = one axis). Handles scale (corners keep proportions; Cmd frees them; Shift scales from the centre). Click a selected object again for rotate handles (Cmd = 15° steps). Double-click an object to edit its nodes with the Shape tool |
| Shape | Drag nodes and Bézier handles, marquee nodes, double-click a segment to add a node or a node to delete it, Delete removes nodes. Drag a rectangle corner to round it. Ellipse: drag its node inside for a pie, outside for an arc (Cmd = 15° steps). Polygon and star: drag an outer node to resize and turn every side (Cmd keeps the angle); a star's inner node sets its sharpness |
| Rectangle, Ellipse, Polygon, Star | Drag to draw; Cmd = equal sides, Shift = from the centre. Polygon sides and star points/sharpness in the property bar. A selected ellipse gets Whole ellipse / Pie / Arc and Change direction |
| Freehand | Drag; the stroke is fitted to Béziers; ending on the start closes it |
| Bézier, Pen | Click for corners, drag for smooth nodes, click the first node to close, Enter / Esc / click the last node to finish. Pen previews the next segment |
| Text | Text tool (`F8`): click the page and type artistic text; Enter starts a new line, arrows/Home/End move the caret, Backspace/Delete remove, Esc or another tool finishes. Click text to edit it, or double-click it with the Pick tool. Property bar: font family and style (the bundled Ubuntu Light plus the system's fonts), size in points, left/centre/right alignment; with nothing selected they set up new text. New text is black with no outline. Text moves, scales, shapes and converts to curves like any object, and exports as outlines. Glyphs are placed by their advance widths only: no kerning, ligatures or complex scripts |
| Grid, guidelines, snapping | View toolbar or View menu: Grid (every 5 mm by default; spacing in the Properties docker with nothing selected, thinned out when zoomed far out) and Guidelines. Drag from a ruler to add a guideline; drag one with the Pick tool to move it, back onto a ruler to remove it. Guidelines belong to the page, are saved and undoable. Snap To (View menu or toolbar magnet): Document Grid, Guidelines (on by default), Objects (edges and centres), Page (edges and centre). New shapes, Bézier/Pen nodes, text, moved objects (by their edges and centre), scale handles and Shape-tool nodes snap within 6 screen points, per axis. The grid and guidelines snap only while shown. Which targets are on by default and the 5 mm grid are this app's choice |
| Fountain fills, transparency, eyedroppers | Properties docker and the Interactive Fill tool (`G`) property bar: No fill, Uniform fill (colour) or Fountain fill (linear or elliptical, start and end colours). With the Interactive Fill tool, click an object and drag across it to place a fountain; drag its ends to move them. Fountain fills live in the object's own space, so they move, scale and turn with it, and export to SVG, PNG and PDF. Transparency (0–100 %, the whole object) in the Properties docker and the Transparency tool's property bar. Color Eyedropper: click to pick a colour (fill, gradient colour at that point, outline, or the white page), then click a fill or outline to apply it. Attributes Eyedropper: pick an object's fill, outline and transparency, then click objects to apply them. Fountain fills have two colours; more stops, conical/rectangular fountains, patterns, textures, mesh fills and fountain transparency are not built |
| Fill and outline | Palette: click = fill, right-click = outline, crossed swatch removes. Property bar (with a selection): outline width with presets (None, Hairline, 0.25–36 pt) and line style. Properties docker: fill colour; outline width, colour, line style (solid, dotted, dashed, long dash, dash dot, dash dot dot), corners (miter, round, bevel), line caps (butt, round, square), behind fill, scale with object; corner radius, sides, points, sharpness, name. Edits to several objects change only the setting you touched. Fills use the even-odd rule on the canvas and in every export |
| Group and lock | Group (`Cmd+G`), Ungroup (`Cmd+U`), Ungroup All, Lock, Unlock, Unlock All: title bar buttons and the Object menu. A group moves, scales and takes colours as one; SVG export writes it as `<g>`. Locked objects can be selected (padlocks instead of handles) but not moved, edited, restyled, grouped or deleted |
| Units | 13 units: inches, millimeters, picas, points, pixels (96 dpi), ciceros, didots, feet, yards, miles, centimeters, meters, kilometers. Steppers move about 1 mm in any unit |
| Transform | Property bar and Transform docker: X, Y, W, H, rotate, mirror |
| Shaping | With several objects selected, the property bar and Object > Shaping have Weld, Trim, Intersect, Simplify, Front Minus Back, Back Minus Front and Boundary. The result takes the target's fill and outline: the last object selected, or the bottom-most after a marquee. Curves stay curves. Trim and Intersect keep the other objects; Boundary adds an unfilled outline around the selection. Open curves can't be shaped |
| Combine, Break Apart | Combine (`Cmd+L`) joins objects into one curve styled like the target; where its paths overlap there are holes. Break Apart (`Cmd+K`) makes each path its own object |
| Join Curves, Convert Outline | Join Curves (Object menu) merges the selected objects into one curve and links open ends less than 1 mm apart; they meet halfway, and a path whose own ends meet closes. Convert Outline to Object (`Cmd+Shift+Q`) turns each outline into a filled curve of its colour, above the object (below for Behind fill); the object keeps its fill, or goes if it had none |
| Arrange | Align (L R T B E C, P = centre to page): one object aligns to the page, several to the last selected. Distribute (three or more objects; Align and Distribute docker and Object menu): left, centres, right or spacing, horizontally or vertically, between the two outermost objects. Order: front, back, forward, backward, reverse order. Convert to Curves (Cmd+Q / Ctrl+Q), which also converts every member of a group |
| Edit | Undo/redo (every finished gesture is one step), cut/copy/paste (copy also puts SVG on the system clipboard), duplicate (offset set in the property bar), delete, select all, arrow nudge (distance set in the property bar, default 1 mm; Shift ×10). Step and Repeat docker (`Cmd+Shift+D`): number of copies, and for each direction an offset or a spacing between objects. Copy Properties From: choose outline pen, outline colour, fill and text properties, then click the object to copy from (Esc cancels). Select All › Nodes (or `Cmd+A` with the Shape tool) selects every node of the selected curve |
| Objects docker | Every page's objects top-first; click to select, Shift/Cmd-click to add, double-click to rename, order and delete buttons |
| Files | New, Open, Save, Save As (`.vstudio`), unsaved-changes prompt on New/Open/close, `•` in the title when unsaved. Export: SVG (current page), PNG (current page, dpi, transparent or white), PDF (all pages). Import (`Cmd+I`), centred on the page, selected, and scaled down to fit when larger: SVG as editable curves (several shapes come in as a group; shapes, groups, solid and fountain fills, outlines and opacity are kept; text, embedded bitmaps, patterns, clipping, masks and filters are not yet); PNG and JPEG as bitmaps at 96 pixels per inch. Bitmaps keep the file's own bytes, move, scale, rotate and take transparency, and export to SVG, PNG and PDF |

## Tests

`cargo test --workspace` runs about 230 tests. The 89 end-to-end tests drive the whole app through its window on a real GPU. Every feature and change ships with one. See [TESTING.md](TESTING.md).

## Shortcuts

Tools: `F10` Shape, `Z` Zoom, `H` Pan, `F5` Freehand, `F6` Rectangle, `F7` Ellipse, `Y` Polygon, `F8` Text, `Esc` finishes a path, then clears the selection, then picks the Pick tool.
Edit: `Cmd+Z` / `Cmd+Shift+Z`, `Cmd+X/C/V`, `Cmd+D` duplicate, `Cmd+A` select all (all nodes with the Shape tool), `Cmd+Shift+D` Step and Repeat, `Delete`/`Backspace`.
Arrange: `Cmd+L` combine, `Cmd+K` break apart, `L R T B E C P` align, `Shift+PageUp/PageDown` to front/back, `Cmd+PageUp/PageDown` forward/back one.
View: `Cmd+=` / `Cmd+-` zoom, `Shift+F4` zoom to page, `Cmd+1` actual size. `PageUp`/`PageDown` change page.

`Cmd+Q` is Convert to Curves, so Quit (in the Vector Studio menu) has no shortcut.
