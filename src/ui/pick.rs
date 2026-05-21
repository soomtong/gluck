use crate::app::App;
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

fn format_date(time: std::time::SystemTime) -> String {
    let duration = time
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    format!("{}", secs)
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

        let list = List::new(items).block(
            Block::bordered()
                .title(format!(" {} commits ", visible.len()))
                .style(Style::new().white()),
        );

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected));

        frame.render_stateful_widget(list, body, &mut list_state);
    }

    let hints = [("[j/k]", "move"), ("[Enter]", "view"), ("[/]", "search"), ("[q]", "quit")];
    layout::render_footer(frame, footer, &hints);
}