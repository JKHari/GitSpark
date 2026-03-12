use eframe::egui::{self, RichText, Stroke};

use crate::models::DiffEntry;
use crate::ui::components::diff::render_diff_text;
use crate::ui::theme::{BG, BORDER, DIFF_BG, SURFACE_BG, TEXT_MAIN, TEXT_MUTED};

pub struct DiffViewerProps<'a> {
    pub selected_change: Option<&'a str>,
    pub selected_diff: Option<&'a DiffEntry>,
}

pub fn render_diff_viewer(ctx: &egui::Context, props: &DiffViewerProps<'_>) {
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(BG))
        .show(ctx, |ui| {
            let Some(change_path) = props.selected_change else {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new("No file selected")
                            .color(TEXT_MUTED)
                            .size(14.0),
                    );
                });
                return;
            };

            render_diff_title(ui, change_path);

            egui::Frame::default()
                .fill(DIFF_BG)
                .stroke(Stroke::new(1.0, BORDER))
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| match props.selected_diff {
                    Some(diff) if diff.is_binary => {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new("Binary file changed.")
                                    .color(TEXT_MUTED)
                                    .size(14.0),
                            );
                        });
                    }
                    Some(diff) if diff.diff.trim().is_empty() => {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new("No diff text available.")
                                    .color(TEXT_MUTED)
                                    .size(14.0),
                            );
                        });
                    }
                    Some(diff) => {
                        render_diff_text(ui, &diff.diff, &diff.path);
                    }
                    None => {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new("No diff available for this file.")
                                    .color(TEXT_MUTED)
                                    .size(14.0),
                            );
                        });
                    }
                });
        });
}

fn render_diff_title(ui: &mut egui::Ui, path: &str) {
    egui::Frame::default()
        .fill(SURFACE_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(path).color(TEXT_MAIN).size(14.0).strong());
            });
        });
}
