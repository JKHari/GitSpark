use eframe::egui::{self, Color32, RichText};

use crate::ui::theme::{SURFACE_BG, TEXT_MUTED};

pub fn status_badge(ui: &mut egui::Ui, label: &str, color: Color32) {
    let badge = egui::Frame::default()
        .fill(color)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(6, 2));
    badge.show(ui, |ui| {
        ui.label(RichText::new(label).size(10.0).color(Color32::WHITE));
    });
}

pub fn count_badge(ui: &mut egui::Ui, count: usize) {
    if count == 0 {
        return;
    }
    let badge = egui::Frame::default()
        .fill(SURFACE_BG)
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(5, 1));
    badge.show(ui, |ui| {
        ui.label(RichText::new(count.to_string()).size(10.0).color(TEXT_MUTED));
    });
}
