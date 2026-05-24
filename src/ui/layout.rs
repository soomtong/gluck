use crate::theme::Palette;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use unicode_width::UnicodeWidthStr;

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

pub fn render_header(
    frame: &mut ratatui::Frame,
    area: Rect,
    palette: &Palette,
    mode: &str,
    theme: &str,
    message: Option<&str>,
) {
    let logo = Span::styled("◆ ", Style::new().fg(palette.accent));
    let name = Span::styled(
        "glc",
        Style::new().fg(palette.fg).add_modifier(Modifier::BOLD),
    );
    let version = Span::styled(
        format!(" v{}", env!("CARGO_PKG_VERSION")),
        Style::new().fg(palette.dim),
    );
    let sep = Span::styled(" · ", Style::new().fg(palette.dim));
    let mode_span = Span::styled(
        mode,
        Style::new().fg(palette.accent).add_modifier(Modifier::BOLD),
    );
    let theme_span = Span::styled(format!(" · {}", theme), Style::new().fg(palette.dim));

    let line = if let Some(msg) = message {
        let prefix_width =
            2 + 3 + 2 + env!("CARGO_PKG_VERSION").len() + mode.len() + theme.len() + 10;
        let available = (area.width as usize).saturating_sub(prefix_width + 2);
        let truncated: String = if msg.len() > available && available > 0 {
            msg.chars()
                .take(available.saturating_sub(1))
                .chain(['…'])
                .collect()
        } else {
            msg.to_string()
        };
        let sep2 = Span::styled(" · ", Style::new().fg(palette.dim));
        let msg_span = Span::styled(truncated, Style::new().fg(palette.dim));
        Line::from(vec![
            logo, name, version, sep, mode_span, theme_span, sep2, msg_span,
        ])
    } else {
        let project = Span::styled(" GLUCK", Style::new().fg(palette.fg));
        let tagline = Span::styled(
            " git log unfolds code into knowledge",
            Style::new().fg(palette.dim).add_modifier(Modifier::ITALIC),
        );
        Line::from(vec![
            logo, name, version, sep, mode_span, theme_span, project, tagline,
        ])
    };

    let header =
        Paragraph::new(line).block(Block::bordered().border_style(Style::new().fg(palette.border)));
    frame.render_widget(header, area);
}

pub fn render_footer(
    frame: &mut ratatui::Frame,
    area: Rect,
    palette: &Palette,
    hints: &[(&str, &str)],
) {
    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(
                    key.to_string(),
                    Style::new()
                        .fg(palette.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {} ", desc)),
            ]
        })
        .collect();
    let footer = Paragraph::new(Line::from(spans));
    frame.render_widget(footer, area);
}

pub fn render_search_bar(frame: &mut ratatui::Frame, area: Rect, palette: &Palette, query: &str) {
    let search = Paragraph::new(format!("/ {}", query))
        .style(Style::new().fg(palette.warning))
        .block(Block::bordered().border_style(Style::new().fg(palette.border)));
    frame.render_widget(search, area);
    frame.set_cursor_position((area.x + 3 + query.width() as u16, area.y + 1));
}
