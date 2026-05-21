use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use gluck::app::App;
use gluck::cli::Cli;
use gluck::debug;
use gluck::git::repo::GitRepo;
use std::path::PathBuf;

fn main() -> Result<()> {
    let cli = Cli::parse();

    debug::init_logging(&cli.log_level);

    let path = cli
        .path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let repo = GitRepo::open(&path)?;
    let mut app = App::new(repo)?;
    if cli.debug {
        app.debug_overlay = true;
    }

    let mut terminal = ratatui::init();
    let result = run_app(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_app(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| app.render(f))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if key.modifiers.is_empty() {
                    app.handle_key(key.code);
                } else {
                    app.handle_ctrl_key(key.code);
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
