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
| `app` | eframe/egui shell: native menu (macOS), title bar and toolbar, property bar, toolbox, dockers, palette, GPU canvas |
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

Not yet available (shown disabled): Group, Ungroup, Lock, Unlock, Grid, Guidelines, Snap To options, view modes other than Enhanced. Icons are Material Symbols (outline).

Phase 1 drawing:

| Area | What you can do |
|---|---|
| Pick | Click, Shift-click, marquee select. Drag to move (Cmd = one axis). Handles scale (corners keep proportions; Cmd frees them; Shift scales from the centre). Click a selected object again for rotate handles (Cmd = 15° steps) |
| Shape | Drag nodes and Bézier handles, marquee nodes, double-click a segment to add a node or a node to delete it, Delete removes nodes. Drag a rectangle corner to round it |
| Rectangle, Ellipse, Polygon, Star | Drag to draw; Cmd = equal sides, Shift = from the centre. Polygon sides and star points/sharpness in the property bar |
| Freehand | Drag; the stroke is fitted to Béziers; ending on the start closes it |
| Bézier, Pen | Click for corners, drag for smooth nodes, click the first node to close, Enter / Esc / click the last node to finish. Pen previews the next segment |
| Fill and outline | Palette: click = fill, right-click = outline, crossed swatch removes. Properties docker: colours, outline width (0 = hairline), corner radius, sides, points, sharpness, name |
| Transform | Property bar and Transform docker: X, Y, W, H, rotate, mirror |
| Arrange | Align (L R T B E C, P = centre to page): one object aligns to the page, several to the last selected. Order: front, back, forward, backward. Convert to Curves |
| Edit | Undo/redo (every finished gesture is one step), cut/copy/paste (copy also puts SVG on the system clipboard), duplicate (offset set in the property bar), delete, select all, arrow nudge (distance set in the property bar, default 1 mm; Shift ×10) |
| Objects docker | Every page's objects top-first; click to select, Shift/Cmd-click to add, double-click to rename, order and delete buttons |
| Files | New, Open, Save, Save As (`.vstudio`), unsaved-changes prompt on New/Open/close, `•` in the title when unsaved. Export: SVG (current page), PNG (current page, dpi, transparent or white), PDF (all pages) |

## Shortcuts

Tools: `F10` Shape, `Z` Zoom, `H` Pan, `F5` Freehand, `F6` Rectangle, `F7` Ellipse, `Y` Polygon, `F8` Text, `Esc` finishes a path, then clears the selection, then picks the Pick tool.
Edit: `Cmd+Z` / `Cmd+Shift+Z`, `Cmd+X/C/V`, `Cmd+D` duplicate, `Cmd+A` select all, `Delete`/`Backspace`.
Arrange: `L R T B E C P` align, `Shift+PageUp/PageDown` to front/back, `Cmd+PageUp/PageDown` forward/back one.
View: `Cmd+=` / `Cmd+-` zoom, `Shift+F4` zoom to page, `Cmd+1` actual size. `PageUp`/`PageDown` change page.

On macOS, `Cmd+Q` stays Quit. Convert to Curves is in the Object menu and the property bar.
