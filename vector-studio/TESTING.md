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

## Coverage

| Feature | Test module (`app/tests/e2e/`) |
|---|---|
| Rectangle, Ellipse, Polygon, Star, Freehand, Bézier, Pen; Esc behaviour | `drawing.rs` |
| Pick (select, Shift, marquee, move, Cmd axis, scale, Shift from centre, rotate, double-click to Shape tool); Shape (nodes, handles, node marquee, rounded corners) | `editing.rs` |
| Palette fill and outline, outline width / style / corners / caps / behind fill / scale with object, multi-object edits, behind fill on the GPU canvas | `style.rs` |
| X, Y, W, H, rotate, mirror (property bar, toolbar, Transform docker) | `transform.rs` |
| Align keys and menus, Align docker, Distribute (docker and menu), order keys / toolbar / docker, Reverse Order, Convert to Curves | `arrange.rs` |
| Cmd+Q Convert to Curves (groups too); Shape tool on ellipses (pie, arc, Change direction), polygons and stars | `nodes.rs` |
| Undo / redo, copy (SVG on the clipboard), cut, paste, duplicate offset, delete, select all, nudge, Step and Repeat docker (spacing and offset), Copy Properties From (choices, pick, Esc), Select All Nodes (menu and Cmd+A in the Shape tool) | `edit.rs` |
| Objects docker: select, Shift-select, rename, order, delete | `objects.rs` |
| Layers: add, hide, lock, move objects, rename, delete, undo | `layers.rs` |
| PowerClip: place inside (Esc cancels), clipped pixels, edit and finish, move with frame, extract | `powerclip.rs` |
| Page navigator, Pages docker, page size / orientation / width, Layout menu, PageUp / PageDown | `pages.rs` |
| Save, Save As, Open, command-line file, damaged file, unsaved-changes prompt, Export SVG / PNG / PDF, Import SVG (Cmd+I and menu, placement, undo, bad file), Import PNG (bitmap drawn on the canvas, selection, fit to page) | `files.rs` |
| Revert (cancel, confirm, only after changes to a saved file), Close and Close All (unsaved prompt, new drawing) | `files.rs` |
| Group, Ungroup, Ungroup All, Lock, Unlock, Unlock All | `groups.rs` |
| Join Curves (Object menu) closing two nearly-touching paths, Convert Outline to Object (Cmd+Shift+Q) and how it renders | `curves.rs` |
| Crop tool (Enter, Esc, double-click on the selection, canvas result, undo), Knife tool, Eraser tool (size from the property bar, canvas result) | `cutting.rs` |
| Shaping (Weld, Trim, Intersect, Simplify, Front/Back Minus, Boundary: property bar and Object > Shaping), the target rule (marquee vs Shift-click), Combine (Cmd+L) with even-odd holes, Break Apart (Cmd+K) | `shaping.rs` |
| Text tool: typing, lines, caret keys, plain keys not running shortcuts, Esc, one undo step, empty text removed, clicking and Pick double-click to edit, font size and alignment for new and selected text, move, save/open, Convert to Curves | `text.rs` |
| Paragraph text: drag a frame, wrap, convert to artistic and back in place, frame height and overflow, empty frames stay | `text.rs` |
| Fit Text to Path: along an ellipse, pixels on the curve, edit in Properties, Straighten Text | `text.rs` |
| Grid (toolbar, menu, drawn on the canvas, spacing in the docker), guidelines (dragged from both rulers, drawn, moved with Pick, removed on the ruler, undo, hidden), snapping to grid, guidelines, objects and page, Snap To on/off | `snapping.rs` |
| Fountain fill from the docker (linear, elliptical, rendered colours), uniform / no fill, transparency (docker and Transparency tool, rendered), Interactive Fill drag and end handles, property bar fill type, fill moving with the object, Color Eyedropper pick/apply to fill and outline, Attributes Eyedropper | `fills.rs` |
| The 13 units in the property bar and the Properties docker | `units.rs` |
| Zoom (keys, toolbar, Zoom tool), Pan tool, rulers, dockers, palette, status bar | `view.rs` |
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
