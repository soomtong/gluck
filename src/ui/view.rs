use crate::app::App;
use crate::git::tree::EntryKind;
use crate::mode::{FileContent, Mode};
use crate::ui::{layout, wrap};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

/// Columns used by the `{:>4} ` line-number gutter.
const GUTTER_WIDTH: usize = 5;

fn entry_depth(entry: &crate::git::tree::FileEntry) -> usize {
    let path = entry.path.strip_suffix('/').unwrap_or(&entry.path);
    path.matches('/').count()
}

pub fn render_view(frame: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let (header, body, footer) = layout::app_layout(area);
    let (left, right) = layout::split_horizontal(body, 36);
    let mut tree_offset = 0;

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

        let items: Vec<ListItem> = state
            .visible
            .iter()
            .map(|&tree_idx| {
                let entry = &state.tree[tree_idx];
                let is_dir = matches!(entry.kind, EntryKind::Directory);
                let collapsed = is_dir && state.collapsed.contains(&entry.path);
                let indent = "  ".repeat(entry_depth(entry));
                // A collapsed directory inherits the change marker of
                // anything hidden inside it.
                let changed = state.changed_paths.contains(&entry.path)
                    || (collapsed && {
                        let prefix = format!("{}/", entry.path);
                        state.changed_paths.iter().any(|p| p.starts_with(&prefix))
                    });
                let marker = if changed {
                    Span::styled("*", Style::new().fg(palette.warning))
                } else {
                    Span::styled(" ", Style::reset())
                };
                let fold_icon = if is_dir {
                    if collapsed {
                        "▸ "
                    } else {
                        "▾ "
                    }
                } else {
                    ""
                };
                let suffix = if is_dir { "/" } else { "" };

                let mut spans = vec![
                    marker,
                    Span::raw(format!("{}{}{}{}", indent, fold_icon, entry.name, suffix)),
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
        tree_offset = list_state.offset();

        let file_name = state
            .selected_entry()
            .map(|e| e.path.as_str())
            .unwrap_or("no file");

        let content_height = right.height.saturating_sub(2) as usize;
        let lines: Vec<Line> = match &state.file_content {
            FileContent::NotLoaded => {
                vec![Line::from(Span::styled(
                    "(select a file to view)",
                    Style::new().fg(palette.dim),
                ))]
            }
            FileContent::Loading => {
                vec![Line::from(Span::styled(
                    "(loading...)",
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
                    // Materialize only the visible window; cloning every
                    // line of a large file each frame stalls navigation.
                    // With wrap on, one logical line may take several rows,
                    // so keep pulling lines until the pane is full.
                    let wrap_width = if app.config.ui.word_wrap {
                        (right.width as usize)
                            .saturating_sub(2)
                            .saturating_sub(GUTTER_WIDTH)
                    } else {
                        0
                    };
                    let gutter_style = Style::new().fg(palette.dim);
                    let mut rows: Vec<Line> = Vec::with_capacity(content_height);
                    for (i, line) in highlighted.iter().enumerate().skip(state.scroll) {
                        if rows.len() >= content_height {
                            break;
                        }
                        for (r, row) in wrap::wrap_spans(&line.spans, wrap_width)
                            .into_iter()
                            .enumerate()
                        {
                            let gutter = if r == 0 {
                                format!("{:>4} ", i + 1)
                            } else {
                                " ".repeat(GUTTER_WIDTH)
                            };
                            let mut spans = vec![Span::styled(gutter, gutter_style)];
                            spans.extend(row);
                            rows.push(Line::from(spans));
                        }
                    }
                    rows
                } else {
                    vec![Line::raw("")]
                }
            }
        };

        let content = Paragraph::new(lines).block(
            Block::bordered()
                .title(format!(" {} ", file_name))
                .border_style(Style::new().fg(palette.border)),
        );

        frame.render_widget(content, right);
    }

    app.view_tree_area = Some(left);
    app.view_content_area = Some(right);
    app.view_tree_offset = tree_offset;

    let mut hints = vec![
        ("[j/k]", "move"),
        ("[h/l]", "fold"),
        ("[u/d]", "scroll"),
        ("[J/K]", "page"),
        ("[H/L]", "change"),
        ("[^P/^N]", "commit"),
        ("[.]", "ign"),
        ("[w]", "wrap"),
        ("[Enter]", "open"),
        ("[Tab]", "diff"),
        ("[Esc]", "back"),
    ];
    if app.repo_changed {
        hints.push(("[!]", "repo changed"));
    }
    layout::render_footer(frame, footer, &app.palette, &hints);
}
