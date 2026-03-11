mod ai;
mod git;
mod models;
mod storage;
mod ui;

fn main() -> eframe::Result<()> {
    let saved_settings = storage::load_settings().unwrap_or_default();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([
            saved_settings.window_size.width,
            saved_settings.window_size.height,
        ]),
        ..Default::default()
    };
    eframe::run_native(
        "GitSpark",
        options,
        Box::new(|cc| Ok(Box::new(ui::GitSparkApp::new(cc)))),
    )
}
