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

View modes (View menu or the View Modes dropdown): Enhanced draws fills, outlines and bitmaps; Wireframe draws every outline as a one-pixel line, with no fills, bitmaps as frames and PowerClip contents unclipped; Pixels draws the page as the bitmap it would export to at the Export window's resolution (300 dpi unless changed), magnified without smoothing once its pixels are bigger than the screen's. View > Full-Screen Preview (`F9`) fills the screen with the active page on black, in Enhanced view, without tools, selection, grid or guidelines; any key or click ends it and does nothing else. Not yet available (shown disabled): Snap To Pixels and the Normal view mode (drawing without anti-aliasing, which the renderer does not offer). Icons are Material Symbols (outline).

Phase 1 drawing:

| Area | What you can do |
|---|---|
| Pick | Click, Shift-click, marquee select. Drag to move (Cmd = one axis). Handles scale (corners keep proportions; Cmd frees them; Shift scales from the centre). Click a selected object again for rotate handles (Cmd = 15° steps). Double-click an object to edit its nodes with the Shape tool |
| Shape | Drag nodes and Bézier handles, marquee nodes, double-click a segment to add a node or a node to delete it, Delete removes nodes. Drag a rectangle corner to round it. Ellipse: drag its node inside for a pie, outside for an arc (Cmd = 15° steps). Polygon and star: drag an outer node to resize and turn every side (Cmd keeps the angle); a star's inner node sets its sharpness |
| Rectangle, Ellipse, Polygon, Star | Drag to draw; Cmd = equal sides, Shift = from the centre. Polygon sides and star points/sharpness in the property bar. A selected ellipse gets Whole ellipse / Pie / Arc and Change direction |
| Freehand | Drag; the stroke is fitted to Béziers; ending on the start closes it |
| Bézier, Pen | Click for corners, drag for smooth nodes, click the first node to close, Enter / Esc / click the last node to finish. Pen previews the next segment |
| 2-Point Line, Polyline, B-Spline | 2-Point Line: drag a straight line (Ctrl/Cmd keeps it to 15° steps). Polyline: click for straight segments. B-Spline: click control points; the curve is pulled towards them and starts and ends on the first and last. Both finish like the Pen tool (double-click, Enter, Esc, or the first node to close) |
| 3-point tools | 3-Point Curve: drag start to end, then click the point the curve passes through. 3-Point Rectangle: drag one side, then click to set the height. 3-Point Ellipse: drag one axis, then click to set the other. Rectangles and ellipses stay editable shapes, turned along the first edge. Esc drops the first edge |
| Spiral | Spiral tool (`A`): drag its box; an even spiral from the centre out to the edge, turning the number of times set on the property bar (4 by default). Ctrl/Cmd makes the box square |
| Text | Text tool (`F8`): click the page and type artistic text; Enter starts a new line, arrows/Home/End move the caret, Backspace/Delete remove, Esc or another tool finishes. Click text to edit it, or double-click it with the Pick tool. Property bar: font family and style (the bundled Ubuntu Light plus the system's fonts), size in points, left/centre/right alignment; with nothing selected they set up new text. New text is black with no outline. Text moves, scales, shapes and converts to curves like any object, and exports as outlines. Glyphs are placed by their advance widths only: no kerning, ligatures or complex scripts |
| Paragraph text | Text tool: drag a frame instead of clicking. Words wrap to the frame width and alignment works within it; lines that do not fit are hidden and the frame turns red. Frame width and height in the Properties docker. An empty frame stays when typing ends. Text > Convert to Artistic/Paragraph Text (`Cmd+F8`) switches either way without moving the text: wrapped lines become line breaks. Resizing with the Pick tool scales the text; it does not reflow |
| Edit Text | Text > Edit Text… (`Cmd+Shift+T`) with one text object selected: its words in a window, applied with OK (one undo step) or left as they were with Cancel. Works for artistic, paragraph and path text; editing a page number this way makes it plain text |
| Find and Replace | Edit > Find and Replace… (`Cmd+F`) opens the Find and Replace docker (also under Window > Dockers). Find next (or Enter in the Find field) selects the next text object containing the words, on to the following pages and round to the first; Match case is optional. Replace changes the selected match and finds the next; Replace all changes every match in one undo step. Text in groups and PowerClips is found (the group or frame is selected); locked objects, hidden or locked layers and page numbers are left alone |
| Fit Text to Path | Select one artistic text and one shape or curve, then Text > Fit Text to Path: each glyph sits on the outline, turned to follow it, from the start, middle or end by the text's alignment; glyphs past the end are hidden. The text keeps a copy of the outline, so editing the shape afterwards does not move the text. Edit its words in the Properties docker. Text > Straighten Text makes it plain artistic text again, starting where it started |
| Grid, guidelines, snapping | View toolbar or View menu: Grid (every 5 mm by default; spacing in the Properties docker with nothing selected, thinned out when zoomed far out) and Guidelines. Drag from a ruler to add a guideline; drag one with the Pick tool to move it, back onto a ruler to remove it. Guidelines belong to the page, are saved and undoable. Snap To (View menu or toolbar magnet): Document Grid, Guidelines (on by default), Objects (edges and centres), Page (edges and centre). New shapes, Bézier/Pen nodes, text, moved objects (by their edges and centre), scale handles and Shape-tool nodes snap within 6 screen points, per axis. The grid and guidelines snap only while shown. Which targets are on by default and the 5 mm grid are this app's choice |
| Fountain fills, transparency, eyedroppers | Properties docker and the Interactive Fill tool (`G`) property bar: No fill, Uniform fill (colour) or Fountain fill (linear or elliptical, start and end colours). With the Interactive Fill tool, click an object and drag across it to place a fountain; drag its ends to move them. Fountain fills live in the object's own space, so they move, scale and turn with it, and export to SVG, PNG and PDF. Transparency (0–100 %, the whole object) in the Properties docker and the Transparency tool's property bar. Color Eyedropper: click to pick a colour (fill, gradient colour at that point, outline, or the white page), then click a fill or outline to apply it. Attributes Eyedropper: pick an object's fill, outline and transparency, then click objects to apply them. Fountain fills have two colours; more stops, conical/rectangular fountains, patterns, textures, mesh fills and fountain transparency are not built |
| Fill and outline | Palette: click = fill, right-click = outline, crossed swatch removes. Property bar (with a selection): outline width with presets (None, Hairline, 0.25–36 pt) and line style. Properties docker: fill colour; outline width, colour, line style (solid, dotted, dashed, long dash, dash dot, dash dot dot), corners (miter, round, bevel), line caps (butt, round, square), behind fill, scale with object; corner radius, sides, points, sharpness, name. Edits to several objects change only the setting you touched. Fills use the even-odd rule on the canvas and in every export |
| Group and lock | Group (`Cmd+G`), Ungroup (`Cmd+U`), Ungroup All, Lock, Unlock, Unlock All: title bar buttons and the Object menu. A group moves, scales and takes colours as one; SVG export writes it as `<g>`. Locked objects can be selected (padlocks instead of handles) but not moved, edited, restyled, grouped or deleted |
| Units | 13 units: inches, millimeters, picas, points, pixels (96 dpi), ciceros, didots, feet, yards, miles, centimeters, meters, kilometers. Steppers move about 1 mm in any unit |
| Transform | Property bar and Transform docker: X, Y, W, H, rotate, mirror |
| Shaping | With several objects selected, the property bar and Object > Shaping have Weld, Trim, Intersect, Simplify, Front Minus Back, Back Minus Front and Boundary. The result takes the target's fill and outline: the last object selected, or the bottom-most after a marquee. Curves stay curves. Trim and Intersect keep the other objects; Boundary adds an unfilled outline around the selection. Open curves can't be shaped |
| Combine, Break Apart | Combine (`Cmd+L`) joins objects into one curve styled like the target; where its paths overlap there are holes. Break Apart (`Cmd+K`) makes each path its own object |
| Join Curves, Convert Outline | Join Curves (Object menu) merges the selected objects into one curve and links open ends less than 1 mm apart; they meet halfway, and a path whose own ends meet closes. Convert Outline to Object (`Cmd+Shift+Q`) turns each outline into a filled curve of its colour, above the object (below for Behind fill); the object keeps its fill, or goes if it had none |
| Crop, Knife, Eraser | Crop: drag the area to keep, then Enter, a double-click inside it or the property bar's Crop button (Esc clears it); it cuts the selected objects, or everything on the page when nothing is selected, and removes what lies wholly outside. Knife: drag a line across objects to cut each one it crosses into two objects, both selected. Eraser (`X`): drag to remove a round stroke from the selected objects (all objects it touches when nothing is selected); size in the property bar, default 5 mm. Closed shapes are cut with the boolean operations and open curves are split where they cross, so curves stay curves. Bitmaps are kept whole (Crop removes them only when wholly outside); Knife leaves groups and bitmaps alone |
| Arrange | Align (L R T B E C, P = centre to page): one object aligns to the page, several to the last selected. Distribute (three or more objects; Align and Distribute docker and Object menu): left, centres, right or spacing, horizontally or vertically, between the two outermost objects. Order: front, back, forward, backward, reverse order. Convert to Curves (Cmd+Q / Ctrl+Q), which also converts every member of a group |
| Edit | Undo/redo (every finished gesture is one step), cut/copy/paste (copy also puts the selection on the system clipboard as SVG, so it pastes into other apps). Edit > Paste Special… reads the system clipboard and offers the ways it can come in: Vector Studio objects (this app's last copy, where it was), SVG drawing (as curves, centred on the page and scaled down to fit) or Plain text (as artistic text, centred on the page); duplicate (offset set in the property bar), delete, select all, arrow nudge (distance set in the property bar, default 1 mm; Shift ×10). Step and Repeat docker (`Cmd+Shift+D`): number of copies, and for each direction an offset or a spacing between objects. Copy Properties From: choose outline pen, outline colour, fill and text properties, then click the object to copy from (Esc cancels). Select All › Nodes (or `Cmd+A` with the Shape tool) selects every node of the selected curve |
| Repeat | Edit > Repeat (`Cmd+R`) does the last move, scale, rotation, mirror or nudge again to the current selection, about its centre, or runs the last selection command again (Duplicate, Align, Distribute, Order, Convert to Curves, Convert Outline, shaping, Combine, Break Apart, Group, Lock, Unlock). Typing values into position and size fields is not repeated |
| Clone | Edit > Clone makes a linked copy, placed like Duplicate. A clone follows its master's shape, fill, outline and transparency as they change, but keeps its own position, size and rotation. Changing the clone's own shape or style, or deleting the master, makes it an ordinary object. Groups and PowerClips cannot be cloned. The link is saved with the drawing |
| Objects docker | Every page's objects top-first; click to select, Shift/Cmd-click to add, double-click to rename, order and delete buttons |
| Layers | Every page has layers (Layer 1 to start), listed in the Objects docker with their objects. Click a layer to draw on it; eye and lock toggles hide it (not drawn, exported, hit or snapped to) or lock it (drawn, not selectable). Docker buttons: New layer, Delete layer (with its objects; the last layer stays), Move layer up/down, Move to active layer. Double-click a layer to rename it. Files without layers open with one |
| Pages | Insert, duplicate and delete pages from the page navigator, the Pages docker or the Layout menu. Layout > Rename Page… renames the active page in the Pages docker (or double-click a page there). Layout > Go to Page… jumps to a page by number. Layout > Insert Page Number adds the page's number as artistic text centred near the bottom edge, in the font set for new text; it renumbers itself when pages are added, removed or moved, and typing into it makes it plain text. Layout > Page Background…: No background (white paper) or a solid colour for every page (colour button or hex), shown on the canvas and picked by the Color Eyedropper; with Print and export background on (the default) it comes out under the objects in SVG, PNG and PDF. Background changes are undoable and count as unsaved changes |
| PowerClip | Object > Clip Frame > Place Inside Frame…, then click a closed shape: the selection is placed inside it, where it was, and shows only within its outline (fill under the contents, outline over them) on screen and in SVG, PNG and PDF. Moving or scaling the frame moves its contents. Edit Contents takes them out to edit; Finish editing PowerClip on the property bar puts them back. Extract Contents puts them back on the page. Esc cancels the pick. Not yet: Create Empty Frame |
| Files | New, Open, Save, Save As (`.vstudio`), unsaved-changes prompt on New/Open/close, `•` in the title when unsaved. Close (`Cmd+W`) and Close All ask to save changes, then start a new drawing (one drawing is open at a time). Revert asks, then reloads the saved file; it is available once a saved drawing has changes. Export: SVG (current page), PNG (current page, dpi, transparent or white), PDF (all pages). Import (`Cmd+I`), centred on the page, selected, and scaled down to fit when larger: SVG as editable curves (several shapes come in as a group; shapes, groups, solid and fountain fills, outlines and opacity are kept; text, embedded bitmaps, patterns, clipping, masks and filters are not yet); PNG and JPEG as bitmaps at 96 pixels per inch. Bitmaps keep the file's own bytes, move, scale, rotate and take transparency, and export to SVG, PNG and PDF. File > Document Properties…: title, subject, author, copyright, keywords and notes, saved in the drawing and undoable; all but the notes are written to exported SVG (title, description and Dublin Core metadata) and PDF (document information). File > Print… (`Cmd+P`): printer (the system default first), copies, and all pages, the current page or a range like `1-3, 5`; the pages go to the printer as a PDF through the system print queue (CUPS `lp`), with the page background when it is set to print. There is no print preview, scaling, tiling or colour separation |

## Tests

`cargo test --workspace` runs about 235 tests. The 93 end-to-end tests drive the whole app through its window on a real GPU. Every feature and change ships with one. See [TESTING.md](TESTING.md).

## Shortcuts

Tools: `F10` Shape, `Z` Zoom, `H` Pan, `F5` Freehand, `F6` Rectangle, `F7` Ellipse, `Y` Polygon, `A` Spiral, `F8` Text, `Esc` finishes a path, then clears the selection, then picks the Pick tool.
Edit: `Cmd+Z` / `Cmd+Shift+Z`, `Cmd+X/C/V`, `Cmd+D` duplicate, `Cmd+A` select all (all nodes with the Shape tool), `Cmd+Shift+D` Step and Repeat, `Delete`/`Backspace`.
Arrange: `Cmd+L` combine, `Cmd+K` break apart, `L R T B E C P` align, `Shift+PageUp/PageDown` to front/back, `Cmd+PageUp/PageDown` forward/back one.
View: `Cmd+=` / `Cmd+-` zoom, `Shift+F4` zoom to page, `Cmd+1` actual size. `PageUp`/`PageDown` change page.

`Cmd+Q` is Convert to Curves, so Quit (in the Vector Studio menu) has no shortcut.
