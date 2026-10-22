use crate::app::App;
use crate::ask::context::Side;
use crate::git::diff::{DiffFile, DiffLine};
use crate::highlight::engine::expand_tabs;
use crate::mode::{DiffHighlight, Mode};
use crate::selection::{mark_selected, ContentRows, ContentTarget, Selection};
use crate::ui::{ask_panel, diff_words, layout, wrap};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Tabs};

pub fn render_diff(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let (header, body, footer) = layout::app_layout(area);
    let (body, ask_area) = ask_panel::split(body, app.ask_panel.is_open());
    let mut content_rows = None;
    app.ensure_diff_highlight();

    if let Mode::Diff(state) = &app.mode {
        let palette = &app.palette;
        let title = format!("DIFF: {} ↦ {}", state.from.short_id, state.to.short_id);
        let timestamp = layout::format_header_date(state.to.date);
        layout::render_header(
            frame,
            header,
            palette,
            &title,
            &timestamp,
            Some(&state.to.message),
        );

        if state.diff_result.files.is_empty() {
            let empty = Paragraph::new("No diff").block(Block::bordered());
            frame.render_widget(empty, body);
        } else {
            let [tabs_row, diff_area] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(body);

            let file_names: Vec<String> = state
                .diff_result
                .files
                .iter()
                .map(|f| {
                    f.change
                        .as_ref()
                        .map(|c| c.path())
                        .unwrap_or("?")
                        .to_string()
                })
                .collect();

            let (tab_offset, visible_names) =
                visible_tabs(&file_names, state.selected_file, tabs_row.width);

            let adjusted_select = state.selected_file.saturating_sub(tab_offset);

            let tabs = Tabs::new(visible_names)
                .select(adjusted_select)
                .highlight_style(
                    Style::new()
                        .fg(palette.fg)
                        .add_modifier(Modifier::BOLD)
                        .add_modifier(Modifier::UNDERLINED),
                )
                .divider("|");
            frame.render_widget(tabs, tabs_row);

            if let Some(file) = state.diff_result.files.get(state.selected_file) {
                frame.render_widget(Clear, diff_area);
                let word_wrap = app.config.ui.word_wrap;
                let target = ContentTarget::Diff {
                    to: state.to.id,
                    file: state.selected_file,
                    side_by_side: state.side_by_side,
                };
                let selection = app.selection.as_ref().filter(|s| s.target == target);
                let inner = diff_area.inner(ratatui::layout::Margin::new(1, 1));
                let colors = DiffColors {
                    palette,
                    advanced: app.diff_highlight.as_ref().filter(|_| app.advanced_diff()),
                };
                let (rows, panes) = if state.side_by_side {
                    let (rows, panes) = render_side_by_side(
                        frame,
                        diff_area,
                        file,
                        state.scroll,
                        word_wrap,
                        colors,
                        selection,
                    );
                    (rows, Some(panes))
                } else {
                    let rows = render_unified(
                        frame,
                        diff_area,
                        file,
                        state.scroll,
                        word_wrap,
                        colors,
                        selection,
                    );
                    (rows, None)
                };
                content_rows = Some(ContentRows {
                    target,
                    inner,
                    panes,
                    rows,
                });
            }
        }
    }
    app.content_rows = content_rows;
    if let Some(area) = ask_area {
        ask_panel::render(frame, area, app);
    } else {
        app.ask_panel_area = None;
    }

    // With the answer panel open, j/k scroll it instead (see ask_hints).
    let scroll_keys = if app.ask_panel.is_open() {
        "[u/d]"
    } else {
        "[j/k/u/d]"
    };
    let mut hints = vec![
        ("[h/l/←/→]", "file"),
        (scroll_keys, "scroll"),
        ("[J/K/↑/↓]", "page"),
        ("[^P/^N]", "commit"),
        ("[w]", "wrap"),
        ("[c]", "color"),
        ("[s]", "view"),
        ("[Tab]", "back"),
        ("[Esc]", "pick"),
    ];
    hints.extend(layout::ask_hints(app));
    if app.repo_changed {
        hints.push(("[!]", "repo changed"));
    }
    layout::render_footer(frame, footer, &app.palette, &hints);
}

fn visible_tabs(file_names: &[String], selected: usize, area_width: u16) -> (usize, Vec<String>) {
    if file_names.is_empty() {
        return (0, vec![]);
    }

    let width = area_width as usize;
    let divider_len = " | ".len();

    let max_fit = {
        let mut n = 0;
        let mut w = 0;
        for name in file_names {
            let need = if n == 0 {
                name.len()
            } else {
                divider_len + name.len()
            };
            if w + need > width {
                break;
            }
            w += need;
            n += 1;
        }
        n.max(1)
    };

    let right_margin = match max_fit {
        0 | 1 => 0,
        2..=4 => 1,
        _ => 2,
    };

    // 1. selected tab
    let mut used = file_names[selected].len();

    // 2. ensure at least right_margin tabs to the right
    let mut right_count = 0;
    for i in 1..=right_margin {
        let idx = selected + i;
        if idx >= file_names.len() {
            break;
        }
        if used + divider_len + file_names[idx].len() > width {
            break;
        }
        used += divider_len + file_names[idx].len();
        right_count = i;
    }

    // 3. fill left with as many as fit
    let mut left_count = 0;
    for i in (0..selected).rev() {
        if used + divider_len + file_names[i].len() > width {
            break;
        }
        used += divider_len + file_names[i].len();
        left_count += 1;
    }

    // 4. fill any remaining space with more right tabs
    for i in (right_count + 1).. {
        let idx = selected + i;
        if idx >= file_names.len() {
            break;
        }
        if used + divider_len + file_names[idx].len() > width {
            break;
        }
        used += divider_len + file_names[idx].len();
        right_count = i;
    }

    let offset = selected - left_count;
    let end = selected + 1 + right_count;
    let visible: Vec<String> = file_names[offset..end].to_vec();
    (offset, visible)
}

fn style_for_line(line: &DiffLine, palette: &crate::theme::Palette) -> Style {
    match line {
        DiffLine::Added { .. } => Style::new().fg(palette.added),
        DiffLine::Removed { .. } => Style::new().fg(palette.removed),
        DiffLine::Context { .. } => Style::new(),
    }
}

/// How diff lines are colored. With `advanced` (Advanced diff color) the
/// content keeps its syntax colors over a tinted row; without it, lines are
/// plain text in the added/removed fg — also the plain-palette fallback.
#[derive(Clone, Copy)]
struct DiffColors<'a> {
    palette: &'a crate::theme::Palette,
    advanced: Option<&'a DiffHighlight>,
}

/// Styles for one diff line's display rows.
struct RowStyles {
    marker: Style,
    gutter: Style,
    /// Fills the row behind the content (Advanced tint).
    row: Style,
}

impl DiffColors<'_> {
    fn styles(&self, dl: &DiffLine) -> RowStyles {
        let p = self.palette;
        let dim = Style::new().fg(p.dim);
        let change = match dl {
            DiffLine::Added { .. } => Some(true),
            DiffLine::Removed { .. } => Some(false),
            DiffLine::Context { .. } => None,
        };
        let tint = change.and_then(|added| p.diff_bg(added, false));
        match (self.advanced, change, tint) {
            (Some(_), Some(added), Some(bg)) => {
                let fg = if added { p.added } else { p.removed };
                let s = Style::new().fg(fg).bg(bg);
                RowStyles {
                    marker: s,
                    gutter: s,
                    row: Style::new().bg(bg),
                }
            }
            _ => RowStyles {
                marker: style_for_line(dl, p),
                gutter: dim,
                row: Style::new(),
            },
        }
    }

    /// Content spans for `dl` as shown on `side` (Context lines exist on
    /// both). `partner` is the paired removed/added line for word emphasis.
    fn content(&self, dl: &DiffLine, side: Side, partner: Option<&DiffLine>) -> Vec<Span<'static>> {
        let Some(hl) = self.advanced else {
            let style = style_for_line(dl, self.palette);
            return vec![Span::styled(expand_tabs(diff_content(dl)), style)];
        };
        let text = display_text(dl);
        let (lines, line_no) = match (dl, side) {
            (DiffLine::Removed { line_no, .. }, _) => (&hl.old, *line_no),
            (DiffLine::Added { line_no, .. }, _) => (&hl.new, *line_no),
            (DiffLine::Context { old_line_no, .. }, Side::Old) => (&hl.old, *old_line_no),
            (DiffLine::Context { new_line_no, .. }, Side::New) => (&hl.new, *new_line_no),
        };
        let mut spans = highlighted_spans(lines.as_deref(), line_no, &text)
            .unwrap_or_else(|| vec![Span::raw(text.clone())]);

        let row = self.styles(dl).row;
        for span in &mut spans {
            span.style = span.style.patch(row);
        }
        let added = matches!(dl, DiffLine::Added { .. });
        let strong = self.palette.diff_bg(added, true);
        if let (Some(partner), Some(bg)) = (partner, strong) {
            let other = display_text(partner);
            let (old, new) = if added {
                (&other, &text)
            } else {
                (&text, &other)
            };
            if let Some((old_ranges, new_ranges)) = diff_words::changed_ranges(old, new) {
                let ranges = if added { new_ranges } else { old_ranges };
                spans = diff_words::emphasize(spans, &ranges, Style::new().bg(bg));
            }
        }
        spans
    }
}

fn diff_content(dl: &DiffLine) -> &str {
    match dl {
        DiffLine::Context { content, .. }
        | DiffLine::Added { content, .. }
        | DiffLine::Removed { content, .. } => content,
    }
}

/// The line text as rendered: no line ending, tabs expanded.
fn display_text(dl: &DiffLine) -> String {
    expand_tabs(diff_content(dl).trim_end_matches(['\n', '\r']))
}

/// The highlighted spans of 1-based `line_no`, only if their text matches
/// `text` — a mismatch (lossy UTF-8, odd line endings) falls back to plain.
fn highlighted_spans(
    lines: Option<&[Line<'static>]>,
    line_no: u32,
    text: &str,
) -> Option<Vec<Span<'static>>> {
    let line = lines?.get((line_no as usize).checked_sub(1)?)?;
    let mut spans = line.spans.clone();
    // CRLF files keep the '\r' in the highlighted line's last span.
    if let Some(last) = spans.last_mut() {
        if last.content.ends_with('\r') {
            last.content = last.content.trim_end_matches('\r').to_string().into();
        }
    }
    let joined: String = spans.iter().map(|s| s.content.as_ref()).collect();
    (joined == text).then_some(spans)
}

/// Width of the unified gutter: change marker + ` NNNN,NNNN `.
const UNIFIED_GUTTER_WIDTH: usize = 1 + 11;
/// Width of a side-by-side gutter: change marker + ` NNNN `.
const SIDE_GUTTER_WIDTH: usize = 1 + 6;

/// Wrap width for content inside a bordered pane whose gutter takes
/// `gutter` columns; 0 turns wrapping off.
fn wrap_width(pane: Rect, gutter: usize, word_wrap: bool) -> usize {
    if word_wrap {
        (pane.width as usize)
            .saturating_sub(2)
            .saturating_sub(gutter)
    } else {
        0
    }
}

/// Render one diff line as display rows: the first carries the marker and
/// line number, continuation rows repeat the marker over a blank gutter so
/// added/removed runs stay scannable.
fn diff_line_rows(
    marker: &'static str,
    line_no: String,
    content: Vec<Span<'static>>,
    styles: RowStyles,
    width: usize,
) -> Vec<Line<'static>> {
    let blank = " ".repeat(line_no.len());
    wrap::wrap_spans(&content, width)
        .into_iter()
        .enumerate()
        .map(|(r, row)| {
            let gutter = if r == 0 {
                line_no.clone()
            } else {
                blank.clone()
            };
            let mut spans = vec![
                Span::styled(marker, styles.marker),
                Span::styled(gutter, styles.gutter),
            ];
            spans.extend(row);
            Line::from(spans).style(styles.row)
        })
        .collect()
}

fn unified_line_rows(
    dl: &DiffLine,
    partner: Option<&DiffLine>,
    colors: DiffColors,
    width: usize,
) -> Vec<Line<'static>> {
    let (marker, line_no) = match dl {
        DiffLine::Context {
            old_line_no,
            new_line_no,
            ..
        } => {
            let no = if old_line_no == new_line_no {
                format!(" {:>4}     ", old_line_no)
            } else {
                format!(" {:>4},{:<4} ", old_line_no, new_line_no)
            };
            (" ", no)
        }
        DiffLine::Removed { line_no, .. } => ("-", format!(" {:>4},_    ", line_no)),
        DiffLine::Added { line_no, .. } => ("+", format!(" _,{:<4}    ", line_no)),
    };
    let content = colors.content(dl, Side::New, partner);
    diff_line_rows(marker, line_no, content, colors.styles(dl), width)
}

/// Pad tinted rows with spaces to `width` columns: `Paragraph` doesn't
/// paint a `Line`'s style past its text.
fn fill_rows(rows: &mut [Line<'static>], width: usize) {
    for row in rows {
        let pad = width.saturating_sub(row.width());
        if row.style.bg.is_some() && pad > 0 {
            let style = row.style;
            row.spans.push(Span::styled(" ".repeat(pad), style));
        }
    }
}

/// For each `file.lines` index, the removed/added line it is paired with
/// in the side-by-side alignment (word emphasis in the unified view).
fn change_partners(lines: &[DiffLine]) -> Vec<Option<usize>> {
    let mut partners = vec![None; lines.len()];
    for (o, n) in align_indices(lines) {
        if let (Some(o), Some(n)) = (o, n) {
            if o != n {
                partners[o] = Some(n);
                partners[n] = Some(o);
            }
        }
    }
    partners
}

/// Returns the logical line (`file.lines` index) shown on each row.
fn render_unified(
    frame: &mut ratatui::Frame,
    area: Rect,
    file: &DiffFile,
    scroll: usize,
    word_wrap: bool,
    colors: DiffColors,
    selection: Option<&Selection>,
) -> Vec<Option<usize>> {
    let palette = colors.palette;
    let height = area.height.saturating_sub(2) as usize;
    let width = wrap_width(area, UNIFIED_GUTTER_WIDTH, word_wrap);
    let partners = if colors.advanced.is_some() {
        change_partners(&file.lines)
    } else {
        Vec::new()
    };

    // Only the visible window is materialized; `scroll` counts logical
    // diff lines, so a wrapped line scrolls as one unit.
    let mut lines: Vec<Line> = Vec::with_capacity(height);
    let mut row_map = Vec::with_capacity(height);
    for (i, dl) in file.lines.iter().enumerate().skip(scroll) {
        if lines.len() >= height {
            break;
        }
        let partner = partners.get(i).copied().flatten().map(|p| &file.lines[p]);
        let mut rows = unified_line_rows(dl, partner, colors, width);
        fill_rows(&mut rows, area.width.saturating_sub(2) as usize);
        if selection.is_some_and(|s| s.contains(i, None)) {
            mark_selected(&mut rows);
        }
        row_map.extend(std::iter::repeat_n(Some(i), rows.len()));
        lines.extend(rows);
    }
    row_map.truncate(height);

    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().border_style(Style::new().fg(palette.border)));

    frame.render_widget(paragraph, area);
    row_map
}

#[derive(Debug)]
enum AlignedLine<'a> {
    Both {
        old: &'a DiffLine,
        new: &'a DiffLine,
    },
    OldOnly {
        old: &'a DiffLine,
    },
    NewOnly {
        new: &'a DiffLine,
    },
}

fn align_diff_lines(lines: &[DiffLine]) -> Vec<AlignedLine<'_>> {
    align_indices(lines)
        .into_iter()
        .map(|pair| match pair {
            (Some(o), Some(n)) => AlignedLine::Both {
                old: &lines[o],
                new: &lines[n],
            },
            (Some(o), None) => AlignedLine::OldOnly { old: &lines[o] },
            (None, Some(n)) => AlignedLine::NewOnly { new: &lines[n] },
            (None, None) => unreachable!("aligned pair with no side"),
        })
        .collect()
}

/// Side-by-side alignment as `file.lines` indices: (old, new) per row
/// pair. Context lines pair with themselves; each removed/added run is
/// zipped, leftovers sit alone on their side.
pub fn align_indices(lines: &[DiffLine]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if matches!(lines[i], DiffLine::Context { .. }) {
            result.push((Some(i), Some(i)));
            i += 1;
            continue;
        }
        let mut removed = Vec::new();
        let mut added = Vec::new();
        while i < lines.len() && !matches!(lines[i], DiffLine::Context { .. }) {
            match &lines[i] {
                DiffLine::Removed { .. } => removed.push(i),
                DiffLine::Added { .. } => added.push(i),
                DiffLine::Context { .. } => {}
            }
            i += 1;
        }
        for j in 0..removed.len().max(added.len()) {
            result.push((removed.get(j).copied(), added.get(j).copied()));
        }
    }
    result
}

/// The `file.lines` indices shown by aligned pairs `lo..=hi` on `side`
/// (both sides when None), sorted. Empty when that side has no lines there.
pub fn aligned_to_file_indices(
    lines: &[DiffLine],
    lo: usize,
    hi: usize,
    side: Option<Side>,
) -> Vec<usize> {
    let aligned = align_indices(lines);
    let hi = hi.min(aligned.len().saturating_sub(1));
    let mut picked: Vec<usize> = aligned
        .get(lo..=hi)
        .unwrap_or(&[])
        .iter()
        .flat_map(|&(o, n)| match side {
            Some(Side::Old) => [o, None],
            Some(Side::New) => [None, n],
            None => [o, n],
        })
        .flatten()
        .collect();
    picked.sort_unstable();
    picked.dedup();
    picked
}

/// Returns the aligned-pair index shown on each row and the two panes'
/// inner rects (old, new).
fn render_side_by_side(
    frame: &mut ratatui::Frame,
    area: Rect,
    file: &DiffFile,
    scroll: usize,
    word_wrap: bool,
    colors: DiffColors,
    selection: Option<&Selection>,
) -> (Vec<Option<usize>>, (Rect, Rect)) {
    let palette = colors.palette;
    let (left, right) = layout::split_horizontal(area, area.width / 2);
    let height = area.height.saturating_sub(2) as usize;
    let old_width = wrap_width(left, SIDE_GUTTER_WIDTH, word_wrap);
    let new_width = wrap_width(right, SIDE_GUTTER_WIDTH, word_wrap);

    let aligned = align_diff_lines(&file.lines);

    let mut old_lines: Vec<Line> = Vec::with_capacity(height);
    let mut new_lines: Vec<Line> = Vec::with_capacity(height);
    let mut row_map = Vec::with_capacity(height);

    // Each aligned pair must take the same number of rows on both sides
    // so the panes stay in step when one side wraps further than the other.
    // That also lets one row map serve both panes.
    for (i, al) in aligned.iter().enumerate().skip(scroll) {
        if old_lines.len() >= height {
            break;
        }
        let (mut old_rows, mut new_rows) = match al {
            AlignedLine::Both { old, new } => {
                // A removed/added pair (not a context line paired with itself).
                let paired = !matches!(old, DiffLine::Context { .. });
                (
                    diff_line_rows_side(old, paired.then_some(*new), colors, false, old_width),
                    diff_line_rows_side(new, paired.then_some(*old), colors, true, new_width),
                )
            }
            AlignedLine::OldOnly { old } => (
                diff_line_rows_side(old, None, colors, false, old_width),
                Vec::new(),
            ),
            AlignedLine::NewOnly { new } => (
                Vec::new(),
                diff_line_rows_side(new, None, colors, true, new_width),
            ),
        };
        fill_rows(&mut old_rows, left.width.saturating_sub(2) as usize);
        fill_rows(&mut new_rows, right.width.saturating_sub(2) as usize);
        let rows = old_rows.len().max(new_rows.len());
        old_rows.resize_with(rows, || Line::from(""));
        new_rows.resize_with(rows, || Line::from(""));
        if let Some(s) = selection {
            if s.contains(i, Some(Side::Old)) {
                mark_selected(&mut old_rows);
            }
            if s.contains(i, Some(Side::New)) {
                mark_selected(&mut new_rows);
            }
        }
        row_map.extend(std::iter::repeat_n(Some(i), rows));
        old_lines.extend(old_rows);
        new_lines.extend(new_rows);
    }

    let old_widget = Paragraph::new(old_lines).block(
        Block::bordered()
            .title(" old ")
            .border_style(Style::new().fg(palette.border)),
    );
    let new_widget = Paragraph::new(new_lines).block(
        Block::bordered()
            .title(" new ")
            .border_style(Style::new().fg(palette.border)),
    );

    frame.render_widget(old_widget, left);
    frame.render_widget(new_widget, right);
    row_map.truncate(height);
    let margin = ratatui::layout::Margin::new(1, 1);
    (row_map, (left.inner(margin), right.inner(margin)))
}

/// Rows for one side of the side-by-side view; `width` 0 means no wrap.
fn diff_line_rows_side(
    dl: &DiffLine,
    partner: Option<&DiffLine>,
    colors: DiffColors,
    is_new: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let (marker, line_no) = match dl {
        DiffLine::Context {
            old_line_no,
            new_line_no,
            ..
        } => {
            let no = if is_new { new_line_no } else { old_line_no };
            (" ", format!(" {:>4} ", no))
        }
        DiffLine::Removed { line_no, .. } => ("-", format!(" {:>4} ", line_no)),
        DiffLine::Added { line_no, .. } => ("+", format!(" {:>4} ", line_no)),
    };
    let side = if is_new { Side::New } else { Side::Old };
    let content = colors.content(dl, side, partner);
    diff_line_rows(marker, line_no, content, colors.styles(dl), width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_diff_lines_context_only() {
        let lines = vec![DiffLine::Context {
            old_line_no: 1,
            new_line_no: 1,
            content: "hello".into(),
        }];
        let aligned = align_diff_lines(&lines);
        assert_eq!(aligned.len(), 1);
        match &aligned[0] {
            AlignedLine::Both { old, new } => {
                assert_eq!(old, new);
            }
            _ => panic!("expected Both"),
        }
    }

    #[test]
    fn test_align_diff_lines_added_removed_pairs() {
        let lines = vec![
            DiffLine::Removed {
                line_no: 1,
                content: "old".into(),
            },
            DiffLine::Added {
                line_no: 1,
                content: "new".into(),
            },
        ];
        let aligned = align_diff_lines(&lines);
        assert_eq!(aligned.len(), 1);
        match &aligned[0] {
            AlignedLine::Both { old, new } => {
                match old {
                    DiffLine::Removed { content, .. } => assert_eq!(content, "old"),
                    _ => panic!("expected Removed"),
                }
                match new {
                    DiffLine::Added { content, .. } => assert_eq!(content, "new"),
                    _ => panic!("expected Added"),
                }
            }
            _ => panic!("expected Both"),
        }
    }

    #[test]
    fn test_align_diff_lines_more_added_than_removed() {
        let lines = vec![
            DiffLine::Removed {
                line_no: 1,
                content: "old".into(),
            },
            DiffLine::Added {
                line_no: 1,
                content: "new1".into(),
            },
            DiffLine::Added {
                line_no: 2,
                content: "new2".into(),
            },
        ];
        let aligned = align_diff_lines(&lines);
        assert_eq!(aligned.len(), 2);
        match &aligned[0] {
            AlignedLine::Both { .. } => {}
            _ => panic!("expected Both at position 0"),
        }
        match &aligned[1] {
            AlignedLine::NewOnly { new } => match new {
                DiffLine::Added { content, .. } => assert_eq!(content, "new2"),
                _ => panic!("expected Added"),
            },
            _ => panic!("expected NewOnly at position 1"),
        }
    }

    #[test]
    fn test_align_diff_lines_more_removed_than_added() {
        let lines = vec![
            DiffLine::Removed {
                line_no: 1,
                content: "old1".into(),
            },
            DiffLine::Removed {
                line_no: 2,
                content: "old2".into(),
            },
            DiffLine::Added {
                line_no: 1,
                content: "new".into(),
            },
        ];
        let aligned = align_diff_lines(&lines);
        assert_eq!(aligned.len(), 2);
        match &aligned[0] {
            AlignedLine::Both { .. } => {}
            _ => panic!("expected Both at position 0"),
        }
        match &aligned[1] {
            AlignedLine::OldOnly { old } => match old {
                DiffLine::Removed { content, .. } => assert_eq!(content, "old2"),
                _ => panic!("expected Removed"),
            },
            _ => panic!("expected OldOnly at position 1"),
        }
    }

    #[test]
    fn test_align_diff_lines_context_surrounding_changes() {
        let lines = vec![
            DiffLine::Context {
                old_line_no: 1,
                new_line_no: 1,
                content: "before".into(),
            },
            DiffLine::Removed {
                line_no: 2,
                content: "old".into(),
            },
            DiffLine::Added {
                line_no: 2,
                content: "new".into(),
            },
            DiffLine::Context {
                old_line_no: 3,
                new_line_no: 3,
                content: "after".into(),
            },
        ];
        let aligned = align_diff_lines(&lines);
        assert_eq!(aligned.len(), 3);
        match &aligned[0] {
            AlignedLine::Both { .. } => {}
            _ => panic!("expected Both for context line"),
        }
        match &aligned[1] {
            AlignedLine::Both { .. } => {}
            _ => panic!("expected Both for paired change"),
        }
        match &aligned[2] {
            AlignedLine::Both { .. } => {}
            _ => panic!("expected Both for context line"),
        }
    }

    #[test]
    fn test_aligned_to_file_indices_per_side() {
        let lines = vec![
            DiffLine::Context {
                old_line_no: 1,
                new_line_no: 1,
                content: "a".into(),
            },
            DiffLine::Removed {
                line_no: 2,
                content: "b".into(),
            },
            DiffLine::Added {
                line_no: 2,
                content: "c".into(),
            },
            DiffLine::Added {
                line_no: 3,
                content: "d".into(),
            },
        ];
        assert_eq!(
            align_indices(&lines),
            vec![(Some(0), Some(0)), (Some(1), Some(2)), (None, Some(3))]
        );
        assert_eq!(
            aligned_to_file_indices(&lines, 1, 2, Some(Side::New)),
            vec![2, 3]
        );
        assert_eq!(
            aligned_to_file_indices(&lines, 1, 2, Some(Side::Old)),
            vec![1]
        );
        assert!(aligned_to_file_indices(&lines, 2, 2, Some(Side::Old)).is_empty());
        assert_eq!(
            aligned_to_file_indices(&lines, 0, 2, None),
            vec![0, 1, 2, 3]
        );
        // Only the head of a removed/added block on screen: the rest of the
        // removed run (index 3 here) must not be pulled in.
        let block = vec![
            DiffLine::Removed {
                line_no: 1,
                content: "r1".into(),
            },
            DiffLine::Removed {
                line_no: 2,
                content: "r2".into(),
            },
            DiffLine::Added {
                line_no: 1,
                content: "a1".into(),
            },
            DiffLine::Added {
                line_no: 2,
                content: "a2".into(),
            },
        ];
        assert_eq!(aligned_to_file_indices(&block, 0, 0, None), vec![0, 2]);
    }

    #[test]
    fn test_diff_line_span_context_uses_correct_line_no() {
        let palette = crate::theme::Palette::plain();
        let dl = DiffLine::Context {
            old_line_no: 10,
            new_line_no: 20,
            content: "ctx".into(),
        };
        let old_span = diff_line_rows_side(&dl, None, legacy(&palette), false, 0).remove(0);
        let new_span = diff_line_rows_side(&dl, None, legacy(&palette), true, 0).remove(0);
        let old_text: String = old_span.spans.iter().map(|s| s.content.as_ref()).collect();
        let new_text: String = new_span.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            old_text.contains("10"),
            "old side should use old_line_no: {}",
            old_text
        );
        assert!(
            new_text.contains("20"),
            "new side should use new_line_no: {}",
            new_text
        );
    }

    fn legacy(palette: &crate::theme::Palette) -> DiffColors<'_> {
        DiffColors {
            palette,
            advanced: None,
        }
    }

    fn hl_line(parts: &[(&str, Style)]) -> Line<'static> {
        Line::from(
            parts
                .iter()
                .map(|(t, s)| Span::styled(t.to_string(), *s))
                .collect::<Vec<_>>(),
        )
    }

    fn row_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn test_side_rows_unwrapped_is_single_row() {
        let palette = crate::theme::Palette::plain();
        let dl = DiffLine::Added {
            line_no: 7,
            content: "a very long added line that would wrap".into(),
        };
        let rows = diff_line_rows_side(&dl, None, legacy(&palette), true, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            row_text(&rows[0]),
            "+    7 a very long added line that would wrap"
        );
    }

    #[test]
    fn test_side_rows_wrap_with_marker_and_blank_gutter() {
        let palette = crate::theme::Palette::plain();
        let dl = DiffLine::Removed {
            line_no: 12,
            content: "alpha beta gamma delta".into(),
        };
        let rows = diff_line_rows_side(&dl, None, legacy(&palette), false, 11);
        let texts: Vec<String> = rows.iter().map(row_text).collect();
        assert_eq!(texts, vec!["-   12 alpha beta ", "-      gamma delta"]);
        // The removed style survives on the continuation row's content.
        let style = style_for_line(&dl, &palette);
        assert_eq!(rows[1].spans.last().unwrap().style, style);
    }

    #[test]
    fn test_unified_rows_keep_gutter_width_on_continuation() {
        let palette = crate::theme::Palette::plain();
        let dl = DiffLine::Context {
            old_line_no: 3,
            new_line_no: 5,
            content: "one two three".into(),
        };
        let rows = unified_line_rows(&dl, None, legacy(&palette), 8);
        let texts: Vec<String> = rows.iter().map(row_text).collect();
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0], "     3,5    one two ");
        assert_eq!(
            texts[1],
            format!("{}three", " ".repeat(UNIFIED_GUTTER_WIDTH))
        );
        assert_eq!(texts[0].len(), UNIFIED_GUTTER_WIDTH + "one two ".len());
    }

    #[test]
    fn test_wrap_width_zero_when_disabled_or_too_narrow() {
        let pane = Rect::new(0, 0, 40, 10);
        assert_eq!(wrap_width(pane, SIDE_GUTTER_WIDTH, false), 0);
        assert_eq!(wrap_width(pane, SIDE_GUTTER_WIDTH, true), 40 - 2 - 7);
        assert_eq!(
            wrap_width(Rect::new(0, 0, 5, 10), SIDE_GUTTER_WIDTH, true),
            0
        );
    }

    #[test]
    fn test_visible_tabs_empty() {
        let (offset, visible) = visible_tabs(&[], 0, 80);
        assert_eq!(offset, 0);
        assert!(visible.is_empty());
    }

    #[test]
    fn test_visible_tabs_all_fit() {
        let names = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let (offset, visible) = visible_tabs(&names, 1, 80);
        assert_eq!(offset, 0);
        assert_eq!(visible, names);
    }

    #[test]
    fn test_visible_tabs_scroll_to_selected() {
        let names: Vec<String> = (0..20).map(|i| format!("file_{:02}.rs", i)).collect();
        let (offset, visible) = visible_tabs(&names, 15, 80);
        assert!(
            offset > 0,
            "should scroll when selected is beyond visible range"
        );
        let adjusted = 15 - offset;
        assert_eq!(visible[adjusted], "file_15.rs");
    }

    #[test]
    fn test_visible_tabs_selected_at_zero() {
        let names: Vec<String> = (0..20).map(|i| format!("file_{:02}.rs", i)).collect();
        let (offset, _visible) = visible_tabs(&names, 0, 80);
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_visible_tabs_narrow_width() {
        let names = vec!["aaaaa.rs".to_string(), "bbbbb.rs".to_string()];
        let (offset, visible) = visible_tabs(&names, 1, 10);
        assert_eq!(offset, 1);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0], "bbbbb.rs");
    }

    #[test]
    fn test_advanced_rows_keep_syntax_fg_over_tint() {
        let palette = crate::theme::Palette::catppuccin();
        let kw = Style::new().fg(palette.syn_keyword);
        let hl = DiffHighlight {
            key: (git2::Oid::ZERO_SHA1, git2::Oid::ZERO_SHA1, 0),
            old: None,
            new: Some(vec![
                hl_line(&[("fn", kw), (" a() {}", Style::new())]),
                hl_line(&[("let", kw), (" x = 1;", Style::new())]),
            ]),
        };
        let colors = DiffColors {
            palette: &palette,
            advanced: Some(&hl),
        };
        let dl = DiffLine::Added {
            line_no: 2,
            content: "let x = 1;\n".into(),
        };
        let rows = diff_line_rows_side(&dl, None, colors, true, 0);
        let tint = palette.diff_bg(true, false).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(row_text(&rows[0]), "+    2 let x = 1;");
        assert_eq!(rows[0].style.bg, Some(tint), "row fills with the tint");
        let content = &rows[0].spans[2];
        assert_eq!(content.content, "let");
        assert_eq!(content.style.fg, Some(palette.syn_keyword));
        assert_eq!(content.style.bg, Some(tint));
        assert_eq!(rows[0].spans[0].style.fg, Some(palette.added));
    }

    #[test]
    fn test_advanced_context_line_reads_its_own_side() {
        let palette = crate::theme::Palette::nord();
        let hl = DiffHighlight {
            key: (git2::Oid::ZERO_SHA1, git2::Oid::ZERO_SHA1, 0),
            old: Some(vec![hl_line(&[("ctx", Style::new().fg(palette.syn_type))])]),
            new: Some(vec![
                Line::from("other"),
                hl_line(&[("ctx", Style::new().fg(palette.syn_string))]),
            ]),
        };
        let colors = DiffColors {
            palette: &palette,
            advanced: Some(&hl),
        };
        let dl = DiffLine::Context {
            old_line_no: 1,
            new_line_no: 2,
            content: "ctx\n".into(),
        };
        let old = diff_line_rows_side(&dl, None, colors, false, 0);
        let new = diff_line_rows_side(&dl, None, colors, true, 0);
        assert_eq!(old[0].spans[2].style.fg, Some(palette.syn_type));
        assert_eq!(new[0].spans[2].style.fg, Some(palette.syn_string));
        assert_eq!(old[0].style.bg, None, "context rows are not tinted");
    }

    #[test]
    fn test_advanced_falls_back_to_plain_on_mismatch() {
        let palette = crate::theme::Palette::nord();
        let hl = DiffHighlight {
            key: (git2::Oid::ZERO_SHA1, git2::Oid::ZERO_SHA1, 0),
            old: Some(vec![Line::from("something else")]),
            new: None,
        };
        let colors = DiffColors {
            palette: &palette,
            advanced: Some(&hl),
        };
        let dl = DiffLine::Removed {
            line_no: 1,
            content: "\tgone\r\n".into(),
        };
        let rows = unified_line_rows(&dl, None, colors, 0);
        let tint = palette.diff_bg(false, false).unwrap();
        assert_eq!(rows[0].spans[2].content, "    gone");
        assert_eq!(rows[0].spans[2].style, Style::new().bg(tint));
        // Out-of-range line numbers fall back the same way.
        let far = DiffLine::Removed {
            line_no: 9,
            content: "x\n".into(),
        };
        assert_eq!(
            unified_line_rows(&far, None, colors, 0)[0].spans[2].content,
            "x"
        );
    }

    #[test]
    fn test_advanced_pair_emphasizes_changed_words() {
        let palette = crate::theme::Palette::tokyo_night();
        let hl = DiffHighlight {
            key: (git2::Oid::ZERO_SHA1, git2::Oid::ZERO_SHA1, 0),
            old: None,
            new: None,
        };
        let colors = DiffColors {
            palette: &palette,
            advanced: Some(&hl),
        };
        let old = DiffLine::Removed {
            line_no: 1,
            content: "if a() {\n".into(),
        };
        let new = DiffLine::Added {
            line_no: 1,
            content: "if a() || b() {\n".into(),
        };
        let rows = diff_line_rows_side(&new, Some(&old), colors, true, 0);
        let strong = palette.diff_bg(true, true).unwrap();
        let emphasized: String = rows[0]
            .spans
            .iter()
            .filter(|s| s.style.bg == Some(strong))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(emphasized, "|| b()");
    }

    #[test]
    fn test_legacy_colors_when_advanced_off() {
        let palette = crate::theme::Palette::catppuccin();
        let dl = DiffLine::Added {
            line_no: 3,
            content: "let x = 1;\n".into(),
        };
        let rows = diff_line_rows_side(&dl, None, legacy(&palette), true, 0);
        assert_eq!(rows[0].style, Style::new());
        assert_eq!(rows[0].spans[2].style, Style::new().fg(palette.added));
    }

    #[test]
    fn test_change_partners_pairs_runs_only() {
        let lines = vec![
            DiffLine::Context {
                old_line_no: 1,
                new_line_no: 1,
                content: "c".into(),
            },
            DiffLine::Removed {
                line_no: 2,
                content: "r1".into(),
            },
            DiffLine::Removed {
                line_no: 3,
                content: "r2".into(),
            },
            DiffLine::Added {
                line_no: 2,
                content: "a1".into(),
            },
        ];
        assert_eq!(change_partners(&lines), vec![None, Some(3), None, Some(1)]);
    }

    #[test]
    fn test_fill_rows_pads_tinted_rows_only() {
        use ratatui::style::Color;
        let tint = Style::new().bg(Color::Rgb(1, 2, 3));
        let mut rows = vec![Line::from("ab").style(tint), Line::from("cd")];
        fill_rows(&mut rows, 6);
        assert_eq!(row_text(&rows[0]), "ab    ");
        assert_eq!(rows[0].spans.last().unwrap().style, tint);
        assert_eq!(row_text(&rows[1]), "cd");
        // Wider than the pane (wrap off): untouched.
        let mut long = vec![Line::from("abcdefgh").style(tint)];
        fill_rows(&mut long, 4);
        assert_eq!(row_text(&long[0]), "abcdefgh");
    }
}
