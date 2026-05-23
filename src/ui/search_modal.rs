use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::search::modal::ModalState;
use crate::search::DocKind;

pub fn render_search_modal(frame: &mut Frame, app: &App) {
    let modal = &app.search_modal;
    if !modal.is_open() {
        return;
    }

    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);

    match &modal.state {
        ModalState::Idle => {}
        ModalState::NoIndex => render_no_index(frame, area, app),
        ModalState::Typing { input } | ModalState::Loading { input } => {
            render_input(frame, area, input.as_str(), app)
        }
        ModalState::Results { input, results } => {
            render_results(frame, area, input.as_str(), results, modal.selected, app)
        }
    }
}

fn render_no_index(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" No index found ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.palette.accent));
    let msg = Paragraph::new(vec![
        Line::from(""),
        Line::from("  Run `glc index` to build the search index."),
        Line::from(""),
        Line::from(Span::styled(
            "  Press Esc to close",
            Style::default().fg(app.palette.dim),
        )),
    ])
    .block(block);
    frame.render_widget(msg, area);
}

fn render_input(frame: &mut Frame, area: Rect, input: &str, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input_block = Block::default()
        .title(" Semantic Search (S) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.palette.accent));
    let input_widget = Paragraph::new(format!("> {}_", input)).block(input_block);
    frame.render_widget(input_widget, chunks[0]);

    let hint_block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(app.palette.accent));
    let hint = Paragraph::new(Line::from(Span::styled(
        "  Type to search commits and files",
        Style::default().fg(app.palette.dim),
    )))
    .block(hint_block);
    frame.render_widget(hint, chunks[1]);
}

fn render_results(
    frame: &mut Frame,
    area: Rect,
    input: &str,
    results: &[crate::search::SearchResult],
    selected: usize,
    app: &App,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let input_block = Block::default()
        .title(" Semantic Search ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.palette.accent));
    let input_widget = Paragraph::new(format!("> {}", input)).block(input_block);
    frame.render_widget(input_widget, chunks[0]);

    let items: Vec<ListItem> = results
        .iter()
        .map(|r| {
            let kind_tag = match r.meta.kind {
                DocKind::Commit => Span::styled("[commit]", Style::default().fg(Color::Yellow)),
                DocKind::File => Span::styled("[file]  ", Style::default().fg(Color::Cyan)),
                DocKind::Symbol => Span::styled("[sym]   ", Style::default().fg(Color::Green)),
            };
            let title = Span::raw(format!("  {}", r.meta.title));
            ListItem::new(Line::from(vec![kind_tag, title]))
        })
        .collect();

    let results_block = Block::default()
        .title(format!(" {} results ", results.len()))
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(app.palette.accent));

    let list = List::new(items)
        .block(results_block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, chunks[1], &mut state);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
