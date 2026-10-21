//! Prompt templates. Written for cheap/free models: the output structure,
//! length limits and forbidden habits (preambles, reprinting code) are all
//! spelled out, and the template comes last in the user message because
//! small models follow the most recent instruction best.

use super::context::{AskContext, AskKind};
use super::ChatMessage;
use crate::config::AskConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    Brief,
    #[default]
    Normal,
    Deep,
}

impl Detail {
    pub fn max_tokens(self) -> u32 {
        match self {
            Detail::Brief => 500,
            Detail::Normal => 1200,
            Detail::Deep => 2500,
        }
    }

    fn limits(self) -> Limits {
        match self {
            Detail::Brief => Limits {
                sentences: 1,
                bullets: 2,
                idioms: 3,
                risks: 1,
            },
            Detail::Normal => Limits {
                sentences: 2,
                bullets: 3,
                idioms: 5,
                risks: 3,
            },
            Detail::Deep => Limits {
                sentences: 3,
                bullets: 5,
                idioms: 8,
                risks: 5,
            },
        }
    }
}

struct Limits {
    sentences: usize,
    bullets: usize,
    idioms: usize,
    risks: usize,
}

const SYSTEM_PROMPT: &str = "\
You are a senior software engineer helping a colleague read code in a language, \
library or codebase they may not know well. Explain clearly and concretely, like a \
patient code reviewer.

Rules:
- Write the whole answer in {language}. Keep code identifiers, keywords, API names and \
file paths unchanged, wrapped in backticks.
- Start directly with the first heading of the template. No greeting, preamble or \
closing remarks.
- Use only the template's Markdown headings, in order. Under each heading use short \
bullet points. No tables.
- Do not reprint the code. Quote at most one short fragment when pointing at \
something, and cite line numbers like L42.
- Base every statement on the code and commit message provided. When you infer \
intent, explicitly mark it as a guess.
- Never describe code that is not shown.";

/// Section headings, localized when the answer language is Korean so the
/// model isn't asked to translate its own scaffold.
struct Headings {
    summary: &'static str,
    intent: &'static str,
    flow: &'static str,
    idioms: &'static str,
    review: &'static str,
    what: &'static str,
    syntax: &'static str,
    why: &'static str,
    concepts: &'static str,
    none: &'static str,
}

const KO: Headings = Headings {
    summary: "요약",
    intent: "변경 의도",
    flow: "동작 흐름",
    idioms: "낯선 구문/이디엄",
    review: "리뷰 포인트",
    what: "무엇을 하나",
    syntax: "구문 해설",
    why: "왜 이렇게 썼나",
    concepts: "관련 개념",
    none: "없음",
};

const EN: Headings = Headings {
    summary: "Summary",
    intent: "Intent",
    flow: "How it works",
    idioms: "Unfamiliar syntax / idioms",
    review: "Review notes",
    what: "What it does",
    syntax: "Syntax walkthrough",
    why: "Why it is written this way",
    concepts: "Related concepts",
    none: "None",
};

fn headings(language: &str) -> &'static Headings {
    let l = language.trim().to_lowercase();
    if l.starts_with("korean") || l.starts_with("한국") || l == "ko" {
        &KO
    } else {
        &EN
    }
}

pub fn system_prompt(cfg: &AskConfig) -> String {
    let template = if cfg.system_prompt.trim().is_empty() {
        SYSTEM_PROMPT
    } else {
        cfg.system_prompt.as_str()
    };
    template.replace("{language}", &cfg.language)
}

fn template(ctx: &AskContext, h: &Headings, detail: Detail) -> String {
    let lim = detail.limits();
    let deep_idiom = if detail == Detail::Deep {
        " Also name a common alternative and why this form was chosen."
    } else {
        ""
    };
    let idioms = format!(
        "## {}\n- Syntax, idioms, standard-library or framework APIs in this code that someone new to the language may not know. One bullet each: `token` — what it means and why it is used here.{} At most {} bullets. Omit this whole section if there are none.",
        h.idioms, deep_idiom, lim.idioms
    );
    let review = format!(
        "## {}\n- Possible bugs, edge cases or risks visible in this code. At most {} bullets. Write \"{}\" if there are none.",
        h.review, lim.risks, h.none
    );
    match (ctx.kind, ctx.is_diff) {
        (AskKind::Page, true) => format!(
            "## {}\n- What this change does, in {} sentence(s).\n\n## {}\n- Why the code changed. Quote the commit message as evidence where it helps; mark guesses as guesses. At most {} bullets.\n\n{}\n\n{}",
            h.summary, lim.sentences, h.intent, lim.bullets, idioms, review
        ),
        (AskKind::Page, false) => format!(
            "## {}\n- What this code does, in {} sentence(s).\n\n## {}\n- The control/data flow through the shown code, step by step. At most {} bullets.\n\n{}\n\n{}",
            h.summary, lim.sentences, h.flow, lim.bullets, idioms, review
        ),
        (AskKind::Selection, _) => format!(
            "## {}\n- What the selected code does, in {} sentence(s).\n\n## {}\n- Break the selected code into its parts (expressions, operators, keywords, calls) and explain each one a newcomer may not know. At most {} bullets.{}\n\n## {}\n- Why it is written this way rather than a more obvious alternative. At most {} bullets.\n\n## {}\n- 2–3 concept names or search keywords to learn more, each with a one-line hint.",
            h.what, lim.sentences, h.syntax, lim.idioms, deep_idiom, h.why, lim.bullets, h.concepts
        ),
    }
}

fn user_prompt(ctx: &AskContext, cfg: &AskConfig, detail: Detail) -> String {
    let h = headings(&cfg.language);
    let lang = ctx.language.unwrap_or("text");
    let mut out = String::new();

    out.push_str(&format!("File: `{}` ({})", ctx.path, lang));
    if let Some(change) = &ctx.change {
        out.push_str(&format!(" — {change}"));
    }
    out.push_str(&format!(
        "\nCommit: {}\nCommit message:\n\"\"\"\n{}\n\"\"\"\n\n",
        ctx.commit_short,
        ctx.commit_message.trim()
    ));

    let format_note = if ctx.is_diff {
        "Format: one line per code line — marker (`+` added, `-` removed, space = unchanged), line number (new file for `+`/unchanged, old file for `-`), `|`, code."
    } else {
        "Format: one line per code line — line number, `|`, code."
    };
    let fence = if ctx.is_diff { "diff" } else { lang };
    let block = |label: &str, body: &str| -> String {
        if body.is_empty() {
            String::new()
        } else {
            format!("{label}:\n```{fence}\n{body}\n```\n\n")
        }
    };

    if !ctx.before.is_empty() || !ctx.after.is_empty() {
        out.push_str("Surrounding code is given only for context; explain the selected lines.\n\n");
    }
    out.push_str(&block("Context before (not selected)", &ctx.before));
    out.push_str(&format!("Below is {}.\n{}\n\n", ctx.scope, format_note));
    let mut code = ctx.code.clone();
    if ctx.truncated {
        code.push_str("\n… (truncated)");
    }
    out.push_str(&format!("```{fence}\n{code}\n```\n\n"));
    out.push_str(&block("Context after (not selected)", &ctx.after));

    out.push_str(&format!(
        "Answer in {} using exactly this template (replace the guidance under each heading with your answer):\n\n{}",
        cfg.language,
        template(ctx, h, detail)
    ));
    out
}

pub fn build_messages(ctx: &AskContext, cfg: &AskConfig, detail: Detail) -> Vec<ChatMessage> {
    vec![
        ChatMessage::system(system_prompt(cfg)),
        ChatMessage::user(user_prompt(ctx, cfg, detail)),
    ]
}

/// `[ask].max_tokens` overrides the detail-level default when non-zero.
pub fn max_tokens(cfg: &AskConfig, detail: Detail) -> u32 {
    if cfg.max_tokens > 0 {
        cfg.max_tokens
    } else {
        detail.max_tokens()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ask::context;
    use crate::git::commit::CommitInfo;
    use crate::git::diff::{DiffFile, DiffLine, FileChange};

    fn commit() -> CommitInfo {
        CommitInfo {
            id: git2::Oid::ZERO_SHA1,
            short_id: "abc1234".into(),
            author: "T".into(),
            date: std::time::UNIX_EPOCH,
            message: "Propagate errors".into(),
        }
    }

    fn diff_ctx() -> AskContext {
        let file = DiffFile {
            change: Some(FileChange::Added {
                path: "a.rs".into(),
            }),
            lines: vec![DiffLine::Added {
                line_no: 1,
                content: "let x = g()?;".into(),
            }],
        };
        context::diff_page(&commit(), &file, 0..1, 10_000)
    }

    #[test]
    fn test_diff_page_prompt_has_korean_sections_after_code() {
        let cfg = AskConfig::default();
        let msgs = build_messages(&diff_ctx(), &cfg, Detail::Normal);
        assert_eq!(msgs[0].role, "system");
        assert!(msgs[0].content.contains("Write the whole answer in Korean"));
        let user = &msgs[1].content;
        assert!(user.contains("```diff\n+    1 | let x = g()?;\n```"));
        assert!(user.contains("Propagate errors"));
        let code_at = user.find("```diff").unwrap();
        for h in [
            "## 요약",
            "## 변경 의도",
            "## 낯선 구문/이디엄",
            "## 리뷰 포인트",
        ] {
            let at = user.find(h).unwrap_or_else(|| panic!("missing {h}"));
            assert!(at > code_at, "{h} should follow the code");
        }
    }

    #[test]
    fn test_selection_prompt_sections_and_context_blocks() {
        let cfg = AskConfig::default();
        let ctx = context::file_selection(&commit(), "x.go", "a\nb\nc\n", 1..2, 1, 10_000);
        let user = &build_messages(&ctx, &cfg, Detail::Normal)[1].content;
        assert!(user.contains("## 무엇을 하나"));
        assert!(user.contains("## 구문 해설"));
        assert!(user.contains("Context before (not selected):\n```go\n   1 | a\n```"));
        assert!(user.contains("```go\n   2 | b\n```"));
        assert!(!user.contains("## 변경 의도"));
    }

    #[test]
    fn test_detail_changes_limits_and_tokens() {
        let cfg = AskConfig::default();
        let brief = &build_messages(&diff_ctx(), &cfg, Detail::Brief)[1].content;
        let deep = &build_messages(&diff_ctx(), &cfg, Detail::Deep)[1].content;
        assert!(brief.contains("At most 3 bullets"));
        assert!(deep.contains("At most 8 bullets"));
        assert!(deep.contains("common alternative"));
        assert_eq!(max_tokens(&cfg, Detail::Deep), 2500);
        let fixed = AskConfig {
            max_tokens: 900,
            ..Default::default()
        };
        assert_eq!(max_tokens(&fixed, Detail::Deep), 900);
    }

    #[test]
    fn test_english_headings_and_system_override() {
        let cfg = AskConfig {
            language: "English".into(),
            system_prompt: "Be terse. Reply in {language}.".into(),
            ..Default::default()
        };
        let msgs = build_messages(&diff_ctx(), &cfg, Detail::Normal);
        assert_eq!(msgs[0].content, "Be terse. Reply in English.");
        assert!(msgs[1].content.contains("## Summary"));
    }

    #[test]
    fn test_view_page_uses_flow_section() {
        let cfg = AskConfig::default();
        let ctx = context::file_page(&commit(), "x.rs", "fn a() {}\n", 0..1, false, 10_000);
        let user = &build_messages(&ctx, &cfg, Detail::Normal)[1].content;
        assert!(user.contains("## 동작 흐름"));
        assert!(user.contains("```rust\n   1 | fn a() {}\n```"));
    }
}
