use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind, KeyModifiers};
use gluck::app::App;
use gluck::cli::{Cli, Commands};
use gluck::config::Config;
use gluck::debug;
use gluck::git::repo::GitRepo;
use std::path::PathBuf;
use std::time::Duration;

fn main() -> Result<()> {
    let cli = Cli::parse();

    debug::init_logging(&cli.log_level);

    let path = cli
        .path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let repo = match GitRepo::open(&path) {
        Ok(r) => r,
        Err(_) => {
            eprintln!("fatal: not a git repository: {}", path.display());
            std::process::exit(1);
        }
    };

    match cli.command {
        Some(Commands::Index {
            force,
            batch_size,
            max_file_bytes,
        }) => {
            let opts = gluck::search::indexer::IndexOptions {
                force,
                batch_size,
                max_file_bytes,
            };
            gluck::search::indexer::build_index(&repo, &path, &opts, |msg| eprintln!("{}", msg))
                .map_err(|e| anyhow::anyhow!("index error: {}", e))?;
            return Ok(());
        }
        Some(Commands::Report {
            fixtures,
            out,
            warmup,
            iters,
            limit,
        }) => {
            let opts = gluck::search::report::ReportOptions {
                fixtures_path: PathBuf::from(fixtures),
                out_markdown: out.map(PathBuf::from),
                warmup,
                iters,
                limit,
            };
            match gluck::search::report::run(&repo, &path, &opts) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    eprintln!("report error: {}", e);
                    if matches!(
                        e,
                        gluck::search::report::ReportError::Search(
                            gluck::search::SearchError::IndexNotFound(_)
                        )
                    ) {
                        eprintln!("hint: run `glc index` first");
                    }
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Diagnose { query, limit }) => {
            if let Err(e) = gluck::search::diagnose::run(&path, &query, limit) {
                eprintln!("diagnose error: {}", e);
                if matches!(e, gluck::search::SearchError::IndexNotFound(_)) {
                    eprintln!("hint: run `glc index` first");
                }
                std::process::exit(1);
            }
            return Ok(());
        }
        None => {}
    }

    let config = Config::load().unwrap_or_default();
    let mut app = App::new(repo, config)?;
    if cli.debug {
        app.debug_overlay = true;
    }

    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_app(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        if app.needs_clear {
            terminal.clear()?;
            app.needs_clear = false;
        }
        terminal.draw(|f| app.render(f))?;

        if app.is_indexing() {
            app.drain_index_messages();
            app.drain_engine_messages();
            app.drain_search_results();
            if event::poll(Duration::from_millis(80))? {
                read_and_dispatch(app)?;
            }
        } else {
            app.drain_search_results();
            read_and_dispatch(app)?;
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn read_and_dispatch(app: &mut App) -> Result<()> {
    if let Event::Key(key) = event::read()? {
        if key.kind == KeyEventKind::Press {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                app.handle_ctrl_key(key.code);
            } else {
                app.handle_key(key.code);
            }
        }
    }
    Ok(())
}
