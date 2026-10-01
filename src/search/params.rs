//! Fusion knobs for `SearchEngine::search`.
//!
//! Defaults reproduce the shipped behaviour. `glc report --param key=value`
//! overrides them so tuning experiments don't need a rebuild.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct SearchParams {
    /// RRF rank constant.
    pub rrf_k: f32,
    /// Vector weight in RRF for non-Korean queries (BM25 weight is 1.0).
    pub w_vec: f32,
    /// Vector weight in RRF for Korean queries.
    pub w_vec_korean: f32,
    /// BM25 top-N pinned ahead of fusion for Korean queries.
    pub korean_anchor: usize,
    /// Vector hits below this cosine similarity are dropped before fusion.
    pub vec_min_score: f32,
    /// Subtracted from Commit docs' vector score before re-ranking the vector
    /// list. Commit messages are short and generic, so they crowd the vector
    /// top-k for code queries.
    pub vec_commit_penalty: f32,
    /// Typo correction against the BM25 word vocabulary (non-Korean queries).
    /// 0 off, 1 append corrections to the BM25 query (default), 2 also append
    /// them to the embedded text, 3 replace typos in the embedded text.
    pub typo_mode: usize,
    /// `SearchOutcome::weak` fires when no query word hits title/path_terms and
    /// the vector top-1 exceeds the top-10 mean by less than this. 0 disables.
    /// 0.055 sits between the last flagged negative (0.049) and the first
    /// answered positive without word hits (0.061) on the fixture set.
    pub weak_gap: f32,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            rrf_k: 60.0,
            w_vec: 1.0,
            w_vec_korean: 1.5,
            korean_anchor: 3,
            vec_min_score: f32::NEG_INFINITY,
            vec_commit_penalty: 0.0,
            typo_mode: 1,
            weak_gap: 0.055,
        }
    }
}

impl SearchParams {
    /// Apply one `key=value` override.
    pub fn set(&mut self, assignment: &str) -> Result<(), String> {
        let (key, value) = assignment
            .split_once('=')
            .ok_or_else(|| format!("expected key=value, got '{assignment}'"))?;
        let f = || {
            value
                .trim()
                .parse::<f32>()
                .map_err(|e| format!("{key}: {e}"))
        };
        match key.trim() {
            "rrf_k" => self.rrf_k = f()?,
            "w_vec" => self.w_vec = f()?,
            "w_vec_korean" => self.w_vec_korean = f()?,
            "korean_anchor" => {
                self.korean_anchor = value.trim().parse().map_err(|e| format!("{key}: {e}"))?
            }
            "vec_min_score" => self.vec_min_score = f()?,
            "vec_commit_penalty" => self.vec_commit_penalty = f()?,
            "weak_gap" => self.weak_gap = f()?,
            "typo_mode" => {
                self.typo_mode = value.trim().parse().map_err(|e| format!("{key}: {e}"))?
            }
            other => return Err(format!("unknown search param '{other}'")),
        }
        Ok(())
    }
}

impl fmt::Display for SearchParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rrf_k={} w_vec={} w_vec_korean={} korean_anchor={} vec_min_score={} vec_commit_penalty={} typo_mode={} weak_gap={}",
            self.rrf_k,
            self.w_vec,
            self.w_vec_korean,
            self.korean_anchor,
            if self.vec_min_score.is_finite() {
                self.vec_min_score.to_string()
            } else {
                "off".into()
            },
            self.vec_commit_penalty,
            self.typo_mode,
            self.weak_gap
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_overrides_known_keys() {
        let mut p = SearchParams::default();
        p.set("rrf_k=20").unwrap();
        p.set(" vec_min_score = 0.3 ").unwrap();
        p.set("korean_anchor=0").unwrap();
        assert_eq!(p.rrf_k, 20.0);
        assert_eq!(p.vec_min_score, 0.3);
        assert_eq!(p.korean_anchor, 0);
    }

    #[test]
    fn set_rejects_bad_input() {
        let mut p = SearchParams::default();
        assert!(p.set("nope=1").is_err());
        assert!(p.set("rrf_k").is_err());
        assert!(p.set("rrf_k=abc").is_err());
        assert_eq!(p, SearchParams::default());
    }
}
