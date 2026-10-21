//! Serialize what the user is looking at into prompt-ready text. Pure
//! functions over diff lines / file text and line ranges — no UI types.

use crate::git::commit::CommitInfo;
use crate::git::diff::{DiffFile, DiffLine, FileChange};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskKind {
    /// Everything currently visible in the content pane.
    Page,
    /// A mouse-selected block of lines.
    Selection,
}

/// Which pane of a side-by-side diff a selection came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AskContext {
    pub kind: AskKind,
    /// `code` is unified-diff formatted (vs plain numbered source lines).
    pub is_diff: bool,
    pub path: String,
    pub language: Option<&'static str>,
    /// e.g. "modified", "added", "renamed from a.rs".
    pub change: Option<String>,
    pub commit_short: String,
    pub commit_message: String,
    /// Human description of what `code` covers, used in the prompt.
    pub scope: String,
    pub code: String,
    /// Surrounding lines not on screen / not selected (selection only).
    pub before: String,
    pub after: String,
    pub truncated: bool,
}

impl AskContext {
    /// Short label for the panel title, e.g. `selection L12–20 (new)`.
    pub fn title(&self) -> String {
        match self.kind {
            AskKind::Page => format!("page · {}", self.path),
            AskKind::Selection => format!("selection · {} {}", self.path, self.scope_short()),
        }
    }

    fn scope_short(&self) -> &str {
        self.scope
            .rsplit_once('(')
            .map(|(_, s)| s.trim_end_matches(')'))
            .unwrap_or("")
    }
}

fn base(commit: &CommitInfo, path: &str, kind: AskKind, is_diff: bool) -> AskContext {
    AskContext {
        kind,
        is_diff,
        path: path.to_string(),
        language: crate::lang::Language::from_path(path).map(|l| l.as_str()),
        change: None,
        commit_short: commit.short_id.clone(),
        commit_message: commit.message.clone(),
        scope: String::new(),
        code: String::new(),
        before: String::new(),
        after: String::new(),
        truncated: false,
    }
}

fn describe_change(change: &Option<FileChange>) -> Option<String> {
    Some(match change.as_ref()? {
        FileChange::Added { .. } => "added in this commit".to_string(),
        FileChange::Deleted { .. } => "deleted in this commit".to_string(),
        FileChange::Modified { old_path, new_path } if old_path != new_path => {
            format!("renamed from {old_path}")
        }
        FileChange::Modified { .. } => "modified".to_string(),
    })
}

fn diff_path(file: &DiffFile) -> String {
    file.change
        .as_ref()
        .map(|c| c.path().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// `+  12 | code` — new line number for added/unchanged, old for removed.
fn unified_line(dl: &DiffLine) -> String {
    match dl {
        DiffLine::Context {
            new_line_no,
            content,
            ..
        } => format!("  {new_line_no:>4} | {}", strip_eol(content)),
        DiffLine::Added { line_no, content } => format!("+ {line_no:>4} | {}", strip_eol(content)),
        DiffLine::Removed { line_no, content } => {
            format!("- {line_no:>4} | {}", strip_eol(content))
        }
    }
}

/// git2 diff line content keeps its newline.
fn strip_eol(content: &str) -> &str {
    content.trim_end_matches(['\n', '\r'])
}

/// The line as it exists on one side of the diff, or None if it doesn't.
fn side_line(dl: &DiffLine, side: Side) -> Option<String> {
    match (dl, side) {
        (
            DiffLine::Context {
                old_line_no,
                content,
                ..
            },
            Side::Old,
        ) => Some(numbered(*old_line_no as usize, content)),
        (
            DiffLine::Context {
                new_line_no,
                content,
                ..
            },
            Side::New,
        ) => Some(numbered(*new_line_no as usize, content)),
        (DiffLine::Removed { line_no, content }, Side::Old)
        | (DiffLine::Added { line_no, content }, Side::New) => {
            Some(numbered(*line_no as usize, content))
        }
        _ => None,
    }
}

fn numbered(line_no: usize, content: &str) -> String {
    format!("{line_no:>4} | {}", strip_eol(content))
}

fn render_diff<'a>(lines: impl IntoIterator<Item = &'a DiffLine>, side: Option<Side>) -> String {
    let rows: Vec<String> = match side {
        None => lines.into_iter().map(unified_line).collect(),
        Some(s) => lines.into_iter().filter_map(|l| side_line(l, s)).collect(),
    };
    rows.join("\n")
}

fn on_side(line: &DiffLine, side: Option<Side>) -> bool {
    match side {
        None => true,
        Some(s) => side_line(line, s).is_some(),
    }
}

/// Lowest..=highest line number touched on the new (or old) side.
fn line_span(lines: &[&DiffLine], side: Side) -> Option<(u32, u32)> {
    let nos = lines.iter().filter_map(|l| match (*l, side) {
        (DiffLine::Context { new_line_no, .. }, Side::New) => Some(*new_line_no),
        (DiffLine::Context { old_line_no, .. }, Side::Old) => Some(*old_line_no),
        (DiffLine::Added { line_no, .. }, Side::New) => Some(*line_no),
        (DiffLine::Removed { line_no, .. }, Side::Old) => Some(*line_no),
        _ => None,
    });
    nos.fold(None, |acc, n| match acc {
        None => Some((n, n)),
        Some((lo, hi)) => Some((lo.min(n), hi.max(n))),
    })
}

fn span_text(span: (u32, u32)) -> String {
    match span {
        (lo, hi) if lo == hi => format!("L{lo}"),
        (lo, hi) => format!("L{lo}–{hi}"),
    }
}

/// e.g. `new L12–20`. Without a pane, prefers new-file numbers and falls
/// back to old ones (removed-only blocks, deleted files).
fn span_label(lines: &[&DiffLine], side: Option<Side>) -> String {
    let sides: &[Side] = match side {
        Some(Side::Old) => &[Side::Old],
        Some(Side::New) => &[Side::New],
        None => &[Side::New, Side::Old],
    };
    sides
        .iter()
        .find_map(|&s| {
            let name = if s == Side::New { "new" } else { "old" };
            line_span(lines, s).map(|span| format!("{name} {}", span_text(span)))
        })
        .unwrap_or_else(|| "no lines".to_string())
}

/// Resolve `indices` (into `file.lines`, any order) to sorted, in-range,
/// de-duplicated lines.
fn pick_lines<'a>(file: &'a DiffFile, indices: &[usize]) -> (Vec<usize>, Vec<&'a DiffLine>) {
    let mut idx: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|&i| i < file.lines.len())
        .collect();
    idx.sort_unstable();
    idx.dedup();
    let lines = idx.iter().map(|&i| &file.lines[i]).collect();
    (idx, lines)
}

/// `indices` are the `file.lines` entries actually on screen — not a
/// range, since a side-by-side page can show the head of a removed block
/// while the rest of it is scrolled off.
pub fn diff_page(
    commit: &CommitInfo,
    file: &DiffFile,
    indices: &[usize],
    max_chars: usize,
) -> AskContext {
    let (_, lines) = pick_lines(file, indices);
    let mut ctx = base(commit, &diff_path(file), AskKind::Page, true);
    ctx.change = describe_change(&file.change);
    ctx.scope = format!(
        "the part of this file's diff currently visible on screen ({})",
        span_label(&lines, None)
    );
    ctx.code = render_diff(lines, None);
    fit(&mut ctx, max_chars);
    ctx
}

/// `indices` point into `file.lines`. With `side` set (side-by-side
/// pane), only lines that exist on that side are included, and the
/// `extra` surrounding lines are counted on that side too.
pub fn diff_selection(
    commit: &CommitInfo,
    file: &DiffFile,
    indices: &[usize],
    side: Option<Side>,
    extra: usize,
    max_chars: usize,
) -> AskContext {
    let (idx, lines) = pick_lines(file, indices);
    let mut ctx = base(commit, &diff_path(file), AskKind::Selection, side.is_none());
    ctx.change = describe_change(&file.change);
    ctx.scope = format!("the lines the user selected ({})", span_label(&lines, side));
    ctx.code = render_diff(lines, side);
    if let (Some(&first), Some(&last)) = (idx.first(), idx.last()) {
        let mut before: Vec<&DiffLine> = file.lines[..first]
            .iter()
            .rev()
            .filter(|l| on_side(l, side))
            .take(extra)
            .collect();
        before.reverse();
        ctx.before = render_diff(before, side);
        let after = file.lines[last + 1..]
            .iter()
            .filter(|l| on_side(l, side))
            .take(extra);
        ctx.after = render_diff(after, side);
    }
    fit(&mut ctx, max_chars);
    ctx
}

fn clamp(range: Range<usize>, len: usize) -> Range<usize> {
    let end = range.end.min(len);
    range.start.min(end)..end
}

/// `range` indexes the file's lines (0-based).
pub fn file_page(
    commit: &CommitInfo,
    path: &str,
    raw: &str,
    range: Range<usize>,
    changed: bool,
    max_chars: usize,
) -> AskContext {
    let all: Vec<&str> = raw.lines().collect();
    let range = clamp(range, all.len());
    let mut ctx = base(commit, path, AskKind::Page, false);
    ctx.change = changed.then(|| "changed in this commit".to_string());
    ctx.scope = format!(
        "the part of the file currently visible on screen ({})",
        file_span(&range)
    );
    ctx.code = number_lines(&all, range);
    fit(&mut ctx, max_chars);
    ctx
}

pub fn file_selection(
    commit: &CommitInfo,
    path: &str,
    raw: &str,
    range: Range<usize>,
    extra: usize,
    max_chars: usize,
) -> AskContext {
    let all: Vec<&str> = raw.lines().collect();
    let range = clamp(range, all.len());
    let mut ctx = base(commit, path, AskKind::Selection, false);
    ctx.scope = format!("the lines the user selected ({})", file_span(&range));
    ctx.code = number_lines(&all, range.clone());
    ctx.before = number_lines(&all, range.start.saturating_sub(extra)..range.start);
    ctx.after = number_lines(&all, range.end..(range.end + extra).min(all.len()));
    fit(&mut ctx, max_chars);
    ctx
}

fn file_span(range: &Range<usize>) -> String {
    if range.is_empty() {
        "no lines".to_string()
    } else if range.len() == 1 {
        format!("L{}", range.start + 1)
    } else {
        format!("L{}–{}", range.start + 1, range.end)
    }
}

fn number_lines(all: &[&str], range: Range<usize>) -> String {
    all[range.clone()]
        .iter()
        .zip(range)
        .map(|(l, i)| numbered(i + 1, l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Keep the prompt under `max_chars`: drop surrounding context first, then
/// cut the main block line by line. A single line longer than the budget
/// (minified code) is cut mid-line rather than dropped entirely.
fn fit(ctx: &mut AskContext, max_chars: usize) {
    let size = |c: &AskContext| c.code.len() + c.before.len() + c.after.len();
    if size(ctx) <= max_chars {
        return;
    }
    ctx.before.clear();
    ctx.after.clear();
    if ctx.code.len() <= max_chars {
        return;
    }
    let mut kept = 0;
    for line in ctx.code.split_inclusive('\n') {
        if kept + line.len() > max_chars {
            break;
        }
        kept += line.len();
    }
    if kept == 0 {
        kept = (0..=max_chars)
            .rev()
            .find(|&i| ctx.code.is_char_boundary(i))
            .unwrap_or(0);
    }
    ctx.code.truncate(kept);
    let trimmed = ctx.code.trim_end().len();
    ctx.code.truncate(trimmed);
    ctx.truncated = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit() -> CommitInfo {
        CommitInfo {
            id: git2::Oid::ZERO_SHA1,
            short_id: "abc1234".into(),
            author: "T".into(),
            date: std::time::UNIX_EPOCH,
            message: "Use ? operator\n\nPropagate errors instead of unwrap.".into(),
        }
    }

    fn ctx_line(old: u32, new: u32, c: &str) -> DiffLine {
        DiffLine::Context {
            old_line_no: old,
            new_line_no: new,
            content: c.into(),
        }
    }

    fn file() -> DiffFile {
        DiffFile {
            change: Some(FileChange::Modified {
                old_path: "src/a.rs".into(),
                new_path: "src/a.rs".into(),
            }),
            lines: vec![
                ctx_line(1, 1, "fn f() -> Result<()> {"),
                DiffLine::Removed {
                    line_no: 2,
                    content: "    let x = g().unwrap();".into(),
                },
                DiffLine::Added {
                    line_no: 2,
                    content: "    let x = g()?;\n".into(),
                },
                ctx_line(3, 3, "    Ok(())\n"),
                ctx_line(4, 4, "}"),
            ],
        }
    }

    #[test]
    fn test_diff_page_is_unified_with_markers() {
        let ctx = diff_page(&commit(), &file(), &[0, 1, 2, 3, 4], 10_000);
        assert!(ctx.is_diff);
        assert_eq!(ctx.language, Some("rust"));
        assert_eq!(ctx.change.as_deref(), Some("modified"));
        let lines: Vec<&str> = ctx.code.lines().collect();
        assert_eq!(lines[1], "-    2 |     let x = g().unwrap();");
        assert_eq!(lines[2], "+    2 |     let x = g()?;");
        assert_eq!(lines[3], "     3 |     Ok(())");
        assert!(ctx.scope.contains("(new L1–4)"));
        assert!(ctx.before.is_empty());
    }

    #[test]
    fn test_diff_selection_new_side_drops_removed_lines_and_adds_context() {
        let ctx = diff_selection(&commit(), &file(), &[1, 2], Some(Side::New), 1, 10_000);
        assert!(!ctx.is_diff);
        assert_eq!(ctx.code, "   2 |     let x = g()?;");
        assert_eq!(ctx.before, "   1 | fn f() -> Result<()> {");
        assert_eq!(ctx.after, "   3 |     Ok(())");
        assert!(ctx.scope.contains("(new L2)"));
        assert_eq!(ctx.title(), "selection · src/a.rs new L2");
    }

    #[test]
    fn test_diff_selection_unified_keeps_both_sides() {
        let ctx = diff_selection(&commit(), &file(), &[1, 2], None, 0, 10_000);
        assert!(ctx.is_diff);
        assert_eq!(ctx.code.lines().count(), 2);
    }

    #[test]
    fn test_removed_only_selection_and_deleted_file_use_old_numbers() {
        let ctx = diff_selection(&commit(), &file(), &[1], None, 0, 10_000);
        assert!(ctx.scope.contains("(old L2)"), "{}", ctx.scope);
        let deleted = DiffFile {
            change: Some(FileChange::Deleted {
                path: "gone.rs".into(),
            }),
            lines: vec![
                DiffLine::Removed {
                    line_no: 1,
                    content: "a".into(),
                },
                DiffLine::Removed {
                    line_no: 2,
                    content: "b".into(),
                },
            ],
        };
        let ctx = diff_page(&commit(), &deleted, &[0, 1], 10_000);
        assert!(ctx.scope.contains("(old L1–2)"), "{}", ctx.scope);
    }

    #[test]
    fn test_side_context_counts_lines_on_that_side() {
        // new side: 1 l1 | (removed x3) | 2 a | ...
        let mut lines = vec![ctx_line(1, 1, "l1")];
        for n in 2..5 {
            lines.push(DiffLine::Removed {
                line_no: n,
                content: format!("r{n}"),
            });
        }
        lines.push(DiffLine::Added {
            line_no: 2,
            content: "a".into(),
        });
        let file = DiffFile {
            change: None,
            lines,
        };
        let ctx = diff_selection(&commit(), &file, &[4], Some(Side::New), 1, 10_000);
        // The removed run doesn't eat the one-line budget.
        assert_eq!(ctx.before, "   1 | l1");
    }

    #[test]
    fn test_fit_cuts_a_single_overlong_line() {
        let raw = format!("{}\n", "é".repeat(5_000)); // 10_000 bytes, one line
        let ctx = file_page(&commit(), "min.js", &raw, 0..1, false, 1_001);
        assert!(ctx.truncated);
        assert!(!ctx.code.is_empty());
        assert!(ctx.code.len() <= 1_001);
        assert!(ctx.code.starts_with("   1 | é"));
    }

    #[test]
    fn test_file_selection_numbers_from_one() {
        let raw = "a\nb\nc\nd\n";
        let ctx = file_selection(&commit(), "x.py", raw, 1..3, 1, 10_000);
        assert_eq!(ctx.code, "   2 | b\n   3 | c");
        assert_eq!(ctx.before, "   1 | a");
        assert_eq!(ctx.after, "   4 | d");
        assert!(ctx.scope.contains("L2–3"));
    }

    #[test]
    fn test_range_is_clamped() {
        let ctx = file_page(&commit(), "x.txt", "a\nb\n", 1..99, false, 10_000);
        assert_eq!(ctx.code, "   2 | b");
    }

    #[test]
    fn test_fit_drops_context_then_truncates_by_line() {
        let raw: String = (0..100).map(|i| format!("line {i}\n")).collect();
        let ctx = file_selection(&commit(), "x.txt", &raw, 10..90, 10, 200);
        assert!(ctx.before.is_empty() && ctx.after.is_empty());
        assert!(ctx.truncated);
        assert!(ctx.code.len() <= 200);
        assert!(ctx.code.ends_with(|c: char| c.is_ascii_digit()));
    }
}
