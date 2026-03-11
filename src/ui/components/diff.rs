use crate::ui::theme::{
    ACCENT, DIFF_ADD_BG, DIFF_ADD_FG, DIFF_DEL_BG, DIFF_DEL_FG, DIFF_HUNK_BG, TEXT_MAIN, TEXT_MUTED,
};
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

pub fn render_diff_text(ui: &mut egui::Ui, diff_text: &str) {
    let mut old_line = 0;
    let mut new_line = 0;
    let mut in_hunk = false;

    for line in diff_text.lines() {
        let is_hunk_header = line.starts_with("@@ ");
        if is_hunk_header {
            if let Some(hunk_info) = line.split("@@").nth(1) {
                let parts: Vec<&str> = hunk_info.trim().split(' ').collect();
                if parts.len() >= 2 {
                    old_line = parts[0]
                        .trim_start_matches('-')
                        .split(',')
                        .next()
                        .unwrap_or("0")
                        .parse()
                        .unwrap_or(0);
                    new_line = parts[1]
                        .trim_start_matches('+')
                        .split(',')
                        .next()
                        .unwrap_or("0")
                        .parse()
                        .unwrap_or(0);
                    in_hunk = true;
                }
            }
        } else if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
        {
            in_hunk = false;
        }

        let (bg_color, text_color, _line_prefix) =
            if line.starts_with('+') && !line.starts_with("+++") {
                (
                    DIFF_ADD_BG,
                    DIFF_ADD_FG,
                    "+",
                )
            } else if line.starts_with('-') && !line.starts_with("---") {
                (
                    DIFF_DEL_BG,
                    DIFF_DEL_FG,
                    "-",
                )
            } else if is_hunk_header {
                (DIFF_HUNK_BG, ACCENT, "@@")
            } else {
                (Color32::TRANSPARENT, TEXT_MUTED, " ")
            };

        let mut old_num = String::new();
        let mut new_num = String::new();

        if in_hunk && !is_hunk_header {
            if line.starts_with('+') {
                new_num = new_line.to_string();
                new_line += 1;
            } else if line.starts_with('-') {
                old_num = old_line.to_string();
                old_line += 1;
            } else if !line.starts_with('\\') {
                old_num = old_line.to_string();
                new_num = new_line.to_string();
                old_line += 1;
                new_line += 1;
            }
        }

        egui::Frame::default()
            .fill(bg_color)
            .inner_margin(egui::Margin::symmetric(0, 2))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;

                    // Line numbers
                    let ln_rect = ui
                        .allocate_exact_size(Vec2::new(70.0, 16.0), egui::Sense::hover())
                        .0;
                    ui.painter()
                        .rect_filled(ln_rect, 0.0, Color32::from_black_alpha(40));
                    ui.painter().vline(
                        ln_rect.right(),
                        ln_rect.y_range(),
                        Stroke::new(1.0, Color32::from_black_alpha(80)),
                    );

                    ui.painter().text(
                        ln_rect.left_center() + Vec2::new(30.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        old_num,
                        egui::FontId::monospace(11.0),
                        Color32::from_gray(120),
                    );
                    ui.painter().text(
                        ln_rect.right_center() - Vec2::new(6.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        new_num,
                        egui::FontId::monospace(11.0),
                        Color32::from_gray(120),
                    );

                    ui.add_space(8.0);

                    // Content
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(line)
                                    .family(egui::FontFamily::Monospace)
                                    .size(12.5)
                                    .color(text_color),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                });
            });
    }
}
