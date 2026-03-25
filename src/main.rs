// Suppress the console window on Windows for GUI apps.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod git;
mod models;
mod storage;
mod ui;

use gpui::*;
use gpui_component_assets::Assets;

use crate::storage::load_settings;
use crate::ui::GitSparkApp;

fn main() {
    let settings = match load_settings() {
        Ok(s) => s,
        Err(_) => models::AppSettings::default(),
    };

    let initial_width = settings.window_size.width;
    let initial_height = settings.window_size.height;

    let app = Application::new().with_assets(Assets);
    app.run(move |cx| {
        gpui_component::init(cx);
        // Force dark theme on gpui-component to match our GitHub Dark theme
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(initial_width), px(initial_height)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("GitSpark".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| GitSparkApp::new(settings.clone(), cx)),
        )
        .unwrap();
    });
}
