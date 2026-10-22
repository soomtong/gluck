//! Intra-line word diff for Advanced diff color: the parts of a paired
//! removed/added line that actually changed.

use ratatui::style::Style;
use ratatui::text::Span;
use std::ops::Range;

/// Token LCS tables larger than this skip emphasis (very long lines).
const MAX_CELLS: usize = 250_000;

/// Changed byte ranges of the (old, new) line.
pub type ChangedRanges = (Vec<Range<usize>>, Vec<Range<usize>>);

/// Byte ranges of `old` and `new` outside their token-level LCS. None when
/// the lines share too little to make emphasis useful (under 40% of the
/// longer line's non-whitespace text), or are too long to compare. Ranges
/// are trimmed of surrounding whitespace.
pub fn changed_ranges(old: &str, new: &str) -> Option<ChangedRanges> {
    let a = tokenize(old);
    let b = tokenize(new);
    if a.len() * b.len() > MAX_CELLS {
        return None;
    }
    let (keep_a, keep_b) = lcs_keep(old, &a, new, &b);

    let solid = |s: &str, toks: &[Range<usize>], keep: Option<&[bool]>| -> usize {
        toks.iter()
            .enumerate()
            .filter(|(i, _)| keep.is_none_or(|k| k[*i]))
            .filter(|(_, t)| !s[(*t).clone()].trim().is_empty())
            .map(|(_, t)| t.len())
            .sum()
    };
    let shared = solid(new, &b, Some(&keep_b));
    let longest = solid(old, &a, None).max(solid(new, &b, None));
    if shared * 5 < longest * 2 {
        return None;
    }
    Some((unkept(old, &a, &keep_a), unkept(new, &b, &keep_b)))
}

/// Identifier runs, whitespace runs, and single other characters.
fn tokenize(s: &str) -> Vec<Range<usize>> {
    fn class(c: char) -> u8 {
        if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        }
    }
    let mut toks: Vec<Range<usize>> = Vec::new();
    let mut prev = None;
    for (i, c) in s.char_indices() {
        let k = class(c);
        match toks.last_mut() {
            Some(t) if prev == Some(k) && k != 2 => t.end = i + c.len_utf8(),
            _ => toks.push(i..i + c.len_utf8()),
        }
        prev = Some(k);
    }
    toks
}

/// Which tokens of each side belong to the longest common subsequence.
fn lcs_keep(sa: &str, a: &[Range<usize>], sb: &str, b: &[Range<usize>]) -> (Vec<bool>, Vec<bool>) {
    let (n, m) = (a.len(), b.len());
    let eq = |i: usize, j: usize| sa[a[i].clone()] == sb[b[j].clone()];
    // table[i][j] = LCS length of a[i..] and b[j..]
    let w = m + 1;
    let mut table = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * w + j] = if eq(i, j) {
                table[(i + 1) * w + j + 1] + 1
            } else {
                table[(i + 1) * w + j].max(table[i * w + j + 1])
            };
        }
    }
    let (mut keep_a, mut keep_b) = (vec![false; n], vec![false; m]);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if eq(i, j) {
            keep_a[i] = true;
            keep_b[j] = true;
            i += 1;
            j += 1;
        } else if table[(i + 1) * w + j] >= table[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    (keep_a, keep_b)
}

/// Merged byte ranges of the tokens not kept, whitespace-trimmed.
fn unkept(s: &str, toks: &[Range<usize>], keep: &[bool]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for (t, _) in toks.iter().zip(keep).filter(|(_, k)| !**k) {
        match out.last_mut() {
            Some(r) if r.end == t.start => r.end = t.end,
            _ => out.push(t.clone()),
        }
    }
    out.into_iter()
        .filter_map(|r| {
            let text = &s[r.clone()];
            let lead = text.len() - text.trim_start().len();
            let trail = text.len() - text.trim_end().len();
            let trimmed = r.start + lead..r.end - trail;
            (!trimmed.is_empty()).then_some(trimmed)
        })
        .collect()
}

/// Split `spans` at the boundaries of `ranges` (byte offsets into the
/// spans' concatenated text, on char boundaries) and patch `style` onto
/// the parts inside them.
pub fn emphasize(
    spans: Vec<Span<'static>>,
    ranges: &[Range<usize>],
    style: Style,
) -> Vec<Span<'static>> {
    if ranges.is_empty() {
        return spans;
    }
    let mut out = Vec::with_capacity(spans.len() + ranges.len() * 2);
    let mut offset = 0;
    for span in spans {
        let text = span.content.as_ref();
        let (start, end) = (offset, offset + text.len());
        offset = end;
        // Cut points inside this span, from the range edges it contains.
        let mut cuts = vec![start];
        for r in ranges {
            for edge in [r.start, r.end] {
                if edge > start && edge < end {
                    cuts.push(edge);
                }
            }
        }
        cuts.push(end);
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            let (lo, hi) = (pair[0], pair[1]);
            if lo == hi {
                continue;
            }
            let inside = ranges.iter().any(|r| r.start <= lo && hi <= r.end);
            let piece_style = if inside {
                span.style.patch(style)
            } else {
                span.style
            };
            out.push(Span::styled(
                text[lo - start..hi - start].to_string(),
                piece_style,
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn pieces<'a>(s: &'a str, ranges: &[Range<usize>]) -> Vec<&'a str> {
        ranges.iter().map(|r| &s[r.clone()]).collect()
    }

    #[test]
    fn test_changed_ranges_appended_condition() {
        let old = "let timeout = if app.is_indexing() {";
        let new = "let timeout = if app.is_indexing() || app.ask_streaming() {";
        let (a, b) = changed_ranges(old, new).unwrap();
        assert!(a.is_empty());
        assert_eq!(pieces(new, &b), vec!["|| app.ask_streaming()"]);
    }

    #[test]
    fn test_changed_ranges_replaced_call() {
        let old = "let config = Config::load().unwrap_or_default();";
        let new = "let config = Config::load_or_default();";
        let (a, b) = changed_ranges(old, new).unwrap();
        assert_eq!(pieces(new, &b), vec!["load_or_default"]);
        let removed: String = pieces(old, &a).concat();
        assert!(removed.contains("unwrap_or_default"), "{removed}");
        assert!(removed.contains("load"), "{removed}");
    }

    #[test]
    fn test_changed_ranges_skips_unrelated_lines() {
        assert!(changed_ranges("fn alpha(x: u32) {", "    let beta = gamma.delta();").is_none());
    }

    #[test]
    fn test_changed_ranges_identical_and_empty() {
        assert_eq!(changed_ranges("same", "same"), Some((vec![], vec![])));
        assert_eq!(changed_ranges("", ""), Some((vec![], vec![])));
    }

    #[test]
    fn test_tokenize_multibyte() {
        let s = "값 = \"한글\";";
        let toks = tokenize(s);
        let texts: Vec<&str> = toks.iter().map(|r| &s[r.clone()]).collect();
        assert_eq!(texts, vec!["값", " ", "=", " ", "\"", "한글", "\"", ";"]);
    }

    #[test]
    fn test_emphasize_splits_across_spans() {
        let strong = Style::new().bg(Color::Red);
        let kw = Style::new().fg(Color::Blue);
        let spans = vec![Span::styled("let", kw), Span::raw(" abc = 1;")];
        // "c = 1" -> bytes 6..11, starts inside the second span
        let out = emphasize(spans, &[2..5, 6..11], strong);
        let texts: Vec<&str> = out.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(texts, vec!["le", "t", " a", "b", "c = 1", ";"]);
        assert_eq!(out[0].style, kw);
        assert_eq!(out[1].style, kw.patch(strong));
        assert_eq!(out[2].style, strong);
        assert_eq!(out[3].style, Style::new());
        assert_eq!(out[4].style, strong);
        assert_eq!(out[5].style, Style::new());
    }
}
