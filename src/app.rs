use crate::git::commit::list_commits;
use crate::git::diff::compute_diff;
use crate::git::repo::GitRepo;
use crate::git::tree::{list_tree, read_blob};
use crate::highlight::HighlightEngine;
use crate::mode::{
    Action, DiffState, KeyBindings, Mode, PickState, ViewState,
};
use crate::ui;
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::Frame;

pub struct App {
    pub mode: Mode,
    pub repo: GitRepo,
    pub keybindings: KeyBindings,
    pub should_quit: bool,
    pub searching: bool,
    pub search_input: String,
    pub debug_overlay: bool,
    pub highlight: HighlightEngine,
}

impl App {
    pub fn new(repo: GitRepo) -> Result<Self> {
        let commits = list_commits(&repo)?;
        let pick_state = PickState::new(commits);
        Ok(Self {
            mode: Mode::Pick(pick_state),
            repo,
            keybindings: KeyBindings::default_bindings(),
            should_quit: false,
            searching: false,
            search_input: String::new(),
            debug_overlay: false,
            highlight: HighlightEngine::new(),
        })
    }

    pub fn render(&self, frame: &mut Frame) {
        match &self.mode {
            Mode::Pick(_) => ui::pick::render_pick(frame, frame.area(), self),
            Mode::View(_) => ui::view::render_view(frame, frame.area(), self),
            Mode::Diff(_) => ui::diff::render_diff(frame, frame.area(), self),
        }

        if self.debug_overlay {
            self.render_debug_overlay(frame);
        }
    }

    fn render_debug_overlay(&self, frame: &mut Frame) {
        use ratatui::layout::Rect;
        use ratatui::style::{Style, Stylize};
        use ratatui::widgets::Paragraph;

        let mode_name = match &self.mode {
            Mode::Pick(_) => "Pick",
            Mode::View(_) => "View",
            Mode::Diff(_) => "Diff",
        };

        let info = match &self.mode {
            Mode::Pick(s) => format!(
                "Mode: {} | Selected: {} | Commits: {} | Filtered: {}",
                mode_name, s.selected, s.commits.len(), s.filtered_indices.len(),
            ),
            Mode::View(s) => format!(
                "Mode: {} | File: {} | Files: {} | Scroll: {}",
                mode_name, s.selected_file, s.tree.len(), s.scroll,
            ),
            Mode::Diff(s) => format!(
                "Mode: {} | File: {} | Files: {} | Side-by-side: {}",
                mode_name, s.selected_file, s.diff_result.files.len(), s.side_by_side,
            ),
        };

        let area = Rect::new(frame.area().width.saturating_sub(50), 0, 50, 1);
        let debug = Paragraph::new(info).style(Style::new().on_dark_gray().yellow());
        frame.render_widget(debug, area);
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        if self.searching {
            self.handle_search_input(code);
            return;
        }

        let Some(action) = self.keybindings.resolve(code) else {
            return;
        };
        match action {
            Action::Quit => self.should_quit = true,
            Action::Search => self.start_search(),
            Action::MoveDown => self.move_down(),
            Action::MoveUp => self.move_up(),
            Action::Enter => self.enter(),
            Action::Back => self.back(),
            Action::ToggleView => self.toggle_view(),
            Action::SwitchMode => self.switch_mode(),
        }
    }

    pub fn handle_ctrl_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('c') => self.should_quit = true,
            KeyCode::Char('d') => self.debug_overlay = !self.debug_overlay,
            _ => {}
        }
    }

    fn start_search(&mut self) {
        if let Mode::Pick(_) = &self.mode {
            self.searching = true;
            self.search_input.clear();
        }
    }

    fn handle_search_input(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.searching = false;
                self.search_input.clear();
            }
            KeyCode::Enter => {
                self.searching = false;
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                self.apply_search();
            }
            KeyCode::Char(c) => {
                self.search_input.push(c);
                self.apply_search();
            }
            _ => {}
        }
    }

    fn apply_search(&mut self) {
        if let Mode::Pick(state) = &mut self.mode {
            state.apply_search(&self.search_input);
        }
    }

    fn move_down(&mut self) {
        match &mut self.mode {
            Mode::Pick(state) => {
                let max = state.filtered_indices.len().saturating_sub(1);
                state.selected = state.selected.saturating_add(1).min(max);
            }
            Mode::View(state) => {
                let max = state.tree.len().saturating_sub(1);
                state.selected_file = state.selected_file.saturating_add(1).min(max);
            }
            Mode::Diff(state) => {
                let max = state.diff_result.files.len().saturating_sub(1);
                state.selected_file = state.selected_file.saturating_add(1).min(max);
            }
        }
    }

    fn move_up(&mut self) {
        match &mut self.mode {
            Mode::Pick(state) => {
                state.selected = state.selected.saturating_sub(1);
            }
            Mode::View(state) => {
                state.selected_file = state.selected_file.saturating_sub(1);
            }
            Mode::Diff(state) => {
                state.selected_file = state.selected_file.saturating_sub(1);
            }
        }
    }

    fn enter(&mut self) {
        match &self.mode {
            Mode::Pick(state) => {
                if let Some(&idx) = state.filtered_indices.get(state.selected) {
                    let commit = state.commits[idx].clone();
                    let tree = list_tree(&self.repo, &commit).unwrap_or_default();
                    let view_state = ViewState::new(commit, tree);
                    self.mode = Mode::View(view_state);
                }
            }
            Mode::View(state) => {
                if let Some(entry) = state.tree.get(state.selected_file) {
                    if let Ok(content) = read_blob(&self.repo, &state.commit, &entry.path) {
                        let highlighted = self.highlight.highlight(&content, &entry.path);
                        if let Mode::View(vs) = &mut self.mode {
                            vs.content = Some(content);
                            vs.highlighted = highlighted;
                        }
                    }
                }
            }
            Mode::Diff(_) => {}
        }
    }

    fn back(&mut self) {
        match &self.mode {
            Mode::View(_) | Mode::Diff(_) => {
                let commits = list_commits(&self.repo).unwrap_or_default();
                let mut pick = PickState::new(commits);
                if let Mode::View(vs) = &self.mode {
                    pick.selected = pick
                        .commits
                        .iter()
                        .position(|c| c.id == vs.commit.id)
                        .unwrap_or(0);
                }
                self.mode = Mode::Pick(pick);
            }
            Mode::Pick(_) => {}
        }
    }

    fn switch_mode(&mut self) {
        match &self.mode {
            Mode::View(state) => {
                let commits = list_commits(&self.repo).unwrap_or_default();
                let current_idx = commits.iter().position(|c| c.id == state.commit.id);
                if let Some(idx) = current_idx {
                    if idx + 1 < commits.len() {
                        let from = commits[idx + 1].clone();
                        let to = commits[idx].clone();
                        if let Ok(diff_result) = compute_diff(&self.repo, &from, &to) {
                            let diff_state = DiffState::new(from, to, diff_result);
                            self.mode = Mode::Diff(diff_state);
                        }
                    }
                }
            }
            Mode::Diff(state) => {
                let commits = list_commits(&self.repo).unwrap_or_default();
                if let Some(idx) = commits.iter().position(|c| c.id == state.to.id) {
                    let commit = commits[idx].clone();
                    let tree = list_tree(&self.repo, &commit).unwrap_or_default();
                    let view_state = ViewState::new(commit, tree);
                    self.mode = Mode::View(view_state);
                }
            }
            Mode::Pick(_) => {}
        }
    }

    fn toggle_view(&mut self) {
        if let Mode::Diff(state) = &mut self.mode {
            state.side_by_side = !state.side_by_side;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::tests::{add_file_commit, init_test_repo};

    fn test_app() -> (tempfile::TempDir, App) {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First commit");
        add_file_commit(&repo, "b.txt", b"second", "Second commit");
        add_file_commit(&repo, "a.txt", b"third", "Third commit");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let app = App::new(git_repo).unwrap();
        (dir, app)
    }

    #[test]
    fn test_app_starts_in_pick_mode() {
        let (_dir, app) = test_app();
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    #[test]
    fn test_pick_to_view() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));
    }

    #[test]
    fn test_view_to_pick() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));
        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    #[test]
    fn test_view_to_diff_to_pick() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Tab);
        assert!(matches!(app.mode, Mode::Diff(_)));
        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    #[test]
    fn test_quit() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn test_move_selection() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('j'));
        if let Mode::Pick(state) = &app.mode {
            assert_eq!(state.selected, 1);
        }
        app.handle_key(KeyCode::Char('k'));
        if let Mode::Pick(state) = &app.mode {
            assert_eq!(state.selected, 0);
        }
    }

    #[test]
    fn test_search_mode() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('/'));
        assert!(app.searching);
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('h'));
        app.handle_key(KeyCode::Enter);
        assert!(!app.searching);
    }
}