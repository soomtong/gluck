use crate::app::App;
use crate::git::diff::{DiffFile, DiffLine};
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

fn format_date(time: std::time::SystemTime) -> String {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let (year, month, day) = days_to_date(duration.as_secs() / 86400);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn format_commit_line(commit: &crate::git::commit::CommitInfo) -> Line<'static> {
    let date_str = format_date(commit.date);
    Line::from(vec![
        Span::styled(
            format!(" {} ", commit.short_id),
            Style::new().yellow().add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{:<12} ", date_str), Style::new().dark_gray()),
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
                Style::new().white().add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!(" {} <{}>", format_date(commit.date), commit.author),
                Style::new().dark_gray(),
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
                .style(Style::new().white()),
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
                            Style::new().green(),
                        ));
                        spans.push(Span::raw(" "));
                    }
                    if removed > 0 {
                        spans.push(Span::styled(
                            format!("-{}", removed),
                            Style::new().red(),
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
                    .style(Style::new().white()),
            );

            frame.render_widget(files_list, files_area);
        } else {
            let no_diff = Paragraph::new(" (root commit) ")
                .block(
                    Block::bordered()
                        .title(" Files ")
                        .style(Style::new().white()),
                )
                .style(Style::new().dark_gray());
            frame.render_widget(no_diff, files_area);
        }
    }
}

pub fn render_pick(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);

    if let Mode::Pick(state) = &app.mode {
        if let crate::mode::SearchState::Active { input } = &state.search {
            layout::render_search_bar(frame, header, input);
        } else {
            layout::render_header(frame, header, "PICK", None);
        }
    } else {
        layout::render_header(frame, header, "PICK", None);
    }

    if let Mode::Pick(state) = &app.mode {
        let [commit_area, detail_area] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(60)]).areas(body);

        let visible = state.visible_commits();
        let items: Vec<ListItem> = visible
            .iter()
            .map(|c| ListItem::new(format_commit_line(c)))
            .collect();

        let list = List::new(items)
            .block(
                Block::bordered()
                    .title(format!(" {} commits ", visible.len()))
                    .style(Style::new().white()),
            )
            .highlight_style(Style::new().black().on_white());

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected));

        frame.render_stateful_widget(list, commit_area, &mut list_state);
        render_commit_detail(frame, detail_area, app);
    }

    let hints = [
        ("[j/k]", "move"),
        ("[Enter]", "view"),
        ("[/]", "search"),
        ("[q]", "quit"),
    ];
    layout::render_footer(frame, footer, &hints);
}
