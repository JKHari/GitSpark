use gpui::*;
use gpui_component::divider::Divider;
use gpui_component::{h_flex, v_flex, Icon, IconName};

use crate::ui::theme;

// ---------------------------------------------------------------------------
// Section geometry
// ---------------------------------------------------------------------------

const SECTION_WIDTH: f32 = 230.0;
const SECTION_ICON_SIZE: f32 = 16.0;
const SECTION_INNER_PADDING: f32 = 10.0;
const SECTION_GAP: f32 = 10.0;
const CARET_ICON_SIZE: f32 = 10.0;
const BADGE_PILL_RADIUS: f32 = 8.0;

// ---------------------------------------------------------------------------
// Public render entry point
// ---------------------------------------------------------------------------

/// Render the application toolbar as a pure visual element tree.
///
/// Three sections side-by-side (repository, branch, network) separated by
/// 1px vertical dividers. Each section shows an icon, a description label,
/// a title, and a caret-down indicator.
pub fn render_toolbar(
    repo_name: &str,
    branch_name: &str,
    ahead: usize,
    behind: usize,
    last_fetched: Option<&str>,
    network_action_label: &str,
) -> Div {
    h_flex()
        .w_full()
        .h(px(theme::TOOLBAR_HEIGHT))
        .flex_shrink_0()
        .bg(theme::toolbar_bg())
        .border_b_1()
        .border_color(theme::toolbar_button_border())
        // --- Repository section ---
        .child(render_section(
            IconName::FolderOpen,
            "Current Repository",
            repo_name,
            None,
        ))
        .child(vertical_divider())
        // --- Branch section ---
        .child(render_section(
            IconName::GitHub,
            "Current Branch",
            branch_name,
            None,
        ))
        .child(vertical_divider())
        // --- Network section ---
        .child(render_network_section(
            network_action_label,
            ahead,
            behind,
            last_fetched,
        ))
}

// ---------------------------------------------------------------------------
// Shared section builder
// ---------------------------------------------------------------------------

/// Render the repository section of the toolbar.
pub fn render_repo_section(repo_name: &str) -> Stateful<Div> {
    render_section(IconName::FolderOpen, "Current Repository", repo_name, None)
}

/// Render the branch section of the toolbar.
pub fn render_branch_section(branch_name: &str) -> Stateful<Div> {
    render_section(IconName::GitHub, "Current Branch", branch_name, None)
}

/// A single toolbar section: icon | text stack (description + title) | caret.
fn render_section(
    icon_name: IconName,
    description: &str,
    title: &str,
    badges: Option<Div>,
) -> Stateful<Div> {
    let title_row = if let Some(badge_element) = badges {
        h_flex()
            .items_center()
            .gap(px(6.0))
            .child(title_label(title))
            .child(badge_element)
    } else {
        h_flex().items_center().child(title_label(title))
    };

    h_flex()
        .id(SharedString::from(format!("section-{description}")))
        .w(px(SECTION_WIDTH))
        .h_full()
        .items_center()
        .pl(px(SECTION_INNER_PADDING))
        .pr(px(SECTION_INNER_PADDING))
        .gap(px(SECTION_GAP))
        .cursor_pointer()
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
        // Caret down indicator
        .child(caret_icon())
}

// ---------------------------------------------------------------------------
// Network section
// ---------------------------------------------------------------------------

/// The network/fetch section with action label, ahead/behind badges,
/// and a "last fetched" description line.
pub fn render_network_section(
    action_label: &str,
    ahead: usize,
    behind: usize,
    last_fetched: Option<&str>,
) -> Stateful<Div> {
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
    let icon = network_icon_for_label(action_label);

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

    h_flex()
        .id("section-network")
        .flex_1()
        .h_full()
        .items_center()
        .pl(px(SECTION_INNER_PADDING))
        .pr(px(SECTION_INNER_PADDING))
        .gap(px(SECTION_GAP))
        .cursor_pointer()
        .hover(|style| style.bg(theme::toolbar_hover_bg()))
        // Icon
        .child(section_icon(icon))
        // Text stack: description on top, title on bottom (same order as repo/branch)
        .child(
            v_flex()
                .flex_1()
                .gap(px(2.0))
                .overflow_hidden()
                .child(description_label(&description))
                .child(title_row),
        )
        // Caret down (no extra divider — matches original)
        .child(caret_icon())
}

// ---------------------------------------------------------------------------
// Reusable micro-elements
// ---------------------------------------------------------------------------

/// Section icon (16px, white — matches --toolbar-button-color).
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

/// 11px muted description text (e.g. "Current Repository", "Last fetched...").
fn description_label(text: &str) -> Div {
    div()
        .text_size(px(theme::FONT_SIZE_SM))
        .text_color(theme::text_muted())
        .overflow_x_hidden()
        .whitespace_nowrap()
        .child(text.to_string())
}

/// 12px bold white title text, CSS-style overflow truncation.
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

/// Vertical line separating toolbar sections.
pub fn vertical_divider() -> Divider {
    Divider::vertical().color(theme::toolbar_button_border())
}

// ---------------------------------------------------------------------------
// Icon mapping
// ---------------------------------------------------------------------------

/// Map a network action label to an appropriate icon.
fn network_icon_for_label(label: &str) -> IconName {
    let lower = label.to_ascii_lowercase();
    if lower.starts_with("push") {
        IconName::ArrowUp
    } else if lower.starts_with("pull") {
        IconName::ArrowDown
    } else {
        // Fetch or any other default action
        IconName::Loader
    }
}
