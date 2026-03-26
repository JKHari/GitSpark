use gpui::*;
use gpui_component::scroll::ScrollableElement;
use gpui_component::tag::Tag;
use gpui_component::{Icon, IconName, Sizable, h_flex, v_flex};

use crate::models::{ChangeEntry, CommitInfo};
use crate::ui::app::GitSparkApp;
use crate::ui::changes_context_menu;
use crate::ui::history_context_menu;
use crate::ui::theme;
use crate::ui::theme::z;
use crate::ui::ui_state::SidebarTab;

// ---------------------------------------------------------------------------
// Row heights (fixed, for uniform_list)
// ---------------------------------------------------------------------------

const CHANGE_ROW_HEIGHT: f32 = 29.0;
const HISTORY_ROW_HEIGHT: f32 = 40.0; // summary + meta + padding

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

    // Content — virtualized with uniform_list
    let content: AnyElement = match sidebar_tab {
        SidebarTab::Changes => {
            if changes.is_empty() {
                div().flex_1().child(render_empty_state("No changed files")).into_any_element()
            } else {
                let file_count = changes.len();
                let include_all = app.commit.include_all;

                // Include-all header: checkbox + "N changed files"
                let include_header = h_flex()
                    .w_full()
                    .h(z(28.0))
                    .px(z(10.0))
                    .items_center()
                    .gap(z(5.0))
                    .bg(theme::surface_bg())
                    .border_b_1()
                    .border_color(theme::border())
                    .flex_shrink_0()
                    .child({
                        let vh = view.clone();
                        render_checkbox(include_all)
                            .id("include-all-checkbox")
                            .cursor_pointer()
                            .on_click(move |_evt, _win, cx| {
                                vh.update(cx, |app, cx| {
                                    app.commit.include_all = !app.commit.include_all;
                                    cx.notify();
                                });
                            })
                    })
                    .child(
                        div()
                            .text_size(z(11.0))
                            .text_color(theme::text_muted())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{file_count} changed files")),
                    );

                let changes_snapshot: Vec<ChangeEntry> = changes.to_vec();
                let sel = selected_change.clone();
                v_flex().flex_1().min_h_0().child(include_header).child(
                div().id("changes-scroll").flex_1().min_h_0().overflow_y_scrollbar().child(
                    uniform_list("changes-list", changes_snapshot.len(), {
                        let view = view.clone();
                        move |range, _win, _cx| {
                            range
                                .map(|ix| {
                                    let change = &changes_snapshot[ix];
                                    let is_selected = sel.as_deref() == Some(change.path.as_str());
                                    let path = change.path.clone();
                                    let click_view = view.clone();
                                    let ctx_path = change.path.clone();
                                    changes_context_menu::bind_changes_context_click(
                                        render_change_row(change, is_selected)
                                            .id(SharedString::from(format!("change-{}", change.path)))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(theme::hover_bg()))
                                            .on_click(move |_evt, _win, cx| {
                                                let path = path.clone();
                                                click_view.update(cx, |app, cx| {
                                                    app.selection.selected_change = Some(path);
                                                    cx.notify();
                                                });
                                            }),
                                        view.clone(),
                                        ctx_path,
                                    )
                                    .into_any_element()
                                })
                                .collect()
                        }
                    })
                    .flex_1()
                    .with_sizing_behavior(ListSizingBehavior::Infer),
                ).into_any_element()
                ).into_any_element()
            }
        }
        SidebarTab::History => {
            if history.is_empty() {
                div().flex_1().child(render_empty_state("No history")).into_any_element()
            } else {
                let history_snapshot: Vec<CommitInfo> = history.to_vec();
                let sel = selected_commit.clone();
                div().id("history-scroll").flex_1().min_h_0().overflow_y_scrollbar().child(
                    uniform_list("history-list", history_snapshot.len(), {
                        let view = view.clone();
                        move |range, _win, _cx| {
                            range
                                .map(|ix| {
                                    let commit = &history_snapshot[ix];
                                    let is_selected = sel.as_deref() == Some(commit.oid.as_str());
                                    let oid = commit.oid.clone();
                                    let click_view = view.clone();
                                    history_context_menu::bind_history_context_click(
                                        render_history_row(commit, is_selected)
                                        .id(SharedString::from(format!("commit-{}", commit.oid)))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::hover_bg()))
                                        .on_click(move |_evt, _win, cx| {
                                            let oid = oid.clone();
                                            click_view.update(cx, |app, cx| {
                                                app.select_commit(oid, cx);
                                            });
                                        }),
                                        view.clone(),
                                        commit.oid.clone(),
                                    )
                                        .into_any_element()
                                })
                                .collect()
                        }
                    })
                    .flex_1()
                    .with_sizing_behavior(ListSizingBehavior::Infer),
                ).into_any_element()
            }
        }
    };

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
) -> Div {
    let is_changes = active_tab == SidebarTab::Changes;

    // Changes tab content
    let mut changes_content = h_flex().items_center().justify_center().gap(z(4.0)).child(
        div()
            .text_size(z(theme::FONT_SIZE))
            .text_color(if is_changes {
                theme::text_main()
            } else {
                theme::text_muted()
            })
            .font_weight(FontWeight::SEMIBOLD)
            .child("Changes"),
    );

    if change_count > 0 {
        changes_content = changes_content.child(
            div()
                .px(z(6.0))
                .py(z(1.0))
                .rounded(z(10.0))
                .bg(theme::toolbar_badge_bg())
                .text_size(z(theme::FONT_SIZE_XS))
                .text_color(theme::text_main())
                .child(change_count.to_string()),
        );
    }

    let changes_tab = h_flex()
        .id("tab-changes")
        .flex_1()
        .h(z(34.0))
        .items_center()
        .justify_center()
        .cursor_pointer()
        .border_b_2()
        .border_color(if is_changes {
            theme::accent()
        } else {
            gpui::transparent_black()
        })
        .hover(|s| s.bg(theme::hover_bg()))
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.sidebar_tab = SidebarTab::Changes;
            cx.notify();
        }))
        .child(changes_content);

    let history_tab = h_flex()
        .id("tab-history")
        .flex_1()
        .h(z(34.0))
        .items_center()
        .justify_center()
        .cursor_pointer()
        .border_b_2()
        .border_color(if !is_changes {
            theme::accent()
        } else {
            gpui::transparent_black()
        })
        .hover(|s| s.bg(theme::hover_bg()))
        .on_click(cx.listener(|app, _evt, _win, cx| {
            app.nav.sidebar_tab = SidebarTab::History;
            cx.notify();
        }))
        .child(
            div()
                .text_size(z(theme::FONT_SIZE))
                .text_color(if !is_changes {
                    theme::text_main()
                } else {
                    theme::text_muted()
                })
                .font_weight(FontWeight::SEMIBOLD)
                .child("History"),
        );

    h_flex()
        .w_full()
        .flex_shrink_0()
        .border_b_1()
        .border_color(theme::border())
        .child(changes_tab)
        .child(history_tab)
}

// ---------------------------------------------------------------------------
// Changes list
// ---------------------------------------------------------------------------

pub fn render_change_row(change: &ChangeEntry, selected: bool) -> Div {
    let bg = if selected {
        theme::hover_bg()
    } else {
        gpui::transparent_black()
    };

    let badge_label = status_label(&change.status);

    let text_color = if selected {
        gpui::white().into()
    } else {
        theme::text_main()
    };

    let checkbox = render_checkbox(true);

    h_flex()
        .w_full()
        .h(z(CHANGE_ROW_HEIGHT))
        .px(z(10.0))
        .items_center()
        .bg(bg)
        // Blue left border for selected file
        .border_l_2()
        .border_color(if selected {
            theme::accent()
        } else {
            gpui::transparent_black()
        })
        .gap(z(5.0))
        .child(checkbox)
        .child(
            div().flex_1().overflow_x_hidden().child(
                div()
                    .text_size(z(12.0))
                    .text_color(text_color)
                    .whitespace_nowrap()
                    .child(change.path.clone()),
            ),
        )
        .child(status_tag(badge_label))
}

fn render_checkbox(checked: bool) -> Div {
    let size = 14.0;
    if checked {
        div()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
            .bg(theme::accent())
            .border_1()
            .border_color(theme::accent())
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Check)
                    .size(z(10.0))
                    .text_color(gpui::white()),
            )
    } else {
        div()
            .w(z(size))
            .h(z(size))
            .rounded(z(3.0))
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

    let mut summary_row = h_flex().gap(z(6.0)).child(
        div().flex_1().overflow_x_hidden().child(
            div()
                .text_size(z(12.0))
                .text_color(summary_color)
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .child(commit.summary.clone()),
        ),
    );

    // Version tags
    for tag in &commit.tags {
        summary_row = summary_row.child(
            Tag::secondary().xsmall().child(tag.clone()),
        );
    }

    if commit.is_head {
        summary_row = summary_row.child(Tag::primary().xsmall().child("HEAD"));
    }

    v_flex()
        .w_full()
        .px(z(10.0))
        .py(z(6.0))
        .bg(bg)
        .border_b_1()
        .border_color(theme::toolbar_button_border())
        .gap(z(2.0))
        .child(summary_row)
        .child(
            div()
                .text_size(z(11.0))
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
        .py(z(20.0))
        .items_center()
        .justify_center()
        .child(
            div()
                .text_size(z(12.0))
                .text_color(theme::text_muted())
                .child(message.to_string()),
        )
}
