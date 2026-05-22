use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

pub fn app_layout(area: Rect) -> (Rect, Rect, Rect) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    (header, body, footer)
}

pub fn split_horizontal(area: Rect, left_width: u16) -> (Rect, Rect) {
    let [left, right] =
        Layout::horizontal([Constraint::Length(left_width), Constraint::Min(1)]).areas(area);
    (left, right)
}

pub fn render_header(frame: &mut ratatui::Frame, area: Rect, mode: &str) {
    let logo = Span::styled("◆ ", Style::new().magenta());
    let name = Span::styled("glc", Style::new().white().bold());
    let version = Span::styled(" v0.2.2", Style::new().dark_gray());
    let sep = Span::styled(" · ", Style::new().dark_gray());
    let mode_span = Span::styled(mode, Style::new().cyan().bold());
    let project = Span::styled(" GLUCK", Style::new().white().not_bold());
    let tagline = Span::styled(
        " git log unfolds code into knowledge",
        Style::new().dark_gray().italic(),
    );

    let line = Line::from(vec![logo, name, version, sep, mode_span, project, tagline]);

    let header =
        Paragraph::new(line).block(Block::bordered().border_style(Style::new().dark_gray()));
    frame.render_widget(header, area);
}

pub fn render_footer(frame: &mut ratatui::Frame, area: Rect, hints: &[(&str, &str)]) {
    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(
                    format!("[{}]", key),
                    Style::new().yellow().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {} ", desc)),
            ]
        })
        .collect();
    let footer = Paragraph::new(Line::from(spans));
    frame.render_widget(footer, area);
}

pub fn render_search_bar(frame: &mut ratatui::Frame, area: Rect, query: &str) {
    let search = Paragraph::new(format!("/ {}", query))
        .style(Style::new().yellow())
        .block(Block::bordered().style(Style::new().dark_gray()));
    frame.render_widget(search, area);
}
