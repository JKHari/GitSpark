mod ai;
mod git;
mod models;
mod storage;
mod ui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "github-rusttop",
        options,
        Box::new(|cc| Ok(Box::new(ui::RustTopApp::new(cc)))),
    )
}
