use eframe::egui::{self, Stroke};

use crate::ui::theme::{
    BORDER, CORNER_RADIUS, PANEL_BG, SECTION_PADDING, SURFACE_BG, SURFACE_BG_MUTED,
};

pub fn panel_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(PANEL_BG)
        .inner_margin(egui::Margin::same(0))
}

pub fn surface_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(SURFACE_BG)
        .stroke(Stroke::new(1.0, BORDER))
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::default()
        .fill(SURFACE_BG_MUTED)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CORNER_RADIUS)
        .inner_margin(egui::Margin::same(SECTION_PADDING as i8))
}

pub fn bordered_section<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::default()
        .fill(SURFACE_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CORNER_RADIUS)
        .inner_margin(egui::Margin::same(SECTION_PADDING as i8))
        .show(ui, add_contents)
}
