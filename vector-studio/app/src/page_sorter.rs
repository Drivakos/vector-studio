//! Page Sorter View (View menu): every page as a thumbnail in place of the drawing
//! window. Click a page to make it active, double-click to go back to drawing on it,
//! drag it onto another page's place to move it there.

use std::collections::HashMap;

use egui::{Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use vs_core::{Page, PageId, Rgba};

use crate::app::AppState;
use crate::theme;

/// Longer side of a thumbnail, in points.
const THUMB: f32 = 150.0;
/// Space for the page name under a thumbnail.
const LABEL: f32 = 24.0;
const GAP: f32 = 18.0;

/// Page Sorter View: whether it is shown, the thumbnails drawn so far, and a page being
/// dragged.
#[derive(Default)]
pub struct PageSorter {
    pub open: bool,
    /// Each page's thumbnail with the page and paper colour it was drawn from.
    thumbs: HashMap<PageId, (Page, Rgba, egui::TextureHandle)>,
    dragging: Option<usize>,
}

impl PageSorter {
    /// The texture for `page`, drawn again only when the page or paper changed.
    fn thumbnail(&mut self, ctx: &egui::Context, page: &Page, paper: Rgba) -> Option<egui::TextureHandle> {
        if let Some((drawn, colour, tex)) = self.thumbs.get(&page.id)
            && drawn == page
            && *colour == paper
        {
            return Some(tex.clone());
        }
        let longest = (THUMB * ctx.pixels_per_point()).round() as u32;
        let (w, h, rgba) = vs_io::page_thumbnail(page, paper, longest).map_err(|e| log::warn!("page thumbnail: {e}")).ok()?;
        let image = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], &rgba);
        let tex = ctx.load_texture(format!("page-{}", page.id.0), image, egui::TextureOptions::LINEAR);
        self.thumbs.insert(page.id, (page.clone(), paper, tex.clone()));
        Some(tex)
    }
}

pub fn page_sorter_ui(ui: &mut egui::Ui, state: &mut AppState) {
    let full = ui.available_rect_before_wrap();
    ui.painter().rect_filled(full, 0.0, theme::BAR);
    let paper = state.doc.background.map_or(Rgba::WHITE, |b| b.color);
    let pages: Vec<PageId> = state.doc.pages().iter().map(|p| p.id).collect();
    state.page_sorter.thumbs.retain(|id, _| pages.contains(id));

    let pointer = ui.input(|i| i.pointer.interact_pos());
    let released = ui.input(|i| i.pointer.any_released());
    let mut tiles: Vec<Rect> = Vec::new();
    let (mut go, mut leave) = (None, false);
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.add_space(GAP);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            ui.add_space(GAP);
            for i in 0..state.doc.page_count() {
                let label = state.doc.page_label(i);
                let page = state.doc.pages()[i].clone();
                let (rect, resp) = ui.allocate_exact_size(vec2(THUMB + 12.0, THUMB + LABEL + 12.0), Sense::click_and_drag());
                tiles.push(rect);
                let active = i == state.doc.active_page_index();
                crate::a11y::toggle(&resp, true, active, &format!("Page thumbnail {label}"));
                if active {
                    ui.painter().rect_filled(rect, 6.0, theme::SUNKEN);
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 6.0, theme::FIELD);
                }
                if let Some(tex) = state.page_sorter.thumbnail(ui.ctx(), &page, paper) {
                    let size = tex.size_vec2() / ui.ctx().pixels_per_point();
                    let area = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(THUMB, THUMB));
                    let place = Rect::from_center_size(area.center(), size);
                    ui.painter().image(tex.id(), place, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    let edge = if active { theme::ACCENT } else { Color32::from_gray(0x80) };
                    ui.painter().rect_stroke(place, 0.0, Stroke::new(1.0, edge), StrokeKind::Outside);
                }
                ui.painter().text(
                    pos2(rect.center().x, rect.max.y - LABEL / 2.0 - 4.0),
                    egui::Align2::CENTER_CENTER,
                    format!("{}  {label}", i + 1),
                    egui::FontId::proportional(13.0),
                    if active { theme::TEXT } else { theme::TEXT_DIM },
                );
                if resp.drag_started() {
                    state.page_sorter.dragging = Some(i);
                }
                if resp.double_clicked() {
                    go = Some(i);
                    leave = true;
                } else if resp.clicked() {
                    go = Some(i);
                }
            }
        });
    });

    // Dropping a dragged page on another page's place moves it there.
    if let Some(from) = state.page_sorter.dragging {
        let over = pointer.and_then(|p| tiles.iter().position(|r| r.contains(p)));
        if let Some(to) = over.filter(|to| *to != from)
            && let Some(r) = tiles.get(to)
        {
            let x = if to > from { r.max.x + GAP / 2.0 } else { r.min.x - GAP / 2.0 };
            ui.painter().line_segment([pos2(x, r.min.y), pos2(x, r.max.y)], Stroke::new(3.0, theme::ACCENT));
        }
        if released {
            state.page_sorter.dragging = None;
            if let Some(to) = over.filter(|to| *to != from) {
                let _ = state.doc.move_page(from, to);
            }
        }
    }
    if let Some(i) = go {
        let _ = state.doc.set_active_page(i);
    }
    if leave {
        state.page_sorter.open = false;
        state.needs_fit = true;
    }
}
