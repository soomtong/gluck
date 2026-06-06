use crate::app::App;
use crate::git::diff::{DiffFile, DiffLine};
use crate::mode::Mode;
use crate::theme::Palette;
use crate::ui::layout;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

fn format_date(time: std::time::SystemTime) -> String {
    layout::format_header_date(time)
}

fn format_commit_line(commit: &crate::git::commit::CommitInfo, palette: &Palette) -> Line<'static> {
    let date_str = format_date(commit.date);
    Line::from(vec![
        Span::styled(
            format!(" {} ", commit.short_id),
            Style::new()
                .fg(palette.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{:<12} ", date_str), Style::new().fg(palette.dim)),
        Span::raw(commit.message.lines().next().unwrap_or("").to_string()),
    ])
}

fn split_message(msg: &str) -> (&str, &str) {
    if let Some(pos) = msg.find("\n\n") {
        (&msg[..pos], &msg[pos + 2..])
    } else if let Some(pos) = msg.find('\n') {
        (&msg[..pos], &msg[pos + 1..])
    } else {
        (msg, "")
    }
}

fn file_stats(file: &DiffFile) -> (usize, usize) {
    let added = file
        .lines
        .iter()
        .filter(|l| matches!(l, DiffLine::Added { .. }))
        .count();
    let removed = file
        .lines
        .iter()
        .filter(|l| matches!(l, DiffLine::Removed { .. }))
        .count();
    (added, removed)
}

fn render_commit_detail(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let palette = &app.palette;
    if let Mode::Pick(state) = &app.mode {
        let Some(&idx) = state.filtered_indices.get(state.selected) else {
            return;
        };
        let commit = &state.commits[idx];
        let (subject, body) = split_message(&commit.message);

        let desc_height = {
            let h =
                2 + if body.is_empty() {
                    0
                } else {
                    body.lines().count() + 1
                } + 3;
            (h as u16).clamp(3, area.height / 2)
        };

        let [desc_area, files_area] =
            Layout::vertical([Constraint::Length(desc_height), Constraint::Min(1)]).areas(area);

        let mut desc_lines: Vec<Line> = vec![
            Line::from(Span::styled(
                format!(" {}", subject),
                Style::new().fg(palette.fg).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!(" {} <{}>", format_date(commit.date), commit.author),
                Style::new().fg(palette.dim),
            )),
        ];
        if !body.is_empty() {
            desc_lines.push(Line::from(""));
            for line in body.lines() {
                desc_lines.push(Line::from(Span::raw(format!(" {}", line))));
            }
        }

        let desc = Paragraph::new(desc_lines).block(
            Block::bordered()
                .title(" Description ")
                .border_style(Style::new().fg(palette.border)),
        );
        frame.render_widget(desc, desc_area);

        if let Some(diff) = &state.selected_diff {
            let file_items: Vec<ListItem> = diff
                .files
                .iter()
                .map(|f| {
                    let path = f.change.as_ref().map(|c| c.path()).unwrap_or("?");
                    let (added, removed) = file_stats(f);
                    let mut spans = vec![Span::raw(" ")];
                    if added > 0 {
                        spans.push(Span::styled(
                            format!("+{}", added),
                            Style::new().fg(palette.added),
                        ));
                        spans.push(Span::raw(" "));
                    }
                    if removed > 0 {
                        spans.push(Span::styled(
                            format!("-{}", removed),
                            Style::new().fg(palette.removed),
                        ));
                        spans.push(Span::raw(" "));
                    }
                    spans.push(Span::raw(path.to_string()));
                    ListItem::new(Line::from(spans))
                })
                .collect();

            let files_list = List::new(file_items).block(
                Block::bordered()
                    .title(format!(" Files ({}) ", diff.files.len()))
                    .border_style(Style::new().fg(palette.border)),
            );

            frame.render_widget(files_list, files_area);
        } else {
            let no_diff = Paragraph::new(" (root commit) ")
                .block(
                    Block::bordered()
                        .title(" Files ")
                        .border_style(Style::new().fg(palette.border)),
                )
                .style(Style::new().fg(palette.dim));
            frame.render_widget(no_diff, files_area);
        }
    }
}

pub fn render_pick(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);
    let palette = &app.palette;

    if let Mode::Pick(state) = &app.mode {
        if let crate::mode::SearchState::Active { input } = &state.search {
            layout::render_search_bar(frame, header, palette, input);
        } else {
            layout::render_header(frame, header, palette, "PICK", &app.theme_name, None);
        }
    } else {
        layout::render_header(frame, header, palette, "PICK", &app.theme_name, None);
    }

    if let Mode::Pick(state) = &app.mode {
        const MIN_DETAIL_WIDTH: u16 = 100;
        let (commit_area, detail_area) = if body.width >= MIN_DETAIL_WIDTH {
            let [left, right] =
                Layout::horizontal([Constraint::Ratio(3, 5), Constraint::Ratio(2, 5)]).areas(body);
            (left, Some(right))
        } else {
            (body, None)
        };

        let visible = state.visible_commits();
        let items: Vec<ListItem> = visible
            .iter()
            .map(|c| ListItem::new(format_commit_line(c, palette)))
            .collect();

        let list = List::new(items)
            .block(
                Block::bordered()
                    .title(format!(" {} commits ", visible.len()))
                    .border_style(Style::new().fg(palette.border)),
            )
            .highlight_style(palette.highlight_style())
            .scroll_padding(3);

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected));

        frame.render_stateful_widget(list, commit_area, &mut list_state);
        if let Some(area) = detail_area {
            render_commit_detail(frame, area, app);
        }
    }

    let hints = [
        ("[j/k]", "move"),
        ("[d/u]", "scroll"),
        ("[^f/b]", "page"),
        ("[Enter]", "view"),
        ("[Tab]", "diff"),
        ("[/]", "search"),
        ("[s]", "semantic"),
        ("[q]", "quit"),
    ];
    layout::render_footer(frame, footer, palette, &hints);
}
