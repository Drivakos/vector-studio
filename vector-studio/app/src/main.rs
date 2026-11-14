//! Vector Studio: a native vector illustration app.

mod app;
mod canvas;
mod chrome;
mod commands;
mod dockers;
mod files;
#[cfg(target_os = "macos")]
mod native_menu;
mod panels;
mod theme;
mod titlebar;
mod toolbox;
mod tools;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("Vector Studio")
            // The app draws its own title bar and toolbar, as the reference app's unified
            // toolbar does: content runs under a transparent title bar.
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([800.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Vector Studio",
        options,
        Box::new(|cc| Ok(Box::new(app::VectorStudioApp::new(cc)))),
    )
}
