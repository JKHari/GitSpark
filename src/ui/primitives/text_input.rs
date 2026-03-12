use eframe::egui::{self, Stroke, Vec2};

use crate::ui::theme::{ACCENT_MUTED, SURFACE_BG, SURFACE_BG_MUTED, color_with_alpha};

fn apply_dark_input_visuals(ui: &mut egui::Ui) {
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.bg_fill = SURFACE_BG_MUTED;
    visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    visuals.widgets.hovered.bg_fill = SURFACE_BG_MUTED;
    visuals.widgets.hovered.bg_stroke =
        Stroke::new(1.0, color_with_alpha(ACCENT_MUTED, 120.0));
    visuals.widgets.active.bg_fill = SURFACE_BG_MUTED;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_MUTED);
    visuals.selection.bg_fill = color_with_alpha(ACCENT_MUTED, 90.0);
}

pub fn styled_singleline(
    ui: &mut egui::Ui,
    value: &mut String,
    hint: &str,
) -> egui::Response {
    ui.scope(|ui| {
        apply_dark_input_visuals(ui);
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(f32::INFINITY)
                .hint_text(hint)
                .background_color(SURFACE_BG_MUTED)
                .margin(Vec2::new(10.0, 9.0)),
        )
    })
    .inner
}

pub fn styled_password(
    ui: &mut egui::Ui,
    value: &mut String,
    hint: &str,
) -> egui::Response {
    ui.scope(|ui| {
        apply_dark_input_visuals(ui);
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(f32::INFINITY)
                .hint_text(hint)
                .password(true)
                .background_color(SURFACE_BG_MUTED)
                .margin(Vec2::new(10.0, 9.0)),
        )
    })
    .inner
}

pub fn styled_multiline(
    ui: &mut egui::Ui,
    value: &mut String,
    rows: usize,
    hint: &str,
) -> egui::Response {
    ui.scope(|ui| {
        apply_dark_input_visuals(ui);
        ui.add(
            egui::TextEdit::multiline(value)
                .desired_width(f32::INFINITY)
                .desired_rows(rows)
                .hint_text(hint)
                .background_color(SURFACE_BG_MUTED)
                .margin(Vec2::new(10.0, 10.0)),
        )
    })
    .inner
}

pub fn commit_singleline(
    ui: &mut egui::Ui,
    value: &mut String,
    hint: &str,
) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .hint_text(hint)
            .text_color(egui::Color32::from_rgb(232, 238, 245))
            .background_color(SURFACE_BG)
            .margin(egui::Margin::symmetric(6, 4)),
    )
}

pub fn commit_multiline(
    ui: &mut egui::Ui,
    value: &mut String,
    hint: &str,
) -> egui::Response {
    ui.add(
        egui::TextEdit::multiline(value)
            .desired_width(f32::INFINITY)
            .desired_rows(5)
            .hint_text(hint)
            .text_color(egui::Color32::from_rgb(232, 238, 245))
            .background_color(SURFACE_BG)
            .margin(egui::Margin::symmetric(8, 8)),
    )
}
