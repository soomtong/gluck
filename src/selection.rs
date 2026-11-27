//! Line-granular mouse selection over the View/Diff content panes.
//!
//! Render code records, per screen row, which logical line it shows
//! (`ContentRows`); mouse handlers map rows back to logical lines. Logical
//! indices are file lines in View, `file.lines` indices in unified Diff and
//! aligned-pair indices in side-by-side Diff — `ContentTarget` pins which
//! space a selection lives in, so a stale one is dropped on any change.

use crate::ask::context::Side;
use crate::git::diff::DiffLine;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentTarget {
    View {
        commit: git2::Oid,
        path: String,
    },
    Diff {
        to: git2::Oid,
        file: usize,
        side_by_side: bool,
    },
}

/// Content pane geometry captured at render time.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentRows {
    pub target: ContentTarget,
    /// Inner (border-less) area; `rows[i]` is the row at `inner.y + i`.
    pub inner: Rect,
    /// Side-by-side pane inner rects (old, new); None for single panes.
    pub panes: Option<(Rect, Rect)>,
    pub rows: Vec<Option<usize>>,
}

impl ContentRows {
    /// Logical line at a screen row, clamping rows outside the pane to the
    /// first/last line shown (dragging past an edge).
    pub fn line_at(&self, row: u16) -> Option<usize> {
        if row < self.inner.y {
            return self.rows.iter().flatten().next().copied();
        }
        let idx = (row - self.inner.y) as usize;
        match self.rows.get(idx) {
            Some(l) => *l,
            None => self.rows.iter().flatten().next_back().copied(),
        }
    }

    pub fn contains(&self, col: u16, row: u16) -> bool {
        match self.panes {
            Some((l, r)) => rect_has(l, col, row) || rect_has(r, col, row),
            None => rect_has(self.inner, col, row),
        }
    }

    pub fn side_at(&self, col: u16) -> Option<Side> {
        let (_, right) = self.panes?;
        Some(if col >= right.x { Side::New } else { Side::Old })
    }

    /// First and last logical line visible on screen.
    pub fn visible_bounds(&self) -> Option<(usize, usize)> {
        let mut it = self.rows.iter().flatten();
        let first = *it.next()?;
        let last = it.next_back().copied().unwrap_or(first);
        Some((first, last))
    }
}

fn rect_has(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub target: ContentTarget,
    /// Pane of a side-by-side diff; None elsewhere.
    pub side: Option<Side>,
    pub anchor: usize,
    pub head: usize,
    pub dragging: bool,
    /// A press without any drag is a click, which clears the selection.
    pub moved: bool,
}

impl Selection {
    pub fn bounds(&self) -> (usize, usize) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    pub fn contains(&self, line: usize, side: Option<Side>) -> bool {
        let (lo, hi) = self.bounds();
        (lo..=hi).contains(&line) && (self.side.is_none() || self.side == side)
    }
}

pub fn selected_style() -> Style {
    Style::new().add_modifier(Modifier::REVERSED)
}

pub fn mark_selected(rows: &mut [Line<'static>]) {
    for row in rows {
        for span in &mut row.spans {
            span.style = span.style.patch(selected_style());
        }
    }
}

/// File lines `lo..=hi` (0-based) of `raw`, joined with `\n`.
pub fn file_lines_text(raw: &str, lo: usize, hi: usize) -> String {
    raw.lines()
        .skip(lo)
        .take(hi.saturating_sub(lo) + 1)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Diff lines at `indices` as copyable text. A side-by-side pane (`side`
/// set) copies that side's code as-is; unified copies patch-style lines
/// with a `+`/`-`/` ` marker so mixed old/new lines stay distinguishable.
pub fn diff_lines_text(lines: &[DiffLine], indices: &[usize], side: Option<Side>) -> String {
    indices
        .iter()
        .filter_map(|&i| lines.get(i))
        .filter_map(|dl| {
            let (marker, content) = match dl {
                DiffLine::Context { content, .. } => (' ', content),
                DiffLine::Added { content, .. } => ('+', content),
                DiffLine::Removed { content, .. } => ('-', content),
            };
            let on_side = match (side, marker) {
                (None, _) | (_, ' ') => true,
                (Some(Side::New), m) => m == '+',
                (Some(Side::Old), m) => m == '-',
            };
            let content = content.trim_end_matches(['\n', '\r']);
            on_side.then(|| match side {
                Some(_) => content.to_string(),
                None => format!("{marker}{content}"),
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> ContentRows {
        ContentRows {
            target: ContentTarget::View {
                commit: git2::Oid::ZERO_SHA1,
                path: "a".into(),
            },
            inner: Rect::new(10, 5, 40, 4),
            panes: None,
            rows: vec![Some(7), Some(7), Some(8), None],
        }
    }

    #[test]
    fn test_line_at_maps_and_clamps() {
        let r = rows();
        assert_eq!(r.line_at(5), Some(7));
        assert_eq!(r.line_at(6), Some(7));
        assert_eq!(r.line_at(7), Some(8));
        assert_eq!(r.line_at(8), None);
        assert_eq!(r.line_at(2), Some(7));
        assert_eq!(r.line_at(30), Some(8));
        assert_eq!(r.visible_bounds(), Some((7, 8)));
    }

    #[test]
    fn test_selection_bounds_and_side() {
        let s = Selection {
            target: rows().target,
            side: Some(Side::New),
            anchor: 9,
            head: 4,
            dragging: false,
            moved: true,
        };
        assert_eq!(s.bounds(), (4, 9));
        assert!(s.contains(5, Some(Side::New)));
        assert!(!s.contains(5, Some(Side::Old)));
        assert!(!s.contains(10, Some(Side::New)));
    }

    fn diff() -> Vec<DiffLine> {
        vec![
            DiffLine::Context {
                old_line_no: 1,
                new_line_no: 1,
                content: "fn main() {\n".into(),
            },
            DiffLine::Removed {
                line_no: 2,
                content: "    old();\n".into(),
            },
            DiffLine::Added {
                line_no: 2,
                content: "    new();\r\n".into(),
            },
            DiffLine::Context {
                old_line_no: 3,
                new_line_no: 3,
                content: "}".into(),
            },
        ]
    }

    #[test]
    fn test_file_lines_text_inclusive_and_clamped() {
        let raw = "a\nb\nc\nd\n";
        assert_eq!(file_lines_text(raw, 1, 2), "b\nc");
        assert_eq!(file_lines_text(raw, 3, 3), "d");
        assert_eq!(file_lines_text(raw, 2, 99), "c\nd");
        assert_eq!(file_lines_text(raw, 9, 12), "");
    }

    #[test]
    fn test_diff_lines_text_unified_keeps_markers() {
        assert_eq!(
            diff_lines_text(&diff(), &[0, 1, 2, 3], None),
            " fn main() {\n-    old();\n+    new();\n }"
        );
    }

    #[test]
    fn test_diff_lines_text_side_copies_plain_code() {
        let all = [0, 1, 2, 3];
        assert_eq!(
            diff_lines_text(&diff(), &all, Some(Side::New)),
            "fn main() {\n    new();\n}"
        );
        assert_eq!(
            diff_lines_text(&diff(), &all, Some(Side::Old)),
            "fn main() {\n    old();\n}"
        );
        assert_eq!(diff_lines_text(&diff(), &[9], None), "");
    }
}
