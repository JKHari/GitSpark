use eframe::egui::{self, Align2, Color32, RichText, Stroke, Vec2};

use crate::models::CommitInfo;
use crate::ui::theme::{ACCENT_MUTED, BORDER, TEXT_MAIN, TEXT_MUTED};

pub struct HistoryListProps<'a> {
    pub history: &'a [CommitInfo],
    pub selected_commit: Option<&'a str>,
}

/// Returns the OID of a clicked commit, if any.
pub fn render_history_list(ui: &mut egui::Ui, props: &HistoryListProps<'_>) -> Option<String> {
    let mut clicked_oid = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;

            if props.history.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No history").color(TEXT_MUTED));
                });
                return;
            }

            for commit in props.history {
                let is_selected =
                    props.selected_commit == Some(commit.oid.as_str());
                if let Some(oid) = render_history_row(ui, commit, is_selected) {
                    clicked_oid = Some(oid);
                }
            }
        });

    clicked_oid
}

fn render_history_row(
    ui: &mut egui::Ui,
    commit: &CommitInfo,
    is_selected: bool,
) -> Option<String> {
    let bg_color = if is_selected {
        ACCENT_MUTED
    } else {
        Color32::TRANSPARENT
    };
    let summary_color = if is_selected {
        Color32::WHITE
    } else {
        TEXT_MAIN
    };
    let meta_color = if is_selected {
        Color32::from_gray(225)
    } else {
        TEXT_MUTED
    };
    let summary = if commit.summary.trim().is_empty() {
        "Empty commit message"
    } else {
        commit.summary.trim()
    };

    let mut meta_parts = Vec::new();
    if commit.is_head {
        meta_parts.push("HEAD".to_string());
    }
    meta_parts.push(commit.short_oid.clone());
    meta_parts.push(commit.author_name.clone());
    meta_parts.push(commit.date.clone());
    let meta_text = meta_parts.join(" \u{2022} ");

    let response = egui::Frame::default()
        .fill(bg_color)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_min_height(40.0);
            ui.set_width(ui.available_width());
            let row_rect = ui.max_rect();
            let painter = ui.painter();
            let text_left = row_rect.left();
            let text_top = row_rect.top();

            painter.text(
                egui::pos2(text_left, text_top),
                Align2::LEFT_TOP,
                truncate(summary, 54),
                egui::FontId::proportional(12.5),
                summary_color,
            );
            painter.text(
                egui::pos2(text_left, text_top + 21.0),
                Align2::LEFT_TOP,
                truncate(&meta_text, 72),
                egui::FontId::proportional(11.0),
                meta_color,
            );
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    ui.painter().hline(
        response.rect.x_range(),
        response.rect.bottom(),
        Stroke::new(1.0, BORDER),
    );

    if response.clicked() {
        Some(commit.oid.clone())
    } else {
        None
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let mut chars = trimmed.chars();
    let shortened: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}
