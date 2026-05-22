use crate::app::App;
use crate::git::diff::{DiffFile, DiffLine};
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Tabs};

pub fn render_diff(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);

    if let Mode::Diff(state) = &app.mode {
        let title = format!("DIFF: {} ↦ {}", state.from.short_id, state.to.short_id);
        layout::render_header(frame, header, &title, Some(&state.to.message));

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

            let tabs = Tabs::new(file_names)
                .select(state.selected_file)
                .highlight_style(Style::new().white().bold())
                .divider("|");
            frame.render_widget(tabs, tabs_row);

            if let Some(file) = state.diff_result.files.get(state.selected_file) {
                if state.side_by_side {
                    render_side_by_side(frame, diff_area, file, state.scroll);
                } else {
                    render_unified(frame, diff_area, file, state.scroll);
                }
            }
        }
    }

    let hints = [
        ("[j/k/←/→]", "file"),
        ("[J/K]", "scroll"),
        ("[^P/^N]", "commit"),
        ("[s]", "view"),
        ("[Tab]", "back"),
        ("[Esc]", "pick"),
    ];
    layout::render_footer(frame, footer, &hints);
}

fn style_for_line(line: &DiffLine) -> Style {
    match line {
        DiffLine::Added { .. } => Style::new().fg(Color::Green),
        DiffLine::Removed { .. } => Style::new().fg(Color::Red),
        DiffLine::Context { .. } => Style::new(),
    }
}

fn render_unified(frame: &mut ratatui::Frame, area: Rect, file: &DiffFile, scroll: usize) {
    let lines: Vec<Line> = file
        .lines
        .iter()
        .map(|dl| {
            let (prefix, line_no, content, style) = match dl {
                DiffLine::Context {
                    old_line_no,
                    new_line_no,
                    content,
                } => {
                    let no = if old_line_no == new_line_no {
                        format!(" {:>4}     ", old_line_no)
                    } else {
                        format!(" {:>4},{:<4} ", old_line_no, new_line_no)
                    };
                    (" ", no, content.clone(), style_for_line(dl))
                }
                DiffLine::Removed { line_no, content } => (
                    "-",
                    format!(" {:>4},_    ", line_no),
                    content.clone(),
                    style_for_line(dl),
                ),
                DiffLine::Added { line_no, content } => (
                    "+",
                    format!(" _,{:<4}    ", line_no),
                    content.clone(),
                    style_for_line(dl),
                ),
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(line_no, Style::new().dark_gray()),
                Span::styled(content, style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().style(Style::new().white()))
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);
}

fn render_side_by_side(frame: &mut ratatui::Frame, area: Rect, file: &DiffFile, scroll: usize) {
    let (left, right) = layout::split_horizontal(area, area.width / 2);

    let old_lines: Vec<Line> = file
        .lines
        .iter()
        .filter(|dl| !matches!(dl, DiffLine::Added { .. }))
        .map(|dl| {
            let (prefix, line_no) = match dl {
                DiffLine::Removed { line_no, .. } => ("-", format!(" {:>4} ", line_no)),
                DiffLine::Context { old_line_no, .. } => (" ", format!(" {:>4} ", old_line_no)),
                DiffLine::Added { .. } => unreachable!(),
            };
            let content = match dl {
                DiffLine::Context { content, .. } => content,
                DiffLine::Removed { content, .. } => content,
                DiffLine::Added { .. } => unreachable!(),
            };
            let style = style_for_line(dl);
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(line_no, Style::new().dark_gray()),
                Span::styled(content.clone(), style),
            ])
        })
        .collect();

    let new_lines: Vec<Line> = file
        .lines
        .iter()
        .filter(|dl| !matches!(dl, DiffLine::Removed { .. }))
        .map(|dl| {
            let (prefix, line_no) = match dl {
                DiffLine::Added { line_no, .. } => ("+", format!(" {:>4} ", line_no)),
                DiffLine::Context { new_line_no, .. } => (" ", format!(" {:>4} ", new_line_no)),
                DiffLine::Removed { .. } => unreachable!(),
            };
            let content = match dl {
                DiffLine::Context { content, .. } => content,
                DiffLine::Added { content, .. } => content,
                DiffLine::Removed { .. } => unreachable!(),
            };
            let style = style_for_line(dl);
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(line_no, Style::new().dark_gray()),
                Span::styled(content.clone(), style),
            ])
        })
        .collect();

    let old_widget = Paragraph::new(old_lines)
        .block(Block::bordered().title(" old ").style(Style::new().white()))
        .scroll((scroll as u16, 0));
    let new_widget = Paragraph::new(new_lines)
        .block(Block::bordered().title(" new ").style(Style::new().white()))
        .scroll((scroll as u16, 0));

    frame.render_widget(old_widget, left);
    frame.render_widget(new_widget, right);
}
