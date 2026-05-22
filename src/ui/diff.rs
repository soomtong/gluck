use crate::app::App;
use crate::git::diff::{DiffFile, DiffLine};
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Tabs};

pub fn render_diff(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);

    if let Mode::Diff(state) = &app.mode {
        let palette = &app.palette;
        let title = format!("DIFF: {} ↦ {}", state.from.short_id, state.to.short_id);
        layout::render_header(
            frame,
            header,
            palette,
            &title,
            &app.theme_name,
            Some(&state.to.message),
        );

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

            let (tab_offset, visible_names) =
                visible_tabs(&file_names, state.selected_file, tabs_row.width);

            let adjusted_select = state.selected_file.saturating_sub(tab_offset);

            let tabs = Tabs::new(visible_names)
                .select(adjusted_select)
                .highlight_style(Style::new().fg(palette.fg).add_modifier(Modifier::BOLD).add_modifier(Modifier::UNDERLINED))
                .divider("|");
            frame.render_widget(tabs, tabs_row);

            if let Some(file) = state.diff_result.files.get(state.selected_file) {
                if state.side_by_side {
                    render_side_by_side(frame, diff_area, file, state.scroll, palette);
                } else {
                    render_unified(frame, diff_area, file, state.scroll, palette);
                }
            }
        }
    }

    let hints = [
        ("[j/k/←/→]", "file"),
        ("[u/d]", "scroll"),
        ("[J/K]", "page"),
        ("[^P/^N]", "commit"),
        ("[s]", "view"),
        ("[Tab]", "back"),
        ("[Esc]", "pick"),
    ];
    layout::render_footer(frame, footer, &app.palette, &hints);
}

fn visible_tabs(file_names: &[String], selected: usize, area_width: u16) -> (usize, Vec<String>) {
    if file_names.is_empty() {
        return (0, vec![]);
    }

    let width = area_width as usize;
    let divider_len = " | ".len();

    let max_fit = {
        let mut n = 0;
        let mut w = 0;
        for name in file_names {
            let need = if n == 0 { name.len() } else { divider_len + name.len() };
            if w + need > width {
                break;
            }
            w += need;
            n += 1;
        }
        n.max(1)
    };

    let right_margin = match max_fit {
        0 | 1 => 0,
        2..=4 => 1,
        _ => 2,
    };

    // 1. selected tab
    let mut used = file_names[selected].len();

    // 2. ensure at least right_margin tabs to the right
    let mut right_count = 0;
    for i in 1..=right_margin {
        let idx = selected + i;
        if idx >= file_names.len() {
            break;
        }
        if used + divider_len + file_names[idx].len() > width {
            break;
        }
        used += divider_len + file_names[idx].len();
        right_count = i;
    }

    // 3. fill left with as many as fit
    let mut left_count = 0;
    for i in (0..selected).rev() {
        if used + divider_len + file_names[i].len() > width {
            break;
        }
        used += divider_len + file_names[i].len();
        left_count += 1;
    }

    // 4. fill any remaining space with more right tabs
    for i in (right_count + 1).. {
        let idx = selected + i;
        if idx >= file_names.len() {
            break;
        }
        if used + divider_len + file_names[idx].len() > width {
            break;
        }
        used += divider_len + file_names[idx].len();
        right_count = i;
    }

    let offset = selected - left_count;
    let end = selected + 1 + right_count;
    let visible: Vec<String> = file_names[offset..end].to_vec();
    (offset, visible)
}

fn style_for_line(line: &DiffLine, palette: &crate::theme::Palette) -> Style {
    match line {
        DiffLine::Added { .. } => Style::new().fg(palette.added),
        DiffLine::Removed { .. } => Style::new().fg(palette.removed),
        DiffLine::Context { .. } => Style::new(),
    }
}

fn render_unified(
    frame: &mut ratatui::Frame,
    area: Rect,
    file: &DiffFile,
    scroll: usize,
    palette: &crate::theme::Palette,
) {
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
                    (" ", no, content.clone(), style_for_line(dl, palette))
                }
                DiffLine::Removed { line_no, content } => (
                    "-",
                    format!(" {:>4},_    ", line_no),
                    content.clone(),
                    style_for_line(dl, palette),
                ),
                DiffLine::Added { line_no, content } => (
                    "+",
                    format!(" _,{:<4}    ", line_no),
                    content.clone(),
                    style_for_line(dl, palette),
                ),
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(line_no, Style::new().fg(palette.dim)),
                Span::styled(content, style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().border_style(Style::new().fg(palette.border)))
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);
}

fn render_side_by_side(
    frame: &mut ratatui::Frame,
    area: Rect,
    file: &DiffFile,
    scroll: usize,
    palette: &crate::theme::Palette,
) {
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
            let style = style_for_line(dl, palette);
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(line_no, Style::new().fg(palette.dim)),
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
            let style = style_for_line(dl, palette);
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(line_no, Style::new().fg(palette.dim)),
                Span::styled(content.clone(), style),
            ])
        })
        .collect();

    let old_widget = Paragraph::new(old_lines)
        .block(
            Block::bordered()
                .title(" old ")
                .border_style(Style::new().fg(palette.border)),
        )
        .scroll((scroll as u16, 0));
    let new_widget = Paragraph::new(new_lines)
        .block(
            Block::bordered()
                .title(" new ")
                .border_style(Style::new().fg(palette.border)),
        )
        .scroll((scroll as u16, 0));

    frame.render_widget(old_widget, left);
    frame.render_widget(new_widget, right);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visible_tabs_empty() {
        let (offset, visible) = visible_tabs(&[], 0, 80);
        assert_eq!(offset, 0);
        assert!(visible.is_empty());
    }

    #[test]
    fn test_visible_tabs_all_fit() {
        let names = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let (offset, visible) = visible_tabs(&names, 1, 80);
        assert_eq!(offset, 0);
        assert_eq!(visible, names);
    }

    #[test]
    fn test_visible_tabs_scroll_to_selected() {
        let names: Vec<String> = (0..20).map(|i| format!("file_{:02}.rs", i)).collect();
        let (offset, visible) = visible_tabs(&names, 15, 80);
        assert!(offset > 0, "should scroll when selected is beyond visible range");
        let adjusted = 15 - offset;
        assert_eq!(visible[adjusted], "file_15.rs");
    }

    #[test]
    fn test_visible_tabs_selected_at_zero() {
        let names: Vec<String> = (0..20).map(|i| format!("file_{:02}.rs", i)).collect();
        let (offset, _visible) = visible_tabs(&names, 0, 80);
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_visible_tabs_narrow_width() {
        let names = vec!["aaaaa.rs".to_string(), "bbbbb.rs".to_string()];
        let (offset, visible) = visible_tabs(&names, 1, 10);
        assert_eq!(offset, 1);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0], "bbbbb.rs");
    }
}
