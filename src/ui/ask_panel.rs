//! Bottom drawer showing the streamed LLM answer. Docked below the content
//! (not a centered overlay) so the code being explained stays visible.

use crate::app::App;
use crate::ask::panel::PanelStatus;
use crate::ui::wrap;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Split the mode body into (content, panel) when the panel is open.
pub fn split(body: Rect, open: bool) -> (Rect, Option<Rect>) {
    if !open {
        return (body, None);
    }
    let [top, bottom] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(body);
    (top, Some(bottom))
}

pub fn render(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let palette = &app.palette;
    let panel = &app.ask_panel;
    let status = match panel.status {
        PanelStatus::Streaming => {
            let tick = panel.started.elapsed().as_millis() / 100;
            SPINNER[tick as usize % SPINNER.len()].to_string()
        }
        PanelStatus::Done => {
            let mut s = "done".to_string();
            if panel.truncated {
                s.push_str(" (truncated)");
            }
            if panel.cached {
                s.push_str(" · cached");
            }
            s
        }
        PanelStatus::Failed => "error".to_string(),
        PanelStatus::Closed => String::new(),
    };
    let mut title = vec![
        Span::styled(
            " Ask ",
            Style::new().fg(palette.accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("· {} ", panel.title)),
    ];
    if !panel.label.is_empty() {
        title.push(Span::styled(
            format!("· {} ", panel.label),
            Style::new().fg(palette.dim),
        ));
    }
    title.push(Span::styled(
        format!("· {status} "),
        Style::new().fg(palette.warning),
    ));
    let block = Block::bordered()
        .title(Line::from(title))
        .border_style(Style::new().fg(palette.accent));
    let inner = block.inner(area);
    let width = inner.width as usize;
    let height = inner.height as usize;

    let source: Vec<Line<'static>> = if !panel.rendered.is_empty() {
        panel.rendered.clone()
    } else {
        let mut lines: Vec<Line<'static>> = panel
            .text
            .lines()
            .map(|l| Line::raw(l.to_string()))
            .collect();
        if panel.status == PanelStatus::Streaming && lines.is_empty() {
            lines.push(Line::styled(
                "(waiting for the model…)",
                Style::new().fg(palette.dim),
            ));
        }
        lines
    };
    let mut rows: Vec<Line<'static>> = Vec::new();
    for line in &source {
        for row in wrap::wrap_spans(&line.spans, width) {
            rows.push(Line::from(row));
        }
    }
    if let Some(err) = &panel.error {
        if !rows.is_empty() {
            rows.push(Line::raw(""));
        }
        let style = Style::new().fg(palette.removed);
        for l in err.lines() {
            for row in wrap::wrap_spans(&[Span::styled(l.to_string(), style)], width) {
                rows.push(Line::from(row));
            }
        }
    }

    let total = rows.len();
    app.ask_panel.clamp_scroll(total, height);
    let scroll = app.ask_panel.scroll;
    let visible: Vec<Line> = rows.into_iter().skip(scroll).take(height).collect();

    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(visible).block(block), area);
    app.ask_panel_area = Some(area);
}
