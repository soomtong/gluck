use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::search::modal::{Section, SemanticSearchModal};

pub fn render_search_modal(frame: &mut Frame, area: Rect, modal: &SemanticSearchModal) {
    let popup = centered_rect(76, 72, area);
    frame.render_widget(Clear, popup);

    let border_style = Style::default().fg(Color::Cyan);
    let block = Block::default()
        .title(" Semantic Search (S) ")
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if modal.no_index {
        render_message(frame, inner, &[
            Line::from(""),
            Line::from(Span::styled("  No search index found.", Style::default().fg(Color::Yellow))),
            Line::from(Span::raw("  Run `glc index` to build one.")),
            Line::from(""),
            Line::from(Span::styled("  [Esc] close", Style::default().fg(Color::DarkGray))),
        ]);
        return;
    }

    if modal.incompatible {
        render_message(frame, inner, &[
            Line::from(""),
            Line::from(Span::styled("  Index format outdated.", Style::default().fg(Color::Red))),
            Line::from(Span::raw("  Run `glc index --force` to rebuild.")),
            Line::from(""),
            Line::from(Span::styled("  [Esc] close", Style::default().fg(Color::DarkGray))),
        ]);
        return;
    }

    // Layout: [input 3] [warning 1?] [results rest] [help 1]
    let warning_lines = if modal.warning.is_some() { 1u16 } else { 0u16 };
    let constraints = [
        Constraint::Length(3),
        Constraint::Length(warning_lines),
        Constraint::Min(4),
        Constraint::Length(1),
    ];
    let chunks = Layout::vertical(constraints).split(inner);

    // Input
    let input_block = Block::default().borders(Borders::ALL).title(" Query ");
    let input_widget = Paragraph::new(modal.input.as_str()).block(input_block);
    frame.render_widget(input_widget, chunks[0]);

    // Stale warning
    if let Some(ref w) = modal.warning {
        let warn = Paragraph::new(Span::styled(
            format!("  ⚠ {w}"),
            Style::default().fg(Color::Yellow),
        ));
        frame.render_widget(warn, chunks[1]);
    }

    // Results: split 50/50 between Files and Commits
    let result_area = chunks[2];
    let [files_area, commits_area] = Layout::vertical([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ]).areas(result_area);

    render_section(frame, files_area, "Files", &modal.file_results, &modal.focused_section, Section::Files, modal.selected);
    render_section(frame, commits_area, "Commits", &modal.commit_results, &modal.focused_section, Section::Commits, modal.selected);

    // Help bar
    let help = Paragraph::new(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(Color::Green)),
        Span::raw(" open  "),
        Span::styled("[Esc]", Style::default().fg(Color::Green)),
        Span::raw(" close  "),
        Span::styled("[Tab]", Style::default().fg(Color::Green)),
        Span::raw(" section  "),
        Span::styled("[j/k↑↓]", Style::default().fg(Color::Green)),
        Span::raw(" navigate"),
    ]));
    frame.render_widget(help, chunks[3]);
}

fn render_section(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    results: &[crate::search::SearchResult],
    focused: &Section,
    this_section: Section,
    selected: usize,
) {
    let is_focused = *focused == this_section;
    let title_style = if is_focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(Span::styled(format!(" {title} "), title_style))
        .borders(Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let is_selected = is_focused && i == selected;
            let marker = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(r.title.clone(), style),
                Span::styled(
                    format!("  {:.3}", r.score),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

fn render_message(frame: &mut Frame, area: Rect, lines: &[Line]) {
    let para = Paragraph::new(lines.to_vec());
    frame.render_widget(para, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vert[1])[1]
}
