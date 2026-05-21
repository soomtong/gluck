use crate::app::App;
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState};

fn format_date(time: std::time::SystemTime) -> String {
    let duration = time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
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
        Span::styled(
            format!("{:<12} ", date_str),
            Style::new().dark_gray(),
        ),
        Span::raw(commit.message.clone()),
    ])
}

pub fn render_pick(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);

    if app.searching {
        layout::render_search_bar(frame, header, &app.search_input);
    } else {
        layout::render_header(frame, header, "gluck - Pick Mode");
    }

    if let Mode::Pick(state) = &app.mode {
        let visible = state.visible_commits();
        let items: Vec<ListItem> = visible.iter().map(|c| ListItem::new(format_commit_line(c))).collect();

        let list = List::new(items)
            .block(
                Block::bordered()
                    .title(format!(" {} commits ", visible.len()))
                    .style(Style::new().white()),
            )
            .highlight_style(Style::new().black().on_white())
            .highlight_symbol("● ");

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected));

        frame.render_stateful_widget(list, body, &mut list_state);
    }

    let hints = [("[j/k]", "move"), ("[Enter]", "view"), ("[/]", "search"), ("[q]", "quit")];
    layout::render_footer(frame, footer, &hints);
}
