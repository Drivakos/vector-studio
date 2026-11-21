//! Drawing window: Vello-rendered page, rulers, zoom/pan, and page tabs.

use eframe::egui_wgpu;
use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use egui_material_icons::icons::*;
use vs_core::kurbo::{self, Point};
use vs_render::vello::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};
use vs_render::vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};
use vs_render::{CanvasColors, build_page_scene, ruler_ticks};

use crate::app::AppState;
use crate::commands::Command;
use crate::theme;
use crate::toolbox::Tool;

const RULER: f32 = 20.0;
const PAGE_TABS: f32 = theme::PAGE_NAV_HEIGHT;
const FIT_MARGIN: f64 = 24.0;

/// Owns the Vello renderer and the texture egui shows as the canvas.
pub struct GpuCanvas {
    render_state: egui_wgpu::RenderState,
    renderer: Renderer,
    scene: Scene,
    target: Option<Target>,
    colors: CanvasColors,
    normal: Option<NormalView>,
}

/// Normal view's objects, drawn on the CPU without anti-aliasing, kept until the page,
/// the view or the canvas size changes.
struct NormalView {
    page: vs_io::AliasedPage,
    key: ([f64; 6], [u32; 2], Option<u64>),
    image: Option<ImageData>,
}

struct Target {
    _texture: wgpu::Texture,
    /// Unorm view: Vello writes to it as a storage texture.
    storage_view: wgpu::TextureView,
    size: [u32; 2],
    id: egui::TextureId,
}

impl GpuCanvas {
    pub fn new(render_state: &egui_wgpu::RenderState) -> Result<Self, String> {
        let renderer = Renderer::new(
            &render_state.device,
            RendererOptions {
                antialiasing_support: AaSupport::area_only(),
                ..Default::default()
            },
        )
        .map_err(|e| format!("could not start the Vello renderer: {e}"))?;
        Ok(Self {
            render_state: render_state.clone(),
            renderer,
            scene: Scene::new(),
            target: None,
            colors: CanvasColors::default(),
            normal: None,
        })
    }

    fn ensure_target(&mut self, size: [u32; 2]) -> egui::TextureId {
        if let Some(t) = &self.target
            && t.size == size
        {
            return t.id;
        }
        let device = &self.render_state.device;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vector-studio canvas"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let storage_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Vello writes sRGB-encoded bytes and egui blends in gamma space, so egui must
        // sample the bytes as they are: a plain Unorm view, not an sRGB one (which would
        // decode to linear and darken every mid-tone).
        let sample_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut egui_renderer = self.render_state.renderer.write();
        let id = match self.target.take() {
            Some(old) => {
                egui_renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &sample_view,
                    wgpu::FilterMode::Nearest,
                    old.id,
                );
                old.id
            }
            None => egui_renderer.register_native_texture(device, &sample_view, wgpu::FilterMode::Nearest),
        };
        self.target = Some(Target { _texture: texture, storage_view, size, id });
        id
    }

    /// Render the active page as `view` shows it, for a canvas of `size` physical
    /// pixels. Returns the egui texture to show and where it goes, in logical points
    /// from the canvas corner.
    fn render(&mut self, state: &AppState, view: &View, size: [u32; 2], pixels_per_point: f64) -> Option<(egui::TextureId, Rect)> {
        if size[0] == 0 || size[1] == 0 {
            return None;
        }
        // Pixels view: draw the export-resolution bitmap's pixels, shown magnified.
        let grid = (view.look.mode == vs_render::ViewMode::Pixels)
            .then(|| vs_render::pixel_grid(&view.viewport, size, pixels_per_point, state.export.dpi))
            .flatten();
        let (viewport, ppp, size, place) = match grid {
            Some(g) => {
                let shown = vec2(g.size[0] as f32, g.size[1] as f32) * g.scale as f32;
                (g.viewport, 1.0, g.size, Rect::from_min_size(pos2(g.origin.x as f32, g.origin.y as f32), shown))
            }
            None => {
                let shown = vec2(size[0] as f32, size[1] as f32) / pixels_per_point as f32;
                (view.viewport, pixels_per_point, size, Rect::from_min_size(Pos2::ZERO, shown))
            }
        };
        let id = self.ensure_target(size);
        self.scene.reset();
        // A page background shows on screen whether or not it prints and exports.
        let colors = CanvasColors {
            page: state.doc.background.map_or(self.colors.page, |b| vs_render::to_color(view.look.color(b.color))),
            ..self.colors
        };
        let page = crate::symbol_menu::shown_page(state);
        build_page_scene(&mut self.scene, &page, &viewport, ppp, &colors, view.look);
        if view.look.mode == vs_render::ViewMode::Normal {
            let transform = kurbo::Affine::scale(ppp) * viewport.transform();
            if let Some(image) = self.normal_objects(&page, transform, size, view.look.proof) {
                let mut brush = ImageBrush::new(image);
                brush.sampler.quality = ImageQuality::Low;
                self.scene.draw_image(&brush, kurbo::Affine::IDENTITY);
            }
        } else {
            self.normal = None;
        }
        let target = self.target.as_ref()?;
        let params = RenderParams {
            base_color: view.pasteboard.unwrap_or(self.colors.pasteboard),
            width: size[0],
            height: size[1],
            antialiasing_method: AaConfig::Area,
        };
        if let Err(err) = self.renderer.render_to_texture(
            &self.render_state.device,
            &self.render_state.queue,
            &self.scene,
            &target.storage_view,
            &params,
        ) {
            log::error!("canvas render failed: {err}");
            return None;
        }
        Some((id, place))
    }
}

/// What a proof is made from: the profile file, intent and warning colour with how much
/// of the proofed colour shows through it.
type ProofKey = (std::path::PathBuf, vs_core::RenderingIntent, Option<(vs_core::Rgba, u8)>);

thread_local! {
    /// View > Proof Colors' soft proof, made again when its settings change.
    static PROOF: std::cell::RefCell<Option<(ProofKey, std::rc::Rc<vs_render::Proof>)>> = std::cell::RefCell::default();
    /// Whether the generic CMYK profile, the fallback for every other, can be read.
    static CAN_PROOF: bool = vs_render::Proof::system().is_some();
}

/// Whether View > Proof Colors can be used: the system's CMYK profile loaded.
pub fn can_proof() -> bool {
    CAN_PROOF.with(|c| *c)
}

/// The profile Proof Colors simulates: the one chosen in Color Proof Settings, else the
/// document's; the generic one when that can't be read.
pub fn proof_profile(state: &AppState) -> std::path::PathBuf {
    vs_render::resolve_profile(state.prefs.proof.profile.as_deref().or(state.doc.color.cmyk_profile.as_deref()))
}

/// The soft proof for the current Color Proof Settings.
pub fn proof(state: &AppState) -> Option<std::rc::Rc<vs_render::Proof>> {
    let p = &state.prefs.proof;
    let key: ProofKey = (proof_profile(state), p.intent, p.gamut_warning.then_some((p.warning_color, p.warning_transparency.min(100))));
    PROOF.with(|cell| {
        let mut cell = cell.borrow_mut();
        if let Some((k, proof)) = cell.as_ref()
            && *k == key
        {
            return Some(proof.clone());
        }
        let warning = key.2.map(|(c, t)| (c, f64::from(t) / 100.0));
        let proof = std::rc::Rc::new(vs_render::Proof::new(&key.0, key.1, warning)?);
        *cell = Some((key, proof.clone()));
        Some(proof)
    })
}

impl GpuCanvas {
    /// Normal view's picture of `page`'s objects: `size` pixels, `transform` mapping
    /// page points to them.
    fn normal_objects(&mut self, page: &vs_core::Page, transform: kurbo::Affine, size: [u32; 2], proof: Option<&vs_render::Proof>) -> Option<ImageData> {
        let key = (transform.as_coeffs(), size, proof.map(vs_render::Proof::id));
        if self.normal.as_ref().is_none_or(|n| !n.page.shows(page)) {
            let page = vs_io::AliasedPage::new(page).inspect_err(|e| log::error!("Normal view: {e}")).ok()?;
            self.normal = Some(NormalView { page, key, image: None });
        }
        let normal = self.normal.as_mut()?;
        if normal.image.is_none() || normal.key != key {
            normal.key = key;
            normal.image = normal.page.render(transform, size).map(|bytes| {
                let image = ImageData {
                    data: Blob::new(std::sync::Arc::new(bytes)),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width: size[0],
                    height: size[1],
                };
                proof.map_or_else(|| image.clone(), |p| p.image(&image))
            });
        }
        normal.image.clone()
    }
}

/// What the canvas shows: the page through `viewport`, drawn in `mode`, on the desk
/// colour unless `pasteboard` gives another.
struct View<'a> {
    viewport: vs_render::Viewport,
    look: vs_render::Look<'a>,
    pasteboard: Option<vs_render::vello::peniko::Color>,
}

/// Transient canvas interaction state (not persisted).
#[derive(Default)]
pub struct CanvasInteraction {
    zoom_marquee_start: Option<Pos2>,
}

pub fn canvas_pane_ui(
    ui: &mut egui::Ui,
    state: &mut AppState,
    gpu: &mut GpuCanvas,
    commands: &mut Vec<Command>,
) {
    let full = ui.available_rect_before_wrap();
    let ruler = if state.prefs.show_rulers { RULER } else { 0.0 };
    let canvas_rect = Rect::from_min_max(
        full.min + vec2(ruler, ruler),
        pos2(full.max.x, (full.max.y - PAGE_TABS).max(full.min.y + ruler)),
    );
    let tabs_rect = Rect::from_min_max(pos2(full.min.x, canvas_rect.max.y), full.max);
    ui.allocate_rect(full, Sense::hover());

    state.canvas_size = canvas_rect.size();
    state.canvas_rect = canvas_rect;
    if state.needs_fit && canvas_rect.width() > 10.0 && canvas_rect.height() > 10.0 {
        fit_page(state);
        state.needs_fit = false;
    }

    let response = ui.interact(canvas_rect, ui.scope_id().with("canvas"), Sense::click_and_drag());
    let panning = handle_input(ui, state, &response, canvas_rect);
    if !panning && !matches!(state.tool, Tool::Zoom | Tool::Pan) {
        crate::tools::tool_input(ui, state, &response, canvas_rect);
    }

    // Render after input so this frame shows the updated viewport.
    let ppp = ui.ctx().pixels_per_point();
    let size_px = [
        (canvas_rect.width() * ppp).round().max(0.0) as u32,
        (canvas_rect.height() * ppp).round().max(0.0) as u32,
    ];
    let painter = ui.painter_at(canvas_rect);
    let proof = if state.proof_colors { proof(state) } else { None };
    let look = vs_render::Look { mode: state.view_mode, overprints: state.simulate_overprints, proof: proof.as_deref() };
    let rendered = gpu.render(state, &View { viewport: state.viewport, look, pasteboard: None }, size_px, ppp as f64);
    match rendered {
        Some((tex, place)) => {
            let place = place.translate(canvas_rect.min.to_vec2());
            painter.image(tex, place, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            painter.rect_filled(canvas_rect, 0.0, ui.visuals().extreme_bg_color);
        }
    }

    crate::guides::draw(&painter, state, canvas_rect);
    crate::tools::draw_overlay(&painter, state, canvas_rect);

    if let Some(start) = state.interaction.zoom_marquee_start
        && let Some(now) = response.interact_pointer_pos()
    {
        let r = Rect::from_two_pos(start, now);
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_rgb(0x1E, 0x6F, 0xD9)), egui::StrokeKind::Inside);
    }

    if state.prefs.show_rulers {
        draw_rulers(ui, state, full, canvas_rect);
        if !matches!(state.tool, Tool::Zoom | Tool::Pan) {
            let top = Rect::from_min_max(pos2(canvas_rect.min.x, full.min.y), pos2(canvas_rect.max.x, canvas_rect.min.y));
            let left = Rect::from_min_max(pos2(full.min.x, canvas_rect.min.y), pos2(canvas_rect.min.x, canvas_rect.max.y));
            crate::guides::ruler_drag(ui, state, top, left, canvas_rect);
        }
    }
    page_tabs_ui(ui, state, tabs_rect, commands);
}

pub fn fit_page(state: &mut AppState) {
    let size = kurbo::Size::new(state.canvas_size.x as f64, state.canvas_size.y as f64);
    state.viewport.fit(state.doc.active_page().bounds(), size, FIT_MARGIN);
}

fn to_local(p: Pos2, canvas: Rect) -> Point {
    Point::new((p.x - canvas.min.x) as f64, (p.y - canvas.min.y) as f64)
}

/// View navigation (zoom, pan) and the Zoom tool. Returns true while the view is being panned.
fn handle_input(ui: &egui::Ui, state: &mut AppState, response: &egui::Response, canvas: Rect) -> bool {
    let hover = response.hover_pos();
    state.cursor_doc = hover.map(|p| state.viewport.screen_to_doc(to_local(p, canvas)));

    let (scroll, zoom, space) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.key_down(egui::Key::Space)));
    if let Some(pos) = hover {
        if zoom != 1.0 {
            state.viewport.zoom_at(to_local(pos, canvas), zoom as f64);
        } else if scroll != Vec2::ZERO {
            state.viewport.pan(kurbo::Vec2::new(scroll.x as f64, scroll.y as f64));
        }
    }

    let pan_drag = response.dragged_by(PointerButton::Middle)
        || (response.dragged_by(PointerButton::Primary) && (state.tool == Tool::Pan || space));
    if pan_drag {
        // A drag only starts after a few points of movement; include them so the page
        // stays under the pointer.
        let d = if response.drag_started() { response.total_drag_delta().unwrap_or_default() } else { response.drag_delta() };
        state.viewport.pan(kurbo::Vec2::new(d.x as f64, d.y as f64));
    }

    if state.tool == Tool::Zoom && !space {
        let zoom_out = ui.input(|i| i.modifiers.alt || i.modifiers.shift);
        if response.drag_started_by(PointerButton::Primary) {
            state.interaction.zoom_marquee_start = response.interact_pointer_pos();
        }
        if response.drag_stopped_by(PointerButton::Primary)
            && let (Some(start), Some(end)) = (state.interaction.zoom_marquee_start.take(), response.interact_pointer_pos())
        {
            let r = Rect::from_two_pos(start, end);
            if r.width() > 4.0 && r.height() > 4.0 {
                let a = state.viewport.screen_to_doc(to_local(r.min, canvas));
                let b = state.viewport.screen_to_doc(to_local(r.max, canvas));
                let view = kurbo::Size::new(canvas.width() as f64, canvas.height() as f64);
                state.viewport.fit(kurbo::Rect::from_points(a, b), view, 0.0);
            }
        }
        if let Some(pos) = response.interact_pointer_pos() {
            if response.clicked() {
                state.viewport.zoom_at(to_local(pos, canvas), if zoom_out { 0.5 } else { 2.0 });
            } else if response.secondary_clicked() {
                state.viewport.zoom_at(to_local(pos, canvas), 0.5);
            }
        }
    }

    if response.hovered() {
        let icon = if pan_drag {
            CursorIcon::Grabbing
        } else if state.tool == Tool::Pan || space {
            CursorIcon::Grab
        } else if state.tool == Tool::Zoom {
            CursorIcon::ZoomIn
        } else if crate::tools::is_implemented(state.tool) {
            crate::tools::cursor(state.tool)
        } else {
            CursorIcon::NotAllowed
        };
        ui.ctx().set_cursor_icon(icon);
    }
    pan_drag || space
}

fn draw_rulers(ui: &egui::Ui, state: &AppState, full: Rect, canvas: Rect) {
    let painter = ui.painter();

    let bg = theme::BAR;
    let line = Color32::from_gray(0x8A);
    let text = theme::TEXT_DIM;
    let font = egui::FontId::proportional(9.0);
    let unit = state.doc.display_unit;
    let offset = state.viewport.offset();
    let zoom = state.viewport.zoom();

    let top = Rect::from_min_max(pos2(canvas.min.x, full.min.y), pos2(canvas.max.x, canvas.min.y));
    let left = Rect::from_min_max(pos2(full.min.x, canvas.min.y), pos2(canvas.min.x, canvas.max.y));
    let corner = Rect::from_min_max(full.min, canvas.min);
    for r in [top, left, corner] {
        painter.rect_filled(r, 0.0, bg);
    }
    painter.text(corner.center(), egui::Align2::CENTER_CENTER, unit.suffix(), font.clone(), line);
    painter.line_segment([top.left_bottom(), top.right_bottom()], Stroke::new(1.0, line));
    painter.line_segment([left.right_top(), left.right_bottom()], Stroke::new(1.0, line));

    let top_painter = painter.with_clip_rect(top);
    for tick in ruler_ticks(zoom, offset.x, top.width() as f64, unit) {
        let x = top.min.x + tick.pos as f32;
        let h = if tick.major { RULER } else { 5.0 };
        top_painter.line_segment([pos2(x, top.max.y - h), pos2(x, top.max.y)], Stroke::new(1.0, line));
        if let Some(label) = tick.label {
            top_painter.text(pos2(x + 3.0, top.min.y + 1.0), egui::Align2::LEFT_TOP, label, font.clone(), text);
        }
    }

    let left_painter = painter.with_clip_rect(left);
    for tick in ruler_ticks(zoom, offset.y, left.height() as f64, unit) {
        let y = left.min.y + tick.pos as f32;
        let w = if tick.major { RULER } else { 5.0 };
        left_painter.line_segment([pos2(left.max.x - w, y), pos2(left.max.x, y)], Stroke::new(1.0, line));
        if let Some(label) = tick.label {
            let galley = ui.fonts_mut(|f| f.layout_no_wrap(label, font.clone(), text));
            left_painter.add(
                egui::epaint::TextShape::new(pos2(left.min.x + 1.0, y - 3.0), galley, text)
                    .with_angle(-std::f32::consts::FRAC_PI_2),
            );
        }
    }

    // Cursor guides on both rulers.
    if let Some(p) = state.cursor_doc {
        let s = state.viewport.doc_to_screen(p);
        let accent = theme::ACCENT;
        let x = canvas.min.x + s.x as f32;
        let y = canvas.min.y + s.y as f32;
        if top.x_range().contains(x) {
            painter.line_segment([pos2(x, top.min.y), pos2(x, top.max.y)], Stroke::new(1.0, accent));
        }
        if left.y_range().contains(y) {
            painter.line_segment([pos2(left.min.x, y), pos2(left.max.x, y)], Stroke::new(1.0, accent));
        }
    }
}

fn page_tabs_ui(ui: &mut egui::Ui, state: &mut AppState, rect: Rect, commands: &mut Vec<Command>) {
    ui.painter().rect_filled(rect, 0.0, theme::BAR);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(4.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ui = &mut child;
    ui.spacing_mut().item_spacing.x = 0.0;
    let count = state.doc.page_count();
    let active = state.doc.active_page_index();

    let button = |ui: &mut egui::Ui, icon: egui_material_icons::MaterialIcon, tip: &str, enabled: bool| {
        let (r, resp) = ui.allocate_exact_size(vec2(26.0, rect.height()), if enabled { Sense::click() } else { Sense::hover() });
        let color = if !enabled {
            theme::TEXT_DISABLED
        } else if resp.hovered() {
            Color32::WHITE
        } else {
            theme::TEXT
        };
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, <&str>::from(icon), egui::FontId::new(17.0, icon.font_family()), color);
        crate::a11y::button(&resp, enabled, tip);
        resp.on_hover_text(tip).clicked()
    };

    if button(ui, ICON_ADD, "Insert page before", true) {
        commands.push(Command::InsertPageBefore);
    }
    if button(ui, ICON_FIRST_PAGE, "First page", active > 0) {
        let _ = state.doc.set_active_page(0);
    }
    if button(ui, ICON_CHEVRON_LEFT, "Previous page", active > 0) {
        commands.push(Command::PreviousPage);
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new(format!("{} of {}", active + 1, count)).color(theme::TEXT_DIM));
    ui.add_space(4.0);
    if button(ui, ICON_CHEVRON_RIGHT, "Next page", active + 1 < count) {
        commands.push(Command::NextPage);
    }
    if button(ui, ICON_LAST_PAGE, "Last page", active + 1 < count) {
        let _ = state.doc.set_active_page(count - 1);
    }
    if button(ui, ICON_ADD, "Insert page after", true) {
        commands.push(Command::InsertPage);
    }
    ui.add_space(8.0);
    egui::ScrollArea::horizontal().scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for i in 0..count {
                let label = state.doc.page_label(i);
                let text_w = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), egui::FontId::proportional(13.0), Color32::WHITE).size().x);
                let (r, resp) = ui.allocate_exact_size(vec2((text_w + 40.0).max(100.0), rect.height()), Sense::click());
                let on = i == active;
                crate::a11y::toggle(&resp, true, on, &format!("Page tab {label}"));
                if on {
                    ui.painter().rect_filled(r, 0.0, theme::SUNKEN);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, Color32::from_gray(0x30));
                }
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(13.0), if on { theme::TEXT } else { theme::TEXT_DIM });
                if resp.clicked() {
                    let _ = state.doc.set_active_page(i);
                }
            }
        });
    });
}

/// Full-Screen Preview: the page alone, fitted to the window on black, with no tools,
/// selection or guides. Its placement is kept in `state.preview` (tests read it).
pub fn preview_ui(ui: &mut egui::Ui, state: &mut AppState, gpu: &mut GpuCanvas) {
    let rect = ui.available_rect_before_wrap();
    ui.allocate_rect(rect, Sense::hover());
    let mut viewport = vs_render::Viewport::default();
    viewport.fit(state.doc.active_page().bounds(), kurbo::Size::new(rect.width() as f64, rect.height() as f64), FIT_MARGIN);
    state.preview = Some(crate::app::Preview { viewport, rect });
    let ppp = ui.ctx().pixels_per_point();
    let size = [(rect.width() * ppp).round().max(0.0) as u32, (rect.height() * ppp).round().max(0.0) as u32];
    let view = View { viewport, look: vs_render::ViewMode::Enhanced.into(), pasteboard: Some(vs_render::vello::peniko::Color::BLACK) };
    if let Some((tex, place)) = gpu.render(state, &view, size, ppp as f64) {
        ui.painter_at(rect).image(tex, place.translate(rect.min.to_vec2()), Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
    }
}
