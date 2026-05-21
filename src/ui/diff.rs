use crate::app::App;
use crate::git::diff::DiffLineKind;
use crate::mode::Mode;
use crate::ui::layout;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

pub fn render_diff(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    let (header, body, footer) = layout::app_layout(area);

    if let Mode::Diff(state) = &app.mode {
        let title = format!(
            "gluck - Diff: {} vs {}",
            state.from.short_id, state.to.short_id
        );
        layout::render_header(frame, header, &title);

        let file = state.diff_result.files.get(state.selected_file);
        let file_name = file
            .map(|f| f.new_path.as_deref().unwrap_or("?"))
            .unwrap_or("no file");

        if let Some(file) = file {
            if state.side_by_side {
                render_side_by_side(frame, body, file, state.scroll);
            } else {
                render_unified(frame, body, file_name, file, state.scroll);
            }
        } else {
            let empty = Paragraph::new("No diff").block(Block::bordered());
            frame.render_widget(empty, body);
        }
    }

    let hints = [("[j/k]", "move"), ("[s]", "toggle view"), ("[Tab]", "back"), ("[Esc]", "pick")];
    layout::render_footer(frame, footer, &hints);
}

fn style_for_kind(kind: &DiffLineKind) -> Style {
    match kind {
        DiffLineKind::Added => Style::new().fg(Color::Green),
        DiffLineKind::Removed => Style::new().fg(Color::Red),
        DiffLineKind::Context => Style::new(),
    }
}

fn render_unified(
    frame: &mut ratatui::Frame,
    area: Rect,
    file_name: &str,
    file: &crate::git::diff::DiffFile,
    scroll: usize,
) {
    let lines: Vec<Line> = file
        .lines
        .iter()
        .map(|dl| {
            let prefix = match dl.kind {
                DiffLineKind::Added => "+",
                DiffLineKind::Removed => "-",
                DiffLineKind::Context => " ",
            };
            let style = style_for_kind(&dl.kind);
            Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(dl.content.clone(), style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(
            Block::bordered()
                .title(format!(" {} ", file_name))
                .style(Style::new().white()),
        )
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, area);
}

fn render_side_by_side(
    frame: &mut ratatui::Frame,
    area: Rect,
    file: &crate::git::diff::DiffFile,
    scroll: usize,
) {
    let (left, right) = layout::split_horizontal(area, area.width / 2);

    let old_lines: Vec<Line> = file
        .lines
        .iter()
        .filter(|dl| dl.kind != DiffLineKind::Added)
        .map(|dl| {
            let prefix = match dl.kind {
                DiffLineKind::Removed => "-",
                _ => " ",
            };
            let style = style_for_kind(&dl.kind);
            Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(dl.content.clone(), style),
            ])
        })
        .collect();

    let new_lines: Vec<Line> = file
        .lines
        .iter()
        .filter(|dl| dl.kind != DiffLineKind::Removed)
        .map(|dl| {
            let prefix = match dl.kind {
                DiffLineKind::Added => "+",
                _ => " ",
            };
            let style = style_for_kind(&dl.kind);
            Line::from(vec![
                Span::styled(prefix.to_string(), style),
                Span::styled(dl.content.clone(), style),
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