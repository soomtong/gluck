//! Query-time spelling correction against the BM25 word vocabulary.
//!
//! Title and path_terms are word-tokenized, so a misspelled identifier
//! (`is_binray_blob`) matches nothing there and only weak body bigrams remain.
//! Words absent from that vocabulary are mapped to the closest known term by
//! OSA distance (adjacent transposition costs 1). Known words are never
//! touched, so correctly spelled queries rank exactly as before.
//!
//! Short words only accept the edits typing actually produces — a dropped
//! letter, swapped neighbours, a doubled key. A substituted letter usually
//! spells a different real word (`boot`/`bool`, `game`/`gate`), so it is
//! only trusted on long words.

use std::collections::HashMap;

use crate::search::text_prep::split_camel_case;

/// Shorter words have too many neighbours at distance 1 to correct safely.
const MIN_WORD_LEN: usize = 4;
/// Words at least this long may also be corrected by a substituted letter.
/// Distance 2 is never used: on this vocabulary it only produced different
/// real words (`checking` → `chunking`, `replication` → `replicating`).
const LONG_WORD_LEN: usize = 8;

/// Lowercased ASCII-alphabetic query words, split like the word tokenizer
/// (non-alphanumeric boundaries plus camelCase), deduplicated in order.
pub fn query_words(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in split_camel_case(query).split(|c: char| !c.is_ascii_alphanumeric()) {
        if word.len() < MIN_WORD_LEN || !word.bytes().all(|b| b.is_ascii_alphabetic()) {
            continue;
        }
        let word = word.to_ascii_lowercase();
        if !out.contains(&word) {
            out.push(word);
        }
    }
    out
}

/// Whether a single edit turning `word` into `term` looks like a typing slip:
/// `word` dropped a letter of `term`, swapped two neighbours, or repeated a
/// letter. Both are ASCII.
fn is_typing_slip(word: &[u8], term: &[u8]) -> bool {
    let prefix = word.iter().zip(term).take_while(|(a, b)| a == b).count();
    match term.len() as isize - word.len() as isize {
        // Dropped letter: the rest lines up after skipping it in `term`.
        1 => word[prefix..] == term[prefix + 1..],
        // Extra letter: only a repeat of its neighbour (`fussion`).
        -1 => {
            word[prefix + 1..] == term[prefix..]
                && (word.get(prefix + 1) == Some(&word[prefix])
                    || prefix > 0 && word[prefix - 1] == word[prefix])
        }
        // Same length: an adjacent swap, never a substitution.
        0 => {
            prefix + 1 < word.len()
                && word[prefix] == term[prefix + 1]
                && word[prefix + 1] == term[prefix]
                && word[prefix + 2..] == term[prefix + 2..]
        }
        _ => false,
    }
}

/// `a` is `b` plus a plural suffix. Inflections aren't typos, and correcting
/// them pulls in unrelated docs (`injection` → `injections`).
fn is_plural_of(a: &str, b: &str) -> bool {
    a.strip_suffix('s').is_some_and(|s| s == b) || a.strip_suffix("es").is_some_and(|s| s == b)
}

/// For each word missing from `vocab` (term → doc frequency), a vocabulary
/// term one plausible edit away that shares its first letter. Among several,
/// the higher doc frequency wins, then the lexicographically smaller term.
pub fn correct(words: &[String], vocab: &HashMap<String, u32>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for word in words {
        if vocab.contains_key(word) {
            continue;
        }
        let first = word.as_bytes()[0];
        let best = vocab
            .iter()
            .filter(|(term, _)| {
                term.as_bytes().first() == Some(&first) && term.len().abs_diff(word.len()) <= 1
            })
            .filter(|(term, _)| {
                strsim::osa_distance(word, term) == 1
                    && !is_plural_of(word, term)
                    && !is_plural_of(term, word)
                    && (word.len() >= LONG_WORD_LEN
                        || is_typing_slip(word.as_bytes(), term.as_bytes()))
            })
            .map(|(term, &df)| (std::cmp::Reverse(df), term))
            .min();
        if let Some((_, term)) = best {
            out.push((word.clone(), term.clone()));
        }
    }
    out
}

/// Appends the corrected terms so the original words still contribute their
/// body-bigram matches.
pub fn append_corrections(query: &str, corrections: &[(String, String)]) -> String {
    let mut out = query.to_string();
    for (_, fixed) in corrections {
        out.push(' ');
        out.push_str(fixed);
    }
    out
}

/// Replaces whole alphanumeric runs equal (case-insensitively) to a typo.
/// Typos inside a camelCase run are left alone.
pub fn replace_corrections(query: &str, corrections: &[(String, String)]) -> String {
    let mut out = String::with_capacity(query.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        let lower = run.to_ascii_lowercase();
        match corrections.iter().find(|(typo, _)| *typo == lower) {
            Some((_, fixed)) => out.push_str(fixed),
            None => out.push_str(run),
        }
        run.clear();
    };
    for c in query.chars() {
        if c.is_ascii_alphanumeric() {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab(terms: &[(&str, u32)]) -> HashMap<String, u32> {
        terms.iter().map(|(t, df)| (t.to_string(), *df)).collect()
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn query_words_splits_identifiers_and_skips_short_or_numeric() {
        assert_eq!(
            query_words("is_binray_blob buildIndex v2 dat Blob"),
            vec!["binray", "blob", "build", "index"]
        );
    }

    #[test]
    fn correct_fixes_transposition_and_deletion() {
        let v = vocab(&[("binary", 3), ("blob", 2), ("fusion", 1), ("search", 9)]);
        let words = query_words("is_binray_blob fussion serach");
        assert_eq!(
            correct(&words, &v),
            pairs(&[
                ("binray", "binary"),
                ("fussion", "fusion"),
                ("serach", "search")
            ])
        );
    }

    #[test]
    fn correct_leaves_known_and_distant_words() {
        let v = vocab(&[("spring", 1), ("boot", 1), ("root", 5)]);
        // "boot" is known; "django" has no neighbour.
        assert!(correct(&query_words("spring boot django"), &v).is_empty());
    }

    #[test]
    fn correct_requires_same_first_letter() {
        let v = vocab(&[("root", 5)]);
        assert!(correct(&query_words("boot"), &v).is_empty());
    }

    #[test]
    fn correct_rejects_substitution_on_short_words() {
        let v = vocab(&[("bool", 5), ("gate", 1), ("rect", 1), ("diff", 3)]);
        assert!(correct(&query_words("boot game react diffs"), &v).is_empty());
    }

    #[test]
    fn correct_skips_plural_forms() {
        let v = vocab(&[("injections", 1), ("download", 2), ("boxes", 1)]);
        assert!(correct(&query_words("injection downloads box"), &v).is_empty());
    }

    #[test]
    fn typing_slip_kinds() {
        let slip = |w: &str, t: &str| is_typing_slip(w.as_bytes(), t.as_bytes());
        assert!(slip("splt", "split"));
        assert!(slip("comit", "commit"));
        assert!(slip("serach", "search"));
        assert!(slip("binray", "binary"));
        assert!(slip("fussion", "fusion"));
        assert!(slip("ttree", "tree"));
        assert!(!slip("boot", "bool"));
        assert!(!slip("react", "rect"));
        assert!(!slip("diffs", "diff"));
    }

    #[test]
    fn correct_allows_substitution_only_for_long_words() {
        let v = vocab(&[("reciprocal", 1), ("cache", 1), ("chunking", 1)]);
        assert_eq!(
            correct(&query_words("reciprical cachx checking"), &v),
            pairs(&[("reciprical", "reciprocal")])
        );
    }

    #[test]
    fn correct_prefers_frequent_term_on_tie() {
        let v = vocab(&[("commit", 9), ("comet", 1)]);
        assert_eq!(
            correct(&query_words("comit"), &v),
            pairs(&[("comit", "commit")])
        );
    }

    #[test]
    fn append_and_replace() {
        let c = pairs(&[("serach", "search")]);
        assert_eq!(
            append_corrections("comit serach", &c),
            "comit serach search"
        );
        assert_eq!(replace_corrections("Serach, prefix", &c), "search, prefix");
        assert_eq!(replace_corrections("serachMode", &c), "serachMode");
    }
}
