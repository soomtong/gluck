use crate::app::App;
use crate::git::tree::EntryKind;
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

fn entry_depth(entry: &crate::git::tree::FileEntry) -> usize {
    let path = entry.path.strip_suffix('/').unwrap_or(&entry.path);
    path.matches('/').count()
}

pub fn render_view(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);
    layout::render_header(frame, header, "gluck - View Mode");

    if let Mode::View(state) = &app.mode {
        let (left, right) = layout::split_horizontal(body, 36);

        let items: Vec<ListItem> = state
            .tree
            .iter()
            .map(|entry| {
                let indent = "  ".repeat(entry_depth(entry));
                let marker = if state.changed_paths.contains(&entry.path) {
                    "*"
                } else {
                    " "
                };
                let suffix = match entry.kind {
                    EntryKind::Directory => "/",
                    EntryKind::File => "",
                };
                ListItem::new(format!("{}{}{}{}", marker, indent, entry.name, suffix))
            })
            .collect();

        let tree_list = List::new(items)
            .block(
                Block::bordered()
                    .title(format!(" {} ", state.commit.short_id))
                    .style(Style::new().white()),
            )
            .highlight_style(Style::new().black().on_white());

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected_file));
        frame.render_stateful_widget(tree_list, left, &mut list_state);

        let file_name = state
            .tree
            .get(state.selected_file)
            .map(|e| e.path.as_str())
            .unwrap_or("no file");

        let lines: Vec<Line> = if !state.highlighted.is_empty() {
            state
                .highlighted
                .iter()
                .enumerate()
                .map(|(i, line)| {
                    let mut spans = vec![Span::styled(
                        format!("{:>4} ", i + 1),
                        Style::new().dark_gray(),
                    )];
                    spans.extend(line.spans.clone());
                    Line::from(spans)
                })
                .collect()
        } else {
            let content_text = state
                .content
                .as_deref()
                .unwrap_or("(select a file to view)");

            content_text
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    Line::from(vec![
                        Span::styled(
                            format!("{:>4} ", i + 1),
                            Style::new().dark_gray(),
                        ),
                        Span::raw(line.to_string()),
                    ])
                })
                .collect()
        };

        let content = Paragraph::new(lines)
            .block(
                Block::bordered()
                    .title(format!(" {} ", file_name))
                    .style(Style::new().white()),
            )
            .scroll((state.scroll as u16, 0));

        frame.render_widget(content, right);
    }

    let hints = [("[j/k]", "move"), ("[J/K]", "scroll"), ("[^P/^N]", "commit"), ("[.]", "ign"), ("[Enter]", "open"), ("[Tab]", "diff"), ("[Esc]", "back")];
    layout::render_footer(frame, footer, &hints);
}