use gpui::*;
use gpui_component::{h_flex, v_flex};

use crate::models::DiffEntry;
use crate::ui::theme;
use crate::ui::theme::z;

// --- Intra-line character range ---

const MAX_INTRA_LINE_CHARS: usize = 1024;

#[derive(Clone, Debug)]
struct CharRange {
    start: usize,
    end: usize,
}

// --- Diff line classification ---

#[derive(Clone, Debug)]
enum DiffLineKind {
    Context,
    Added,
    Deleted,
    HunkHeader,
    /// A modified line has paired old/new content with optional intra-line highlights.
    Modified {
        old_highlight: Option<CharRange>,
        new_highlight: Option<CharRange>,
    },
}

#[derive(Clone, Debug)]
struct DiffLine {
    kind: DiffLineKind,
    content: String,
    /// For Modified lines, the new (added) content.
    new_content: Option<String>,
    old_line: Option<usize>,
    new_line: Option<usize>,
}

// --- Intermediate parsed line used during pairing ---

#[derive(Clone, Debug)]
struct RawDiffLine {
    is_add: bool,
    is_del: bool,
    content: String,
    old_line: Option<usize>,
    new_line: Option<usize>,
}

/// Parse a unified diff string into classified lines with line numbers.
///
/// Detects paired delete/add blocks of equal length and converts them to
/// `Modified` lines with intra-line character-level diff highlights.
fn parse_diff(raw: &str) -> Vec<DiffLine> {
    // First pass: parse into RawDiffLines and hunk headers
    let mut raw_lines: Vec<Option<RawDiffLine>> = Vec::new(); // None = hunk header
    let mut hunk_header_texts: Vec<String> = Vec::new();
    let mut old_num: usize = 0;
    let mut new_num: usize = 0;

    for raw_line in raw.lines() {
        if raw_line.starts_with("@@") {
            if let Some(rest) = raw_line.strip_prefix("@@ -") {
                let parts: Vec<&str> = rest.splitn(2, " +").collect();
                if parts.len() == 2 {
                    if let Some(old_start_str) = parts[0].split(',').next() {
                        old_num = old_start_str.parse().unwrap_or(1);
                    }
                    let new_part = parts[1].split(' ').next().unwrap_or("");
                    if let Some(new_start_str) = new_part.split(',').next() {
                        new_num = new_start_str.parse().unwrap_or(1);
                    }
                }
            }
            hunk_header_texts.push(raw_line.to_string());
            raw_lines.push(None); // sentinel for hunk header
        } else if raw_line.starts_with("diff --git")
            || raw_line.starts_with("index ")
            || raw_line.starts_with("+++")
            || raw_line.starts_with("---")
        {
            // Meta / file header lines — skip
            continue;
        } else if raw_line.starts_with('+') {
            raw_lines.push(Some(RawDiffLine {
                is_add: true,
                is_del: false,
                content: raw_line[1..].to_string(),
                old_line: None,
                new_line: Some(new_num),
            }));
            new_num += 1;
        } else if raw_line.starts_with('-') {
            raw_lines.push(Some(RawDiffLine {
                is_add: false,
                is_del: true,
                content: raw_line[1..].to_string(),
                old_line: Some(old_num),
                new_line: None,
            }));
            old_num += 1;
        } else {
            let content = raw_line.strip_prefix(' ').unwrap_or(raw_line);
            raw_lines.push(Some(RawDiffLine {
                is_add: false,
                is_del: false,
                content: content.to_string(),
                old_line: Some(old_num),
                new_line: Some(new_num),
            }));
            old_num += 1;
            new_num += 1;
        }
    }

    // Second pass: detect paired delete/add blocks and build final DiffLines
    let mut result = Vec::new();
    let mut hunk_idx = 0usize;
    let mut i = 0usize;

    while i < raw_lines.len() {
        let Some(ref line) = raw_lines[i] else {
            // Hunk header
            result.push(DiffLine {
                kind: DiffLineKind::HunkHeader,
                content: hunk_header_texts.get(hunk_idx).cloned().unwrap_or_default(),
                new_content: None,
                old_line: None,
                new_line: None,
            });
            hunk_idx += 1;
            i += 1;
            continue;
        };

        if line.is_del {
            // Collect contiguous deleted lines
            let del_start = i;
            while i < raw_lines.len() && raw_lines[i].as_ref().map(|l| l.is_del).unwrap_or(false) {
                i += 1;
            }
            let del_end = i;

            // Collect contiguous added lines that follow
            let add_start = i;
            while i < raw_lines.len() && raw_lines[i].as_ref().map(|l| l.is_add).unwrap_or(false) {
                i += 1;
            }
            let add_end = i;

            let del_count = del_end - del_start;
            let add_count = add_end - add_start;

            if del_count > 0 && del_count == add_count {
                // Paired: create Modified lines
                for j in 0..del_count {
                    let del_line = raw_lines[del_start + j].as_ref().unwrap();
                    let add_line = raw_lines[add_start + j].as_ref().unwrap();
                    let (old_highlight, new_highlight) =
                        find_changed_ranges(&del_line.content, &add_line.content);
                    result.push(DiffLine {
                        kind: DiffLineKind::Modified {
                            old_highlight,
                            new_highlight,
                        },
                        content: del_line.content.clone(),
                        new_content: Some(add_line.content.clone()),
                        old_line: del_line.old_line,
                        new_line: add_line.new_line,
                    });
                }
            } else {
                // Unpaired: emit as separate Deleted and Added
                for j in del_start..del_end {
                    let l = raw_lines[j].as_ref().unwrap();
                    result.push(DiffLine {
                        kind: DiffLineKind::Deleted,
                        content: l.content.clone(),
                        new_content: None,
                        old_line: l.old_line,
                        new_line: None,
                    });
                }
                for j in add_start..add_end {
                    let l = raw_lines[j].as_ref().unwrap();
                    result.push(DiffLine {
                        kind: DiffLineKind::Added,
                        content: l.content.clone(),
                        new_content: None,
                        old_line: None,
                        new_line: l.new_line,
                    });
                }
            }
        } else if line.is_add {
            // Standalone added line (not preceded by deletes)
            result.push(DiffLine {
                kind: DiffLineKind::Added,
                content: line.content.clone(),
                new_content: None,
                old_line: None,
                new_line: line.new_line,
            });
            i += 1;
        } else {
            // Context line
            result.push(DiffLine {
                kind: DiffLineKind::Context,
                content: line.content.clone(),
                new_content: None,
                old_line: line.old_line,
                new_line: line.new_line,
            });
            i += 1;
        }
    }

    result
}

/// Find the character ranges that differ between two lines.
///
/// Returns `(old_range, new_range)` where each range marks the changed
/// character span. If a line is too long, returns `(None, None)`.
fn find_changed_ranges(old_line: &str, new_line: &str) -> (Option<CharRange>, Option<CharRange>) {
    let old_chars: Vec<char> = old_line.chars().collect();
    let new_chars: Vec<char> = new_line.chars().collect();

    if old_chars.len() > MAX_INTRA_LINE_CHARS || new_chars.len() > MAX_INTRA_LINE_CHARS {
        return (None, None);
    }

    // Find common prefix length
    let mut prefix = 0usize;
    while prefix < old_chars.len()
        && prefix < new_chars.len()
        && old_chars[prefix] == new_chars[prefix]
    {
        prefix += 1;
    }

    // Find common suffix length
    let mut old_suffix = 0usize;
    let mut new_suffix = 0usize;
    while old_suffix < old_chars.len().saturating_sub(prefix)
        && new_suffix < new_chars.len().saturating_sub(prefix)
        && old_chars[old_chars.len() - 1 - old_suffix]
            == new_chars[new_chars.len() - 1 - new_suffix]
    {
        old_suffix += 1;
        new_suffix += 1;
    }

    let old_end = old_chars.len().saturating_sub(old_suffix);
    let new_end = new_chars.len().saturating_sub(new_suffix);

    let old_range = (prefix < old_end).then_some(CharRange {
        start: prefix,
        end: old_end,
    });
    let new_range = (prefix < new_end).then_some(CharRange {
        start: prefix,
        end: new_end,
    });

    (old_range, new_range)
}

// --- Rendering ---

/// Brighter highlight background for intra-line changed characters.
fn diff_add_highlight_bg() -> Hsla {
    gpui::rgb(0x1a5c2e).into()
}

fn diff_del_highlight_bg() -> Hsla {
    gpui::rgb(0x6e2b25).into()
}

/// Render text with an optional highlighted character range.
///
/// Splits the text into up to 3 spans: before, highlighted, after.
fn render_highlighted_text(
    text: &str,
    highlight: Option<&CharRange>,
    base_color: Hsla,
    highlight_bg: Hsla,
) -> Div {
    let Some(range) = highlight else {
        return div().text_color(base_color).child(text.to_string());
    };

    let chars: Vec<char> = text.chars().collect();
    let before: String = chars[..range.start.min(chars.len())].iter().collect();
    let mid: String = chars[range.start.min(chars.len())..range.end.min(chars.len())]
        .iter()
        .collect();
    let after: String = chars[range.end.min(chars.len())..].iter().collect();

    h_flex()
        .child(div().text_color(base_color).child(before))
        .child(div().text_color(base_color).bg(highlight_bg).child(mid))
        .child(div().text_color(base_color).child(after))
}

/// Render a single diff line as a horizontal flex row.
fn render_diff_line(line: &DiffLine) -> Div {
    // Format line number strings. Hunk headers show no numbers.
    let old_num_str = match line.old_line {
        Some(n) => format!("{n}"),
        None => String::new(),
    };
    let new_num_str = match line.new_line {
        Some(n) => format!("{n}"),
        None => String::new(),
    };

    let mut row = h_flex()
        .w_full()
        .min_h(z(theme::DIFF_ROW_HEIGHT))
        .flex_shrink_0()
        .font_family("monospace")
        .text_size(z(12.0))
        .py(z(2.0)); // match GitHub Desktop: padding 2px 0

    // Old line number gutter
    row = row.child(
        div()
            .w(z(theme::DIFF_LINE_NUM_WIDTH))
            .flex_shrink_0()
            .text_color(theme::line_num_color())
            .px(z(4.0))
            .child(old_num_str),
    );

    // New line number gutter
    row = row.child(
        div()
            .w(z(theme::DIFF_LINE_NUM_WIDTH))
            .flex_shrink_0()
            .text_color(theme::line_num_color())
            .px(z(4.0))
            .child(new_num_str),
    );

    // Content — varies by line kind
    match &line.kind {
        DiffLineKind::Added => {
            row = row.bg(theme::diff_add_bg()).child(
                div()
                    .flex_1()
                    .pl(z(5.0))
                    .text_color(theme::diff_add_fg())
                    .child(line.content.clone()),
            );
        }
        DiffLineKind::Deleted => {
            row = row.bg(theme::diff_del_bg()).child(
                div()
                    .flex_1()
                    .pl(z(5.0))
                    .text_color(theme::diff_del_fg())
                    .child(line.content.clone()),
            );
        }
        DiffLineKind::HunkHeader => {
            row = row.bg(theme::diff_hunk_bg()).child(
                div()
                    .flex_1()
                    .pl(z(5.0))
                    .text_color(theme::text_muted())
                    .child(line.content.clone()),
            );
        }
        DiffLineKind::Context => {
            row = row.child(
                div()
                    .flex_1()
                    .pl(z(5.0))
                    .text_color(theme::text_main()) // --diff-text-color: var(--text-color)
                    .child(line.content.clone()),
            );
        }
        DiffLineKind::Modified {
            old_highlight,
            new_highlight,
        } => {
            // Side-by-side old | new within the content area
            let old_content = render_highlighted_text(
                &line.content,
                old_highlight.as_ref(),
                theme::diff_del_fg(),
                diff_del_highlight_bg(),
            );
            let new_text = line.new_content.as_deref().unwrap_or("");
            let new_content = render_highlighted_text(
                new_text,
                new_highlight.as_ref(),
                theme::diff_add_fg(),
                diff_add_highlight_bg(),
            );
            row = row.child(
                h_flex()
                    .flex_1()
                    .child(
                        div()
                            .flex_1()
                            .pl(z(8.0))
                            .bg(theme::diff_del_bg())
                            .child(old_content),
                    )
                    .child(
                        div()
                            .flex_1()
                            .pl(z(8.0))
                            .bg(theme::diff_add_bg())
                            .child(new_content),
                    ),
            );
        }
    }

    row
}

/// Render the diff header bar showing the selected file path.
fn render_diff_header(file_path: &str) -> Div {
    h_flex()
        .w_full()
        .h(z(theme::DIFF_HEADER_HEIGHT))
        .flex_shrink_0()
        .bg(theme::surface_bg())
        .border_b_1()
        .border_color(theme::border())
        .px(z(14.0))
        .items_center()
        .child(
            div()
                .text_color(theme::text_main())
                .text_size(z(12.0))
                .child(file_path.to_string()),
        )
}

/// Render the empty state when no file is selected.
fn render_empty_state() -> Div {
    div()
        .w_full()
        .h_full()
        .flex_1()
        .bg(theme::bg())
        .items_center()
        .justify_center()
        .child(
            div()
                .text_color(theme::text_muted())
                .text_size(z(14.0))
                .child("Select a file to view its diff"),
        )
}

/// Render the workspace diff viewer.
///
/// Fills the remaining horizontal space (flex-1) and displays either
/// a unified diff for the selected file or a placeholder message.
pub fn render_workspace(selected_file: Option<&str>, diff: Option<&DiffEntry>) -> Div {
    let Some(file_path) = selected_file else {
        return render_empty_state();
    };

    let diff_content: AnyElement = match diff {
        Some(entry) if entry.is_binary => div()
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(z(14.0))
                    .child("Binary file changed."),
            )
            .into_any_element(),
        Some(entry) if entry.diff.trim().is_empty() => div()
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(z(14.0))
                    .child("No diff text available."),
            )
            .into_any_element(),
        Some(entry) => {
            let parsed = parse_diff(&entry.diff);
            let mut scroll_content = v_flex().w_full();
            for line in &parsed {
                scroll_content = scroll_content.child(render_diff_line(line));
            }

            v_flex()
                .id("diff-scroll")
                .w_full()
                .flex_1()
                .overflow_y_scroll()
                .child(scroll_content)
                .into_any_element()
        }
        None => div()
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_color(theme::text_muted())
                    .text_size(z(14.0))
                    .child("No diff available for this file."),
            )
            .into_any_element(),
    };

    v_flex()
        .w_full()
        .h_full()
        .flex_1()
        .items_start()
        .bg(theme::bg())
        .child(render_diff_header(file_path))
        .child(diff_content)
}
