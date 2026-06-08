use crate::app::App;
use crate::git::tree::EntryKind;
use crate::mode::{FileContent, Mode};
use crate::ui::layout;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

fn entry_depth(entry: &crate::git::tree::FileEntry) -> usize {
    let path = entry.path.strip_suffix('/').unwrap_or(&entry.path);
    path.matches('/').count()
}

pub fn render_view(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);

    if let Mode::View(state) = &app.mode {
        let palette = &app.palette;

        let timestamp = layout::format_header_date(state.commit.date);
        layout::render_header(
            frame,
            header,
            &app.palette,
            "VIEW",
            &timestamp,
            Some(&state.commit.message),
        );
        let (left, right) = layout::split_horizontal(body, 36);

        let items: Vec<ListItem> = state
            .tree
            .iter()
            .map(|entry| {
                let indent = "  ".repeat(entry_depth(entry));
                let marker = if state.changed_paths.contains(&entry.path) {
                    Span::styled("*", Style::new().fg(palette.warning))
                } else {
                    Span::styled(" ", Style::reset())
                };
                let suffix = match entry.kind {
                    EntryKind::Directory => "/",
                    EntryKind::File => "",
                };

                let mut spans = vec![
                    marker,
                    Span::raw(format!("{}{}{}", indent, entry.name, suffix)),
                ];

                if let Some(&(added, removed)) = state.changed_stats.get(&entry.path) {
                    if added > 0 {
                        spans.push(Span::raw(" "));
                        spans.push(Span::styled(
                            format!("+{}", added),
                            Style::new().fg(palette.added),
                        ));
                    }
                    if removed > 0 {
                        spans.push(Span::raw(" "));
                        spans.push(Span::styled(
                            format!("-{}", removed),
                            Style::new().fg(palette.removed),
                        ));
                    }
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let tree_list = List::new(items)
            .block(
                Block::bordered()
                    .title(format!(" {} ", state.commit.short_id))
                    .border_style(Style::new().fg(palette.border)),
            )
            .highlight_style(palette.highlight_style())
            .scroll_padding(3);

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected_file));
        frame.render_stateful_widget(tree_list, left, &mut list_state);

        let file_name = state
            .tree
            .get(state.selected_file)
            .map(|e| e.path.as_str())
            .unwrap_or("no file");

        let lines: Vec<Line> = match &state.file_content {
            FileContent::NotLoaded => {
                vec![Line::from(Span::styled(
                    "(select a file to view)",
                    Style::new().fg(palette.dim),
                ))]
            }
            FileContent::Binary => {
                vec![Line::from(Span::styled(
                    "(binary file)",
                    Style::new().fg(palette.dim),
                ))]
            }
            FileContent::Text { highlighted, .. } => {
                if !highlighted.is_empty() {
                    highlighted
                        .iter()
                        .enumerate()
                        .map(|(i, line)| {
                            let mut spans = vec![Span::styled(
                                format!("{:>4} ", i + 1),
                                Style::new().fg(palette.dim),
                            )];
                            spans.extend(line.spans.clone());
                            Line::from(spans)
                        })
                        .collect()
                } else {
                    vec![Line::raw("")]
                }
            }
        };

        let content = Paragraph::new(lines)
            .block(
                Block::bordered()
                    .title(format!(" {} ", file_name))
                    .border_style(Style::new().fg(palette.border)),
            )
            .scroll((state.scroll as u16, 0));

        frame.render_widget(content, right);
    }

    let hints = [
        ("[j/k]", "move"),
        ("[u/d]", "scroll"),
        ("[J/K]", "page"),
        ("[^P/^N]", "commit"),
        ("[.]", "ign"),
        ("[Enter]", "open"),
        ("[Tab]", "diff"),
        ("[Esc]", "back"),
    ];
    layout::render_footer(frame, footer, &app.palette, &hints);
}
