use gpui::*;
use gpui_component::h_flex;

use crate::ui::theme;

/// Render the application status bar.
///
/// A 26px tall bar pinned to the bottom of the window. Displays either
/// an error message in red or a status message in muted text.
pub fn render_status_bar(
    status_message: &str,
    error_message: &str,
) -> Div {
    let has_error = !error_message.is_empty();

    let (text, color) = if has_error {
        (error_message.to_string(), theme::danger())
    } else {
        (status_message.to_string(), theme::text_muted())
    };

    h_flex()
        .w_full()
        .h(px(theme::STATUS_BAR_HEIGHT))
        .flex_shrink_0()
        .bg(theme::panel_bg())
        .border_t_1()
        .border_color(theme::border())
        .px(px(12.0))
        .items_center()
        .child(
            div()
                .text_size(px(11.0))
                .text_color(color)
                .child(text),
        )
}
