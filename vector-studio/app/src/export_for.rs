//! File > Export For: Web (a PNG at screen resolution, or SVG) and Print-Ready PDF (every
//! page with bleed and crop marks).

use std::path::PathBuf;

use vs_core::{Rgba, Unit};
use vs_io::{ExportFormat, ExportOptions, PrintMarks};

use crate::app::AppState;

/// The open Export For window and its settings.
#[derive(Clone, Debug, PartialEq)]
pub enum ExportFor {
    Web { format: ExportFormat, dpi: f64, white_background: bool },
    PrintPdf { bleed_mm: f64, crop_marks: bool },
}

impl ExportFor {
    pub fn web() -> Self {
        // Screen resolution: one pixel per point.
        ExportFor::Web { format: ExportFormat::Png, dpi: 72.0, white_background: false }
    }

    pub fn print_pdf() -> Self {
        ExportFor::PrintPdf { bleed_mm: 3.0, crop_marks: true }
    }

    fn title(&self) -> &'static str {
        match self {
            ExportFor::Web { .. } => "Export For Web",
            ExportFor::PrintPdf { .. } => "Print-Ready PDF",
        }
    }
}

/// Ask where to save, with `format`'s extension added when missing.
fn ask_path(state: &mut AppState, title: &str, format: ExportFormat) -> Option<PathBuf> {
    let name = format!("{}.{}", crate::files::file_stem(state), format.extension());
    let mut path = state.dialogs.save_file(title, format.label(), format.extension(), &name)?;
    if ExportFormat::from_path(&path).ok() != Some(format) {
        path.set_extension(format.extension());
    }
    Some(path)
}

/// Ask where to save, then write the file. True once written.
fn export(state: &mut AppState, settings: &ExportFor) -> bool {
    let title = settings.title();
    let (path, result) = match *settings {
        ExportFor::Web { format, dpi, white_background } => {
            let Some(path) = ask_path(state, title, format) else { return false };
            let options = ExportOptions { dpi, background: white_background.then_some(Rgba::WHITE) };
            let result = vs_io::export(&state.doc, &path, &options).map(|_| ());
            (path, result)
        }
        ExportFor::PrintPdf { bleed_mm, crop_marks } => {
            let Some(path) = ask_path(state, title, ExportFormat::Pdf) else { return false };
            let marks = PrintMarks { bleed: Unit::Millimeters.to_points(bleed_mm), crop_marks };
            let pages: Vec<usize> = (0..state.doc.page_count()).collect();
            let result = vs_io::pages_to_print_pdf(&state.doc, &pages, &marks).and_then(|pdf| std::fs::write(&path, pdf).map_err(Into::into));
            (path, result)
        }
    };
    match result {
        Ok(()) => {
            state.status = Some(format!("Exported {}", path.display()));
            true
        }
        Err(e) => {
            state.dialogs.error("Could not export", &format!("{}: {e}", path.display()));
            false
        }
    }
}

/// The Export For Web or Print-Ready PDF window.
pub fn export_for_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(mut settings) = state.export_for.take() else { return };
    let title = settings.title();
    let page = state.doc.active_page().size;
    let pages = state.doc.page_count();
    let mut open = true;
    let (mut go, mut cancel) = (false, false);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Grid::new("export_for_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| match &mut settings {
                ExportFor::Web { format, dpi, white_background } => {
                    ui.label("Format:");
                    ui.horizontal(|ui| {
                        ui.radio_value(format, ExportFormat::Png, "PNG");
                        ui.radio_value(format, ExportFormat::Svg, "SVG");
                    });
                    ui.end_row();
                    if *format == ExportFormat::Png {
                        ui.label("Resolution:");
                        crate::a11y::add(ui, egui::DragValue::new(dpi).range(18.0..=600.0).speed(1.0).suffix(" dpi"), "Resolution (web)");
                        ui.end_row();
                        ui.label("Background:");
                        ui.horizontal(|ui| {
                            ui.radio_value(white_background, false, "Transparent");
                            ui.radio_value(white_background, true, "White");
                        });
                        ui.end_row();
                        let k = *dpi / 72.0;
                        ui.label("Image size:");
                        ui.label(format!("{} × {} px", (page.width * k).ceil(), (page.height * k).ceil()));
                        ui.end_row();
                    }
                    ui.label("Exports:");
                    ui.label("the current page");
                    ui.end_row();
                }
                ExportFor::PrintPdf { bleed_mm, crop_marks } => {
                    ui.label("Bleed:");
                    crate::a11y::add(ui, egui::DragValue::new(bleed_mm).range(0.0..=25.0).speed(0.1).suffix(" mm"), "Bleed");
                    ui.end_row();
                    ui.label("Marks:");
                    crate::a11y::add(ui, egui::Checkbox::new(crop_marks, "Crop marks"), "Crop marks");
                    ui.end_row();
                    ui.label("Exports:");
                    ui.label(format!("all {pages} page(s)"));
                    ui.end_row();
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                go = crate::a11y::window_button(ui, true, "Export…", title).clicked();
                cancel = crate::a11y::window_button(ui, true, "Cancel", title).clicked();
            });
        });
    if go && export(state, &settings) {
        return;
    }
    if open && !cancel {
        state.export_for = Some(settings);
    }
}
