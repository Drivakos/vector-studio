//! File > Print: the Print window, and printing through the system's print system (CUPS
//! `lp`), behind [`Printing`] so the end-to-end tests can stand in for the printer.

use std::process::Command as Process;

use crate::app::AppState;

/// A print job: which printer, how many copies, and the name it shows in the queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintJob {
    pub printer: String,
    pub copies: u32,
    pub title: String,
}

/// The printers and print queue the app uses.
pub trait Printing {
    /// Printer names, and the system default among them.
    fn printers(&mut self) -> (Vec<String>, Option<String>);
    /// Send `pdf` to a printer.
    fn print(&mut self, pdf: &[u8], job: &PrintJob) -> Result<(), String>;
}

/// The Mac's printers, through CUPS.
pub struct SystemPrinting;

impl Printing for SystemPrinting {
    fn printers(&mut self) -> (Vec<String>, Option<String>) {
        let run = |args: &[&str]| {
            Process::new("lpstat").args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        };
        let names: Vec<String> = run(&["-e"]).unwrap_or_default().lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect();
        // "system default destination: Office_Printer"
        let default = run(&["-d"]).and_then(|out| out.split_once(':').map(|(_, name)| name.trim().to_owned())).filter(|d| names.contains(d));
        (names, default)
    }

    fn print(&mut self, pdf: &[u8], job: &PrintJob) -> Result<(), String> {
        let path = std::env::temp_dir().join(format!("vector-studio-print-{}.pdf", std::process::id()));
        std::fs::write(&path, pdf).map_err(|e| e.to_string())?;
        // lp hands the file to the print system before it returns, so it can go then.
        let out = Process::new("lp")
            .args(["-d", &job.printer, "-n", &job.copies.to_string(), "-t", &job.title])
            .arg(&path)
            .output();
        let _ = std::fs::remove_file(&path);
        let out = out.map_err(|e| format!("could not run lp: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
        }
    }
}

/// Which pages to print.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrintPages {
    #[default]
    All,
    Current,
    Range,
}

/// Settings of the Print window.
#[derive(Clone, Debug)]
pub struct PrintDialog {
    pub open: bool,
    printers: Vec<String>,
    printer: Option<String>,
    copies: u32,
    pages: PrintPages,
    range: String,
}

impl Default for PrintDialog {
    fn default() -> Self {
        Self { open: false, printers: Vec::new(), printer: None, copies: 1, pages: PrintPages::All, range: String::new() }
    }
}

/// Pages from a range like "1-3, 5" or "4-" (1-based, inclusive), as 0-based indices in
/// the order given. `None` if any part is not a page of a `count`-page document.
pub fn parse_page_range(text: &str, count: usize) -> Option<Vec<usize>> {
    let page = |s: &str| s.trim().parse::<usize>().ok().filter(|n| (1..=count).contains(n));
    let mut out = Vec::new();
    for part in text.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let (from, to) = match part.split_once('-') {
            Some((a, b)) => (page(a)?, if b.trim().is_empty() { count } else { page(b)? }),
            None => (page(part)?, page(part)?),
        };
        if from > to {
            return None;
        }
        out.extend(from - 1..to);
    }
    (!out.is_empty()).then_some(out)
}

/// Open the Print window with the system's printers, the default one chosen.
pub fn open(state: &mut AppState) {
    let (printers, default) = state.printing.printers();
    let dialog = &mut state.print;
    if !dialog.printer.as_ref().is_some_and(|p| printers.contains(p)) {
        dialog.printer = default.or_else(|| printers.first().cloned());
    }
    dialog.printers = printers;
    dialog.open = true;
}

/// The pages the Print window would print, 0-based.
fn chosen_pages(state: &AppState) -> Option<Vec<usize>> {
    let count = state.doc.page_count();
    match state.print.pages {
        PrintPages::All => Some((0..count).collect()),
        PrintPages::Current => Some(vec![state.doc.active_page_index()]),
        PrintPages::Range => parse_page_range(&state.print.range, count),
    }
}

/// Print the chosen pages to the chosen printer.
fn print(state: &mut AppState) {
    let (Some(printer), Some(pages)) = (state.print.printer.clone(), chosen_pages(state)) else { return };
    let job = PrintJob { printer, copies: state.print.copies.max(1), title: crate::files::file_stem(state) };
    let result = vs_io::pages_to_pdf(&state.doc, &pages).map_err(|e| e.to_string()).and_then(|pdf| state.printing.print(&pdf, &job));
    match result {
        Ok(()) => {
            state.print.open = false;
            let n = pages.len();
            state.status = Some(format!("Sent {n} page{} to {}", if n == 1 { "" } else { "s" }, job.printer));
        }
        Err(e) => state.dialogs.error("Could not print", &e),
    }
}

/// The Print window: printer, copies and pages.
pub fn print_window(ctx: &egui::Context, state: &mut AppState) {
    if !state.print.open {
        return;
    }
    let count = state.doc.page_count();
    let pages = chosen_pages(state);
    let mut open = true;
    let mut go = false;
    egui::Window::new("Print Drawing")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let d = &mut state.print;
            if d.printers.is_empty() {
                ui.label("No printers are set up. Add one in System Settings › Printers & Scanners.");
                ui.add_space(8.0);
            }
            egui::Grid::new("print_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Printer:");
                let combo = egui::ComboBox::from_id_salt("print_printer")
                    .selected_text(d.printer.as_deref().unwrap_or("None"))
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for p in &d.printers {
                            ui.selectable_value(&mut d.printer, Some(p.clone()), p);
                        }
                    });
                crate::a11y::name(&combo.response, "Printer");
                ui.end_row();
                ui.label("Copies:");
                crate::a11y::add(ui, egui::DragValue::new(&mut d.copies).range(1..=999).speed(0.1), "Copies");
                ui.end_row();
                ui.label("Pages:");
                ui.vertical(|ui| {
                    ui.radio_value(&mut d.pages, PrintPages::All, format!("All pages ({count})"));
                    ui.radio_value(&mut d.pages, PrintPages::Current, "Current page");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut d.pages, PrintPages::Range, "Pages:");
                        let field = crate::a11y::add(ui, egui::TextEdit::singleline(&mut d.range).hint_text("1-3, 5").desired_width(100.0), "Page range");
                        if field.changed() {
                            d.pages = PrintPages::Range;
                        }
                    });
                });
                ui.end_row();
            });
            if d.pages == PrintPages::Range && pages.is_none() && !d.range.trim().is_empty() {
                ui.colored_label(ui.visuals().warn_fg_color, format!("Pages run from 1 to {count}, like 1-3, 5."));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ready = d.printer.is_some() && pages.is_some();
                go = ui.add_enabled(ready, egui::Button::new("Print")).clicked();
                if ui.button("Cancel").clicked() {
                    d.open = false;
                }
            });
        });
    if !open {
        state.print.open = false;
    }
    if go {
        print(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ranges_list_pages_in_order() {
        assert_eq!(parse_page_range("1-3, 5", 5), Some(vec![0, 1, 2, 4]));
        assert_eq!(parse_page_range(" 4- ", 6), Some(vec![3, 4, 5]), "open end: to the last page");
        assert_eq!(parse_page_range("2;2", 3), Some(vec![1, 1]), "as given");
        assert_eq!(parse_page_range("0", 3), None);
        assert_eq!(parse_page_range("2-9", 3), None, "past the last page");
        assert_eq!(parse_page_range("3-1", 3), None);
        assert_eq!(parse_page_range("a", 3), None);
        assert_eq!(parse_page_range(" , ", 3), None, "nothing to print");
    }
}
