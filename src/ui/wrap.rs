//! Soft line wrapping for the View and Diff content panes.
//!
//! ratatui's `Wrap` breaks continuation rows at column 0, which misaligns
//! them with the line-number gutter, so wrapping is done here on the span
//! level and callers prepend their own gutter to every row.

use ratatui::style::Style;
use ratatui::text::Span;
use unicode_width::UnicodeWidthChar;

struct Cell {
    ch: char,
    width: usize,
    style: Style,
}

/// Split `spans` into rows no wider than `width` display columns.
///
/// Breaks after the last whitespace that fits when one exists (and the
/// row is not just indentation); otherwise breaks mid-token. Zero-width
/// characters stay attached to the character before them. Always returns
/// at least one row; `width == 0` disables wrapping.
pub fn wrap_spans(spans: &[Span<'_>], width: usize) -> Vec<Vec<Span<'static>>> {
    let cells: Vec<Cell> = spans
        .iter()
        .flat_map(|span| {
            let style = span.style;
            span.content.chars().map(move |ch| Cell {
                ch,
                width: ch.width().unwrap_or(0),
                style,
            })
        })
        .collect();

    if cells.is_empty() {
        return vec![Vec::new()];
    }
    if width == 0 {
        return vec![rebuild(&cells)];
    }

    let mut rows = Vec::new();
    let mut start = 0;
    while start < cells.len() {
        let mut used = 0;
        let mut end = start;
        while end < cells.len() && used + cells[end].width <= width {
            used += cells[end].width;
            end += 1;
        }
        // Never split a zero-width char (combining mark) from its base.
        while end < cells.len() && cells[end].width == 0 {
            end += 1;
        }
        if end == cells.len() {
            rows.push(rebuild(&cells[start..]));
            break;
        }
        // A char wider than the whole row still has to go somewhere.
        if end == start {
            end = start + 1;
        }

        let mut brk = end;
        if let Some(ws) = (start..end).rev().find(|&i| cells[i].ch.is_whitespace()) {
            let only_indent = cells[start..ws].iter().all(|c| c.ch.is_whitespace());
            if !only_indent {
                brk = ws + 1;
            }
        }
        rows.push(rebuild(&cells[start..brk]));
        start = brk;
    }
    rows
}

/// Regroup consecutive same-styled cells into spans.
fn rebuild(cells: &[Cell]) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut cur: Option<Style> = None;
    for cell in cells {
        match cur {
            Some(style) if style == cell.style => {}
            _ => {
                if let Some(style) = cur.take() {
                    spans.push(Span::styled(std::mem::take(&mut buf), style));
                }
                cur = Some(cell.style);
            }
        }
        buf.push(cell.ch);
    }
    if let Some(style) = cur {
        spans.push(Span::styled(buf, style));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn text(rows: &[Vec<Span<'static>>]) -> Vec<String> {
        rows.iter()
            .map(|row| row.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn empty_input_yields_one_empty_row() {
        let rows = wrap_spans(&[], 10);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_empty());
    }

    #[test]
    fn short_line_is_unchanged() {
        let rows = wrap_spans(&[Span::raw("hello")], 10);
        assert_eq!(text(&rows), vec!["hello"]);
    }

    #[test]
    fn breaks_after_last_whitespace_that_fits() {
        let rows = wrap_spans(&[Span::raw("foo bar baz qux")], 10);
        assert_eq!(text(&rows), vec!["foo bar ", "baz qux"]);
    }

    #[test]
    fn hard_breaks_long_token() {
        let rows = wrap_spans(&[Span::raw("abcdefghijklmnop")], 5);
        assert_eq!(text(&rows), vec!["abcde", "fghij", "klmno", "p"]);
    }

    #[test]
    fn indentation_alone_does_not_form_a_row() {
        // Breaking after the indent would leave a row of pure whitespace.
        let rows = wrap_spans(&[Span::raw("    verylongidentifier")], 8);
        assert_eq!(text(&rows), vec!["    very", "longiden", "tifier"]);
    }

    #[test]
    fn zero_width_disables_wrapping() {
        let rows = wrap_spans(&[Span::raw("foo bar baz")], 0);
        assert_eq!(text(&rows), vec!["foo bar baz"]);
    }

    #[test]
    fn styles_are_preserved_across_rows() {
        let red = Style::new().fg(Color::Red);
        let blue = Style::new().fg(Color::Blue);
        let spans = [Span::styled("aaaa", red), Span::styled("bbbb", blue)];
        let rows = wrap_spans(&spans, 6);
        assert_eq!(text(&rows), vec!["aaaabb", "bb"]);
        assert_eq!(rows[0][0].style, red);
        assert_eq!(rows[0][1].style, blue);
        assert_eq!(rows[1][0].style, blue);
    }

    #[test]
    fn wide_chars_count_two_columns() {
        let rows = wrap_spans(&[Span::raw("한글테스트")], 4);
        assert_eq!(text(&rows), vec!["한글", "테스", "트"]);
    }

    #[test]
    fn combining_mark_stays_with_base() {
        // "e" + U+0301 followed by more text; the mark must not start a row.
        let rows = wrap_spans(&[Span::raw("abcde\u{301}fg")], 5);
        assert_eq!(text(&rows), vec!["abcde\u{301}", "fg"]);
    }

    #[test]
    fn char_wider_than_row_still_progresses() {
        let rows = wrap_spans(&[Span::raw("한a")], 1);
        assert_eq!(text(&rows), vec!["한", "a"]);
    }
}
