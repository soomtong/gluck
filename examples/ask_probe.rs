//! Sends the Ask "page" prompt for one file of the HEAD commit's diff and
//! streams the answer — handy for comparing cheap models on the prompt.
//! `cargo run --example ask_probe -- [FILE_SUBSTRING] [--deep] [--dry]`
//! Uses `[ask]` from the config file, falling back to the env keys.

use gluck::ask::{client, context, prompt, provider, ChatRequest};
use gluck::config::Config;
use gluck::git::commit::CommitInfo;
use gluck::git::repo::GitRepo;
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dry = args.iter().any(|a| a == "--dry");
    let detail = if args.iter().any(|a| a == "--deep") {
        prompt::Detail::Deep
    } else {
        prompt::Detail::Normal
    };
    let filter = args.iter().find(|a| !a.starts_with("--"));

    let repo = GitRepo::open(std::path::Path::new(".")).expect("open repo");
    let head = repo.repository().head().unwrap().peel_to_commit().unwrap();
    let to = CommitInfo::from_git_commit(&head);
    let from = CommitInfo::from_git_commit(&head.parent(0).expect("HEAD has a parent"));
    let diff = gluck::git::diff::compute_diff(&repo, &from, &to).expect("diff");
    let file = diff
        .files
        .iter()
        .find(|f| {
            let path = f.change.as_ref().map(|c| c.path()).unwrap_or("");
            filter.is_none_or(|s| path.contains(s.as_str()))
        })
        .expect("no matching file in HEAD diff");

    let cfg = Config::load_or_default().ask;
    let first_lines: Vec<usize> = (0..60).collect();
    let ctx = context::diff_page(&to, file, &first_lines, cfg.max_context_chars);
    let messages = prompt::build_messages(&ctx, &cfg, detail);
    if dry {
        for m in &messages {
            println!("===== {} =====\n{}\n", m.role, m.content);
        }
        return;
    }

    let endpoint = match provider::resolve(&cfg) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{}", provider::setup_hint(&e));
            std::process::exit(1);
        }
    };
    eprintln!("→ {} · {}", endpoint.label(), ctx.title());
    let request = ChatRequest {
        messages,
        max_tokens: prompt::max_tokens(&cfg, detail),
        temperature: (cfg.temperature >= 0.0).then_some(cfg.temperature),
    };
    let started = std::time::Instant::now();
    let result = client::stream_chat(&endpoint, &request, |d| {
        print!("{d}");
        let _ = std::io::stdout().flush();
        true
    });
    println!();
    match result {
        Ok(end) => eprintln!(
            "← done in {:.1?} (truncated: {})",
            started.elapsed(),
            end.truncated
        ),
        Err(e) => {
            eprintln!("← error: {e}");
            std::process::exit(1);
        }
    }
}
