use gpui::*;
use gpui_component::tab::{Tab, TabBar};
use gpui_component::tag::Tag;
use gpui_component::{h_flex, v_flex, Sizable};

use crate::models::{ChangeEntry, CommitInfo};
use crate::ui::app::GitSparkApp;
use crate::ui::theme;
use crate::ui::ui_state::SidebarTab;

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

fn accent_selection_bg() -> Hsla {
    theme::with_alpha(theme::accent_muted(), 0.2)
}

// ---------------------------------------------------------------------------
// Status helpers
// ---------------------------------------------------------------------------

fn status_tag(label: &str) -> Tag {
    let tag = match label {
        "A" => Tag::success(),
        "M" => Tag::warning(),
        "D" => Tag::danger(),
        _ => Tag::secondary(),
    };
    tag.outline().xsmall().child(label.to_string())
}

fn status_label(status: &str) -> &'static str {
    if status.contains('?') || status.contains('A') {
        "A"
    } else if status.contains('M') {
        "M"
    } else if status.contains('D') {
        "D"
    } else if status.contains('U') {
        "U"
    } else {
        "?"
    }
}

// ---------------------------------------------------------------------------
// Public render entry point (interactive, with click handlers)
// ---------------------------------------------------------------------------

pub fn render_sidebar_interactive(
    app: &GitSparkApp,
    view: Entity<GitSparkApp>,
    cx: &mut Context<GitSparkApp>,
) -> Div {
    let snapshot = app.repo.snapshot.as_ref();
    let empty_changes: Vec<ChangeEntry> = vec![];
    let empty_history: Vec<CommitInfo> = vec![];
    let changes = snapshot
        .map(|s| s.changes.as_slice())
        .unwrap_or(&empty_changes);
    let history = snapshot
        .map(|s| s.history.as_slice())
        .unwrap_or(&empty_history);
    let sidebar_tab = app.nav.sidebar_tab;
    let selected_change = app.selection.selected_change.clone();
    let selected_commit = app.selection.selected_commit.clone();
    let change_count = changes.len();

    // Tab bar with click handlers
    let tab_bar = render_interactive_tab_bar(sidebar_tab, change_count, cx);

    // Content list with click handlers.
    // The inner wrapper is a plain column that sizes to its children.
    // The outer scroll container takes the remaining sidebar height.
    let mut inner = div().flex().flex_col().w_full();

    match sidebar_tab {
        SidebarTab::Changes => {
            if changes.is_empty() {
                inner = inner.child(render_empty_state("No changed files"));
            } else {
                for change in changes {
                    let is_selected =
                        selected_change.as_deref() == Some(change.path.as_str());
                    let path = change.path.clone();
                    let vh = view.clone();
                    inner = inner.child(
                        render_change_row(change, is_selected)
                            .id(SharedString::from(format!("change-{}", change.path)))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover_bg()))
                            .on_click(move |_evt, _win, cx| {
                                let path = path.clone();
                                vh.update(cx, |app, cx| {
                                    app.selection.selected_change = Some(path);
                                    cx.notify();
                                });
                            }),
                    );
                }
            }
        }
        SidebarTab::History => {
            if history.is_empty() {
                inner = inner.child(render_empty_state("No history"));
            } else {
                for commit in history {
                    let is_selected =
                        selected_commit.as_deref() == Some(commit.oid.as_str());
                    let oid = commit.oid.clone();
                    let vh = view.clone();
                    inner = inner.child(
                        render_history_row(commit, is_selected)
                            .id(SharedString::from(format!("commit-{}", commit.oid)))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover_bg()))
                            .on_click(move |_evt, _win, cx| {
                                let oid = oid.clone();
                                vh.update(cx, |app, cx| {
                                    app.select_commit(oid, cx);
                                });
                            }),
                    );
                }
            }
        }
    }

    let content = div()
        .id("sidebar-content")
        .flex_1()
        .overflow_y_scroll()
        .child(inner);

    v_flex()
        .size_full()
        .bg(theme::panel_bg())
        .border_r_1()
        .border_color(theme::border())
        .child(tab_bar)
        .child(content)
}

fn render_interactive_tab_bar(
    active_tab: SidebarTab,
    change_count: usize,
    cx: &mut Context<GitSparkApp>,
) -> impl IntoElement {
    let selected_index = match active_tab {
        SidebarTab::Changes => 0,
        SidebarTab::History => 1,
    };

    let mut changes_tab = Tab::new()
        .label("Changes")
        .small();

    if change_count > 0 {
        // Inline counter pill matching GitHub Desktop's --tab-bar-count style
        changes_tab = changes_tab.suffix(
            div()
                .px(px(6.0))
                .py(px(1.0))
                .rounded(px(10.0))
                .bg(theme::toolbar_badge_bg()) // --tab-bar-count-background-color: $gray-700
                .text_size(px(theme::FONT_SIZE_XS))
                .text_color(theme::text_main()) // --tab-bar-count-color: var(--text-color)
                .child(change_count.to_string()),
        );
    }

    let history_tab = Tab::new()
        .label("History")
        .small();

    TabBar::new("sidebar-tabs")
        .underline()
        .small()
        .selected_index(selected_index)
        .child(changes_tab)
        .child(history_tab)
        .on_click(cx.listener(move |app, index, _win, cx| {
            app.nav.sidebar_tab = match index {
                0 => SidebarTab::Changes,
                _ => SidebarTab::History,
            };
            cx.notify();
        }))
}

// ---------------------------------------------------------------------------
// Changes list
// ---------------------------------------------------------------------------

pub fn render_change_row(change: &ChangeEntry, selected: bool) -> Div {
    let bg = if selected {
        accent_selection_bg()
    } else {
        gpui::transparent_black()
    };

    let badge_label = status_label(&change.status);

    let text_color = if selected {
        gpui::white().into()
    } else {
        theme::text_main()
    };

    // Checkbox: filled blue when selected, outline when not
    let checkbox = render_checkbox(true); // all files included for now

    h_flex()
        .w_full()
        .h(px(29.0)) // match GitHub Desktop --tab-bar-height row sizing
        .px(px(10.0))
        .items_center()
        .bg(bg)
        .border_b_1()
        .border_color(theme::toolbar_button_border())
        .gap(px(5.0))
        // Checkbox
        .child(checkbox)
        // File path — CSS-style overflow truncation
        .child(
            div()
                .flex_1()
                .overflow_x_hidden()
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(text_color)
                        .whitespace_nowrap()
                        .child(change.path.clone()),
                ),
        )
        // Status tag (A/M/D)
        .child(status_tag(badge_label))
}

/// Render a checkbox visual (non-interactive for now).
fn render_checkbox(checked: bool) -> Div {
    let size = 14.0;
    if checked {
        div()
            .w(px(size))
            .h(px(size))
            .rounded(px(3.0))
            .bg(theme::accent())
            .border_1()
            .border_color(theme::accent())
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(gpui::white())
                    .child("\u{2713}"), // ✓
            )
    } else {
        div()
            .w(px(size))
            .h(px(size))
            .rounded(px(3.0))
            .border_1()
            .border_color(theme::text_muted())
            .flex_shrink_0()
    }
}

// ---------------------------------------------------------------------------
// History list
// ---------------------------------------------------------------------------

pub fn render_history_row(commit: &CommitInfo, selected: bool) -> Div {
    let bg = if selected {
        accent_selection_bg()
    } else {
        gpui::transparent_black()
    };

    let summary_color = if selected {
        gpui::white().into()
    } else {
        theme::text_main()
    };

    let meta = format!("{} \u{00b7} {}", commit.author_name, commit.date);

    let mut summary_row = h_flex()
        .gap(px(6.0))
        .child(
            div()
                .flex_1()
                .overflow_x_hidden()
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(summary_color)
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .child(commit.summary.clone()),
                ),
        );

    if commit.is_head {
        summary_row = summary_row.child(
            Tag::primary()
                .xsmall()
                .child("HEAD"),
        );
    }

    v_flex()
        .w_full()
        .px(px(10.0))
        .py(px(6.0))
        .bg(bg)
        .border_b_1()
        .border_color(theme::toolbar_button_border())
        .gap(px(2.0))
        .child(summary_row)
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme::text_muted())
                .whitespace_nowrap()
                .child(meta),
        )
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn render_empty_state(message: &str) -> Div {
    div()
        .w_full()
        .py(px(20.0))
        .items_center()
        .justify_center()
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme::text_muted())
                .child(message.to_string()),
        )
}
