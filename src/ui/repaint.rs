//! Work around stale cells left behind by wide (CJK) graphemes.
//!
//! ratatui stores a wide grapheme's trailing cell as a blank and skips it in
//! the buffer diff while it stays blank. But when a terminal sees the lead
//! cell of a wide grapheme overwritten, it erases the trailing half itself —
//! keeping that cell's old background. Nothing repaints it, so tinted diff
//! rows (or highlighted text) leave one-cell ghosts after a scroll or file
//! switch.

use ratatui::buffer::{Buffer, CellDiffOption};
use unicode_width::UnicodeWidthStr;

/// Force a repaint of every cell that held the trailing half of a wide
/// grapheme in `prev` whose lead cell changes in `next`.
pub fn force_wide_trails(prev: &Buffer, next: &mut Buffer) {
    if prev.area != next.area {
        return;
    }
    let area = next.area;
    for y in area.top()..area.bottom() {
        let mut x = area.left();
        while x < area.right() {
            let old = &prev[(x, y)];
            let width = old.symbol().width().max(1) as u16;
            if width > 1 && next[(x, y)] != *old {
                for t in x + 1..(x + width).min(area.right()) {
                    let cell = &mut next[(t, y)];
                    if cell.diff_option == CellDiffOption::None {
                        cell.diff_option = CellDiffOption::AlwaysUpdate;
                    }
                }
            }
            x += width;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::{Color, Style};

    fn row(text: &str, bg: Color) -> Buffer {
        let mut buf = Buffer::empty(Rect::new(0, 0, 6, 1));
        buf.set_string(0, 0, text, Style::new().bg(bg));
        buf
    }

    #[test]
    fn test_trailing_cell_repaints_when_lead_changes() {
        let prev = row("한글", Color::Red);
        let mut next = Buffer::empty(prev.area);
        next.set_string(0, 0, "a", Style::new());
        // Without the fix the diff skips cell 1 and 3 (blank before and after).
        let plain: Vec<u16> = prev.diff(&next).iter().map(|(x, _, _)| *x).collect();
        assert_eq!(plain, vec![0, 2]);

        force_wide_trails(&prev, &mut next);
        let fixed: Vec<u16> = prev.diff(&next).iter().map(|(x, _, _)| *x).collect();
        assert_eq!(fixed, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_unchanged_wide_cells_stay_quiet() {
        let prev = row("한a", Color::Red);
        let mut next = row("한b", Color::Red);
        force_wide_trails(&prev, &mut next);
        let xs: Vec<u16> = prev.diff(&next).iter().map(|(x, _, _)| *x).collect();
        assert_eq!(xs, vec![2]);
    }

    #[test]
    fn test_resized_buffers_are_ignored() {
        let prev = row("한", Color::Red);
        let mut next = Buffer::empty(Rect::new(0, 0, 4, 1));
        force_wide_trails(&prev, &mut next);
        assert_eq!(next[(1, 0)].diff_option, CellDiffOption::None);
    }
}
