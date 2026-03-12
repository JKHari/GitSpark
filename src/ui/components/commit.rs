use std::time::Duration;

use eframe::egui::{self, Align, Align2, Color32, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;

use crate::models::CommitSuggestion;
use crate::ui::theme::{BORDER, PANEL_BG, SURFACE_BG, TEXT_MAIN, TEXT_MUTED};

pub enum CommitPanelAction {
    GenerateAiCommit,
    ShowSettings,
    CommitAll,
}

pub struct CommitPanelProps<'a> {
    pub summary: &'a mut String,
    pub body: &'a mut String,
    pub ai_in_flight: bool,
    pub ai_preview: Option<&'a CommitSuggestion>,
    pub branch_label: &'a str,
    pub stash_count: usize,
    pub avatar_letter: &'a str,
}

pub struct CommitPanelOutput {
    pub action: Option<CommitPanelAction>,
}

pub fn render_commit_panel(
    ui: &mut egui::Ui,
    props: &mut CommitPanelProps<'_>,
) -> CommitPanelOutput {
    let mut action = None;

    egui::Frame::default()
        .fill(PANEL_BG)
        .inner_margin(egui::Margin::symmetric(8, 8))
        .show(ui, |ui| {
            egui::Frame::default().fill(PANEL_BG).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());

                    // Avatar + summary input
                    ui.horizontal(|ui| {
                        let (avatar_rect, _) =
                            ui.allocate_exact_size(Vec2::new(24.0, 24.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            avatar_rect.center(),
                            11.5,
                            Color32::from_rgb(201, 178, 158),
                        );
                        ui.painter().text(
                            avatar_rect.center(),
                            Align2::CENTER_CENTER,
                            props.avatar_letter,
                            egui::FontId::proportional(12.0),
                            Color32::from_rgb(70, 56, 47),
                        );

                        let summary = egui::TextEdit::singleline(props.summary)
                            .desired_width(f32::INFINITY)
                            .hint_text("Summary (required)")
                            .text_color(Color32::from_rgb(232, 238, 245))
                            .background_color(SURFACE_BG)
                            .margin(egui::Margin::symmetric(6, 4));
                        ui.add_sized([ui.available_width(), 24.0], summary);
                    });

                    ui.add_space(8.0);

                    // Description editor with toolbar
                    egui::Frame::default()
                        .fill(SURFACE_BG)
                        .stroke(Stroke::new(1.0, BORDER))
                        .corner_radius(5.0)
                        .inner_margin(egui::Margin::same(0))
                        .show(ui, |ui| {
                            let editor_height = 108.0;
                            ui.allocate_ui_with_layout(
                                Vec2::new(ui.available_width(), editor_height),
                                egui::Layout::top_down(Align::Min),
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .max_height(editor_height)
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.add(
                                                egui::TextEdit::multiline(props.body)
                                                    .desired_width(f32::INFINITY)
                                                    .desired_rows(5)
                                                    .hint_text("Description")
                                                    .text_color(Color32::from_rgb(232, 238, 245))
                                                    .background_color(SURFACE_BG)
                                                    .margin(egui::Margin::symmetric(8, 8)),
                                            );
                                        });
                                },
                            );

                            // Separator line
                            let separator_y = ui.cursor().top();
                            ui.painter().hline(
                                ui.min_rect().x_range(),
                                separator_y,
                                Stroke::new(1.0, BORDER),
                            );

                            // Toolbar row (AI + settings)
                            egui::Frame::default()
                                .fill(SURFACE_BG)
                                .inner_margin(egui::Margin::symmetric(10, 3))
                                .show(ui, |ui| {
                                    ui.set_height(22.0);
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;

                                        let toolbar_icon = |ui: &mut egui::Ui,
                                                            icon: &str,
                                                            tip: &str,
                                                            enabled: bool,
                                                            color: Color32,
                                                            size: f32| {
                                            ui.add_enabled(
                                                enabled,
                                                egui::Button::new(
                                                    RichText::new(icon)
                                                        .size(size)
                                                        .color(color),
                                                )
                                                .fill(Color32::TRANSPARENT)
                                                .stroke(Stroke::NONE)
                                                .min_size(Vec2::new(18.0, 18.0)),
                                            )
                                            .on_hover_cursor(if enabled {
                                                egui::CursorIcon::PointingHand
                                            } else {
                                                egui::CursorIcon::Default
                                            })
                                            .on_hover_text(tip)
                                        };

                                        if props.ai_in_flight {
                                            ui.ctx().request_repaint_after(
                                                Duration::from_millis(16),
                                            );
                                            ui.add_enabled_ui(false, |ui| {
                                                ui.add(egui::Spinner::new().size(12.0));
                                            })
                                            .response
                                            .on_hover_text("Generating with AI...");
                                        } else if toolbar_icon(
                                            ui,
                                            icons::SPARKLE,
                                            "Generate with AI",
                                            true,
                                            TEXT_MUTED,
                                            15.0,
                                        )
                                        .clicked()
                                        {
                                            action =
                                                Some(CommitPanelAction::GenerateAiCommit);
                                        }

                                        if toolbar_icon(
                                            ui,
                                            icons::GEAR,
                                            "Commit settings",
                                            true,
                                            TEXT_MUTED,
                                            15.0,
                                        )
                                        .clicked()
                                        {
                                            action = Some(CommitPanelAction::ShowSettings);
                                        }
                                    });
                                });
                        });

                    // AI preview
                    if let Some(preview) = props.ai_preview {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!("AI: {}", preview.subject))
                                .small()
                                .color(TEXT_MUTED),
                        );
                    }

                    // Stash row
                    render_stash_row(ui, props.stash_count);

                    ui.add_space(8.0);

                    // Commit button
                    let commit_button = egui::Button::new(
                        RichText::new(format!("Commit to {}", props.branch_label))
                            .color(Color32::from_rgb(223, 230, 240))
                            .strong(),
                    )
                    .fill(Color32::from_rgb(58, 96, 194))
                    .stroke(Stroke::NONE)
                    .corner_radius(5.0);
                    if ui
                        .add_sized([ui.available_width(), 24.0], commit_button)
                        .clicked()
                    {
                        action = Some(CommitPanelAction::CommitAll);
                    }

                    // Last commit info
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("Committed just now")
                                    .size(12.0)
                                    .color(TEXT_MUTED),
                            );
                            ui.label(
                                RichText::new(truncate_commit_footer(props.summary))
                                    .size(12.0)
                                    .color(TEXT_MAIN),
                            );
                        });
                        ui.with_layout(
                            egui::Layout::right_to_left(Align::Center),
                            |ui| {
                                let undo = egui::Button::new(
                                    RichText::new("Undo").size(12.0).color(TEXT_MAIN),
                                )
                                .fill(SURFACE_BG)
                                .stroke(Stroke::new(1.0, BORDER))
                                .corner_radius(5.0)
                                .min_size(Vec2::new(42.0, 22.0));
                                let _ = ui.add(undo);
                            },
                        );
                    });
                });
            });
        });

    CommitPanelOutput { action }
}

fn render_stash_row(ui: &mut egui::Ui, stash_count: usize) {
    if stash_count == 0 {
        return;
    }

    ui.add_space(8.0);
    let label = if stash_count == 1 {
        "▸ Stashed Changes".to_string()
    } else {
        format!("▸ Stashed Changes ({stash_count})")
    };

    let response = ui.add(
        egui::Button::new(RichText::new(label).color(TEXT_MUTED))
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(5.0)
            .min_size(Vec2::new(ui.available_width(), 24.0)),
    );

    if response.clicked() {
        // TODO: Open stash view
    }
}

fn truncate_commit_footer(summary: &str) -> String {
    let trimmed = summary.trim();
    if trimmed.is_empty() {
        return "No commit message yet".to_string();
    }

    let max = 34;
    let mut chars = trimmed.chars();
    let shortened: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}
