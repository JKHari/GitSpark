use gpui::*;
use gpui_component::divider::Divider;
use gpui_component::{h_flex, v_flex, Icon, IconName};

use crate::ui::theme;

// ---------------------------------------------------------------------------
// Section geometry
// ---------------------------------------------------------------------------

const SECTION_ICON_SIZE: f32 = 16.0;
const SECTION_INNER_PADDING: f32 = 10.0;
const SECTION_GAP: f32 = 10.0;
const CARET_ICON_SIZE: f32 = 10.0;
const BADGE_PILL_RADIUS: f32 = 8.0;

// ---------------------------------------------------------------------------
// Toolbar section builder (repo & branch)
// ---------------------------------------------------------------------------

/// Render a toolbar section with optional open/active state.
/// `is_open` shows the active background (dropdown open).
/// `is_in_progress` shows a loading icon instead of the caret.
pub fn render_toolbar_section(
    id: &str,
    icon_name: IconName,
    description: &str,
    title: &str,
    is_open: bool,
    is_in_progress: bool,
) -> Stateful<Div> {
    let title_row = h_flex().items_center().child(title_label(title));

    let bg = if is_open {
        theme::bg() // --toolbar-button-active-background-color
    } else {
        gpui::transparent_black()
    };

    let caret = if is_in_progress {
        div().flex_shrink_0().child(
            Icon::new(IconName::LoaderCircle)
                .size(px(CARET_ICON_SIZE))
                .text_color(theme::text_muted()),
        )
    } else if is_open {
        div().flex_shrink_0().child(
            Icon::new(IconName::ChevronUp)
                .size(px(CARET_ICON_SIZE))
                .text_color(theme::text_muted()),
        )
    } else {
        caret_icon()
    };

    h_flex()
        .id(SharedString::from(id.to_string()))
        .flex_1()
        .h_full()
        .items_center()
        .pl(px(SECTION_INNER_PADDING))
        .pr(px(SECTION_INNER_PADDING))
        .gap(px(SECTION_GAP))
        .cursor_pointer()
        .bg(bg)
        .hover(|style| style.bg(theme::toolbar_hover_bg()))
        // Icon
        .child(section_icon(icon_name))
        // Text stack: description on top, title on bottom
        .child(
            v_flex()
                .flex_1()
                .gap(px(2.0))
                .overflow_hidden()
                .child(description_label(description))
                .child(title_row),
        )
        // Caret / loading indicator
        .child(caret)
}

// ---------------------------------------------------------------------------
// Network section (split button)
// ---------------------------------------------------------------------------

/// Network section: main clickable area + separate caret dropdown zone.
///
/// Returns (main_area, caret_zone) so app.rs can wire separate click handlers.
pub fn render_network_parts(
    action_label: &str,
    ahead: usize,
    behind: usize,
    last_fetched: Option<&str>,
    is_in_flight: bool,
    show_dropdown: bool,
) -> (Stateful<Div>, Stateful<Div>) {
    let description = last_fetched
        .map(|v| format!("Last fetched {v}"))
        .unwrap_or_else(|| "Never fetched".to_string());

    // Build the badge row only when there is something to show.
    let badges: Option<Div> = if ahead > 0 || behind > 0 {
        let mut row = h_flex().gap(px(4.0));
        if ahead > 0 {
            row = row.child(count_badge(&format!("{ahead}\u{2191}")));
        }
        if behind > 0 {
            row = row.child(count_badge(&format!("{behind}\u{2193}")));
        }
        Some(row)
    } else {
        None
    };

    // Pick an icon that hints at the primary action.
    let icon = if is_in_flight {
        IconName::LoaderCircle
    } else {
        network_icon_for_label(action_label)
    };

    let title_row = {
        let mut row = h_flex()
            .items_center()
            .gap(px(6.0))
            .child(title_label(action_label));
        if let Some(b) = badges {
            row = row.child(b);
        }
        row
    };

    // Main clickable area (action label + badges)
    let main_area = h_flex()
        .id("network-main")
        .flex_1()
        .h_full()
        .items_center()
        .pl(px(SECTION_INNER_PADDING))
        .gap(px(SECTION_GAP))
        .cursor_pointer()
        .hover(|style| style.bg(theme::toolbar_hover_bg()))
        // Icon
        .child(section_icon(icon))
        // Text stack: title on top (action), description below (last fetched)
        .child(
            v_flex()
                .flex_1()
                .gap(px(2.0))
                .overflow_hidden()
                .child(title_row)
                .child(description_label(&description)),
        );

    // Caret dropdown zone (separate click target)
    let caret_bg = if show_dropdown {
        theme::bg()
    } else {
        gpui::transparent_black()
    };

    let caret_zone = div()
        .id("network-caret")
        .h_full()
        .flex_shrink_0()
        .px(px(8.0))
        .items_center()
        .justify_center()
        .cursor_pointer()
        .bg(caret_bg)
        .hover(|style| style.bg(theme::toolbar_hover_bg()))
        .border_l_1()
        .border_color(theme::toolbar_button_border())
        .child(
            Icon::new(if show_dropdown {
                IconName::ChevronUp
            } else {
                IconName::ChevronDown
            })
            .size(px(CARET_ICON_SIZE))
            .text_color(theme::text_muted()),
        );

    (main_area, caret_zone)
}

// ---------------------------------------------------------------------------
// Reusable micro-elements
// ---------------------------------------------------------------------------

/// Section icon (16px, white).
fn section_icon(name: IconName) -> Div {
    div().flex_shrink_0().child(
        Icon::new(name)
            .size(px(SECTION_ICON_SIZE))
            .text_color(theme::text_main()),
    )
}

/// Caret-down indicator for a section.
fn caret_icon() -> Div {
    div().flex_shrink_0().child(
        Icon::new(IconName::ChevronDown)
            .size(px(CARET_ICON_SIZE))
            .text_color(theme::text_muted()),
    )
}

/// 11px muted description text.
fn description_label(text: &str) -> Div {
    div()
        .text_size(px(theme::FONT_SIZE_SM))
        .text_color(theme::text_muted())
        .overflow_x_hidden()
        .whitespace_nowrap()
        .child(text.to_string())
}

/// 12px bold white title text.
fn title_label(text: &str) -> Div {
    div()
        .text_size(px(theme::FONT_SIZE))
        .text_color(theme::text_main())
        .font_weight(FontWeight::SEMIBOLD)
        .overflow_x_hidden()
        .whitespace_nowrap()
        .child(text.to_string())
}

/// Small pill badge showing a count string (e.g. "2↑").
fn count_badge(text: &str) -> Div {
    div()
        .px(px(6.0))
        .py(px(2.0))
        .rounded(px(BADGE_PILL_RADIUS))
        .bg(theme::toolbar_badge_bg())
        .text_size(px(theme::FONT_SIZE_XS))
        .text_color(theme::text_main())
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_string())
}

// ---------------------------------------------------------------------------
// Vertical divider
// ---------------------------------------------------------------------------

pub fn vertical_divider() -> Divider {
    Divider::vertical().color(theme::toolbar_button_border())
}

// ---------------------------------------------------------------------------
// Icon mapping
// ---------------------------------------------------------------------------

fn network_icon_for_label(label: &str) -> IconName {
    let lower = label.to_ascii_lowercase();
    if lower.starts_with("push") {
        IconName::ArrowUp
    } else if lower.starts_with("pull") {
        IconName::ArrowDown
    } else {
        IconName::Loader
    }
}
