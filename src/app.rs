use crate::config::Config;
use crate::git::commit::{list_commits, CommitInfo};
use crate::git::diff::compute_diff;
use crate::git::repo::GitRepo;
use crate::git::tree::{is_binary_blob, list_tree, read_blob, EntryKind};
use crate::highlight::HighlightEngine;
use crate::mode::{Action, DiffState, KeyBindings, Mode, PickState, SearchState, ViewState};
use crate::theme::Palette;
use crate::ui;
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::Frame;
use std::collections::HashMap;

pub struct App {
    pub mode: Mode,
    pub repo: GitRepo,
    pub commits: Vec<CommitInfo>,
    pub keybindings: KeyBindings,
    pub should_quit: bool,
    pub debug_overlay: bool,
    pub highlight: HighlightEngine,
    pub palette: Palette,
    pub theme_name: String,
    pub config: Config,
    pub saved_search: SearchState,
}

impl App {
    pub fn new(repo: GitRepo, config: Config) -> Result<Self> {
        let commits = list_commits(&repo)?;
        let pick_state = PickState::new(commits.clone());
        let theme_name = config.theme.name.clone();
        let palette = crate::theme::resolve_palette(Some(&theme_name));
        let mut app = Self {
            mode: Mode::Pick(pick_state),
            repo,
            commits,
            keybindings: KeyBindings::default_bindings(),
            should_quit: false,
            debug_overlay: false,
            highlight: HighlightEngine::new(),
            palette,
            theme_name,
            config,
            saved_search: SearchState::Idle { query: None },
        };
        app.highlight.set_theme(app.palette.to_highlight_map());
        app.update_pick_diff();
        Ok(app)
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
                mode_name,
                s.selected,
                s.commits.len(),
                s.filtered_indices.len(),
            ),
            Mode::View(s) => format!(
                "Mode: {} | File: {} | Files: {} | Scroll: {}",
                mode_name,
                s.selected_file,
                s.tree.len(),
                s.scroll,
            ),
            Mode::Diff(s) => format!(
                "Mode: {} | File: {} | Files: {} | Side-by-side: {}",
                mode_name,
                s.selected_file,
                s.diff_result.files.len(),
                s.side_by_side,
            ),
        };

        let area = Rect::new(frame.area().width.saturating_sub(50), 0, 50, 1);
        let debug = Paragraph::new(info).style(Style::new().on_dark_gray().yellow());
        frame.render_widget(debug, area);
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        let is_searching = matches!(&self.mode, Mode::Pick(p) if matches!(p.search, crate::mode::SearchState::Active { .. }));
        if is_searching {
            self.handle_search_input(code);
            return;
        }

        if code == KeyCode::Esc {
            if let Mode::Pick(state) = &mut self.mode {
                if let SearchState::Idle { query: Some(_) } = &state.search {
                    state.search = SearchState::Idle { query: None };
                    state.update_filter("");
                    self.saved_search = SearchState::Idle { query: None };
                    self.update_pick_diff();
                    return;
                }
            }
        }

        // Diff mode: h/l and arrow keys navigate files
        if matches!(self.mode, Mode::Diff(_)) {
            match code {
                KeyCode::Char('h') | KeyCode::Left => {
                    self.move_up();
                    return;
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    self.move_down();
                    return;
                }
                _ => {}
            }
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
            Action::PageDown => self.page_down(),
            Action::PageUp => self.page_up(),
            Action::ToggleGitignore => self.toggle_gitignore(),
            Action::ScrollDown => self.scroll_down(),
            Action::ScrollUp => self.scroll_up(),
        }
    }

    pub fn handle_ctrl_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('c') => self.should_quit = true,
            KeyCode::Char('d') => self.debug_overlay = !self.debug_overlay,
            KeyCode::Char('p') => self.prev_commit(),
            KeyCode::Char('n') => self.next_commit(),
            KeyCode::Char('t') => self.next_theme(),
            _ => {}
        }
    }

    fn start_search(&mut self) {
        if let Mode::Pick(state) = &mut self.mode {
            state.search = crate::mode::SearchState::Active {
                input: String::new(),
            };
        }
    }

    fn handle_search_input(&mut self, code: KeyCode) {
        use crate::mode::SearchState;
        let query = {
            let Mode::Pick(state) = &mut self.mode else {
                return;
            };
            match code {
                KeyCode::Esc | KeyCode::Enter => {
                    let query = match &state.search {
                        SearchState::Active { input } if !input.is_empty() => Some(input.clone()),
                        _ => None,
                    };
                    state.search = SearchState::Idle { query };
                    None
                }
                KeyCode::Backspace => {
                    if let SearchState::Active { input } = &mut state.search {
                        input.pop();
                        Some(input.clone())
                    } else {
                        None
                    }
                }
                KeyCode::Char(c) => {
                    if let SearchState::Active { input } = &mut state.search {
                        input.push(c);
                        Some(input.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };
        if let Some(q) = query {
            if let Mode::Pick(state) = &mut self.mode {
                state.update_filter(&q);
            }
        }
        self.update_pick_diff();
    }

    fn move_down(&mut self) {
        match &mut self.mode {
            Mode::Pick(state) => {
                let max = state.filtered_indices.len().saturating_sub(1);
                state.selected = state.selected.saturating_add(1).min(max);
                self.update_pick_diff();
            }
            Mode::View(state) => {
                let max = state.tree.len().saturating_sub(1);
                state.selected_file = state.selected_file.saturating_add(1).min(max);
                self.load_view_file();
            }
            Mode::Diff(state) => {
                let max = state.diff_result.files.len().saturating_sub(1);
                let prev = state.selected_file;
                state.selected_file = state.selected_file.saturating_add(1).min(max);
                if state.selected_file != prev {
                    state.scroll = 0;
                }
            }
        }
    }

    fn move_up(&mut self) {
        match &mut self.mode {
            Mode::Pick(state) => {
                state.selected = state.selected.saturating_sub(1);
                self.update_pick_diff();
            }
            Mode::View(state) => {
                state.selected_file = state.selected_file.saturating_sub(1);
                self.load_view_file();
            }
            Mode::Diff(state) => {
                let prev = state.selected_file;
                state.selected_file = state.selected_file.saturating_sub(1);
                if state.selected_file != prev {
                    state.scroll = 0;
                }
            }
        }
    }

    fn enter(&mut self) {
        match &self.mode {
            Mode::Pick(state) => {
                self.saved_search = state.search.clone();
                if let Some(&idx) = state.filtered_indices.get(state.selected) {
                    let commit = state.commits[idx].clone();
                    self.mode = Mode::View(self.make_view_state(commit));
                    self.load_view_file();
                }
            }
            Mode::View(_) => {
                self.load_view_file();
            }
            Mode::Diff(_) => {}
        }
    }

    fn back(&mut self) {
        match &self.mode {
            Mode::View(_) | Mode::Diff(_) => {
                let target_id = if let Mode::View(vs) = &self.mode {
                    Some(vs.commit.id)
                } else if let Mode::Diff(ds) = &self.mode {
                    Some(ds.to.id)
                } else {
                    None
                };

                let mut pick = PickState::new(self.commits.clone());

                if let SearchState::Idle { query: Some(q) } = &self.saved_search {
                    pick.search = SearchState::Idle {
                        query: Some(q.clone()),
                    };
                    pick.update_filter(q);
                }

                if let Some(id) = target_id {
                    if let Some(full_idx) = pick.commits.iter().position(|c| c.id == id) {
                        pick.selected = pick
                            .filtered_indices
                            .iter()
                            .position(|&i| i == full_idx)
                            .unwrap_or(0);
                    }
                }

                self.mode = Mode::Pick(pick);
            }
            Mode::Pick(_) => {}
        }
    }

    fn switch_mode(&mut self) {
        match &self.mode {
            Mode::View(state) => {
                let current_idx = self.commits.iter().position(|c| c.id == state.commit.id);
                if let Some(idx) = current_idx {
                    if idx + 1 < self.commits.len() {
                        let from = self.commits[idx + 1].clone();
                        let to = self.commits[idx].clone();
                        let prev = state.selected_file;
                        if let Ok(diff_result) = compute_diff(&self.repo, &from, &to) {
                            let mut diff_state = DiffState::new(from, to, diff_result);
                            diff_state.prev_view_file = prev;
                            self.mode = Mode::Diff(diff_state);
                        }
                    }
                }
            }
            Mode::Diff(state) => {
                let prev = state.prev_view_file;
                if let Some(idx) = self.commits.iter().position(|c| c.id == state.to.id) {
                    let commit = self.commits[idx].clone();
                    let mut view_state = self.make_view_state(commit);
                    view_state.selected_file = prev.min(view_state.tree.len().saturating_sub(1));
                    self.mode = Mode::View(view_state);
                    self.load_view_file();
                }
            }
            Mode::Pick(_) => {}
        }
    }

    fn next_commit(&mut self) {
        match &self.mode {
            Mode::View(s) => {
                let Some(idx) = self.commits.iter().position(|c| c.id == s.commit.id) else {
                    return;
                };
                if idx == 0 {
                    return;
                }
                let prev_path = self.current_view_file_path();
                let commit = self.commits[idx - 1].clone();
                let mut state = self.make_view_state(commit);
                restore_file_selection(&mut state, prev_path);
                self.mode = Mode::View(state);
                self.load_view_file();
            }
            Mode::Diff(s) => {
                let Some(idx) = self.commits.iter().position(|c| c.id == s.to.id) else {
                    return;
                };
                if idx == 0 {
                    return;
                }
                let prev_file = s.selected_file;
                let prev_file_path = s
                    .diff_result
                    .files
                    .get(s.selected_file)
                    .and_then(|f| f.change.as_ref().map(|c| c.path()))
                    .map(|p| p.to_string());
                let from = self.commits[idx].clone();
                let to = self.commits[idx - 1].clone();
                if let Ok(diff_result) = compute_diff(&self.repo, &from, &to) {
                    let mut state = DiffState::new(from, to, diff_result);
                    state.prev_view_file = prev_file;
                    if let Some(ref path) = prev_file_path {
                        if let Some(pos) = state.diff_result.files.iter().position(|f| {
                            f.change.as_ref().is_some_and(|c| {
                                c.new_path() == Some(path.as_str())
                                    || c.old_path() == Some(path.as_str())
                            })
                        }) {
                            state.selected_file = pos;
                        }
                    }
                    self.mode = Mode::Diff(state);
                }
            }
            _ => {}
        }
    }

    fn prev_commit(&mut self) {
        match &self.mode {
            Mode::View(s) => {
                let Some(idx) = self.commits.iter().position(|c| c.id == s.commit.id) else {
                    return;
                };
                if idx + 1 >= self.commits.len() {
                    return;
                }
                let prev_path = self.current_view_file_path();
                let commit = self.commits[idx + 1].clone();
                let mut state = self.make_view_state(commit);
                restore_file_selection(&mut state, prev_path);
                self.mode = Mode::View(state);
                self.load_view_file();
            }
            Mode::Diff(s) => {
                let Some(idx) = self.commits.iter().position(|c| c.id == s.to.id) else {
                    return;
                };
                if idx + 2 >= self.commits.len() {
                    return;
                }
                let prev_file = s.selected_file;
                let prev_file_path = s
                    .diff_result
                    .files
                    .get(s.selected_file)
                    .and_then(|f| f.change.as_ref().map(|c| c.path()))
                    .map(|p| p.to_string());
                let from = self.commits[idx + 2].clone();
                let to = self.commits[idx + 1].clone();
                if let Ok(diff_result) = compute_diff(&self.repo, &from, &to) {
                    let mut state = DiffState::new(from, to, diff_result);
                    state.prev_view_file = prev_file;
                    if let Some(ref path) = prev_file_path {
                        if let Some(pos) = state.diff_result.files.iter().position(|f| {
                            f.change.as_ref().is_some_and(|c| {
                                c.new_path() == Some(path.as_str())
                                    || c.old_path() == Some(path.as_str())
                            })
                        }) {
                            state.selected_file = pos;
                        }
                    }
                    self.mode = Mode::Diff(state);
                }
            }
            _ => {}
        }
    }

    fn current_view_file_path(&self) -> Option<String> {
        match &self.mode {
            Mode::View(s) => s.tree.get(s.selected_file).map(|e| e.path.clone()),
            _ => None,
        }
    }

    fn page_down(&mut self) {
        match &mut self.mode {
            Mode::View(state) => {
                let max_scroll = state.line_count().saturating_sub(1);
                state.scroll = (state.scroll + 20).min(max_scroll);
            }
            Mode::Diff(state) => {
                let line_count = state
                    .diff_result
                    .files
                    .get(state.selected_file)
                    .map(|f| f.lines.len())
                    .unwrap_or(0);
                let max_scroll = line_count.saturating_sub(1);
                state.scroll = (state.scroll + 20).min(max_scroll);
            }
            _ => {}
        }
    }

    fn page_up(&mut self) {
        match &mut self.mode {
            Mode::View(state) => {
                state.scroll = state.scroll.saturating_sub(20);
            }
            Mode::Diff(state) => {
                state.scroll = state.scroll.saturating_sub(20);
            }
            _ => {}
        }
    }

    fn scroll_down(&mut self) {
        let n = self.config.ui.scroll_lines;
        match &mut self.mode {
            Mode::View(state) => {
                let max_scroll = state.line_count().saturating_sub(1);
                state.scroll = (state.scroll + n).min(max_scroll);
            }
            Mode::Diff(state) => {
                let line_count = state
                    .diff_result
                    .files
                    .get(state.selected_file)
                    .map(|f| f.lines.len())
                    .unwrap_or(0);
                let max_scroll = line_count.saturating_sub(1);
                state.scroll = (state.scroll + n).min(max_scroll);
            }
            _ => {}
        }
    }

    fn scroll_up(&mut self) {
        let n = self.config.ui.scroll_lines;
        match &mut self.mode {
            Mode::View(state) => {
                state.scroll = state.scroll.saturating_sub(n);
            }
            Mode::Diff(state) => {
                state.scroll = state.scroll.saturating_sub(n);
            }
            _ => {}
        }
    }

    fn toggle_gitignore(&mut self) {
        if let Mode::View(state) = &mut self.mode {
            let prev_path = state.tree.get(state.selected_file).map(|e| e.path.clone());
            state.show_ignored = !state.show_ignored;
            let full_tree = list_tree(&self.repo, &state.commit).unwrap_or_default();
            if state.show_ignored {
                state.tree = full_tree;
            } else {
                let repo = self.repo.repository();
                state.tree = full_tree
                    .into_iter()
                    .filter(|e| !repo.is_path_ignored(&e.path).unwrap_or(false))
                    .collect();
            }
            state.selected_file = prev_path
                .and_then(|p| state.tree.iter().position(|e| e.path == p))
                .unwrap_or(0);
            state.file_content = crate::mode::FileContent::NotLoaded;
            self.load_view_file();
        }
    }

    fn toggle_view(&mut self) {
        if let Mode::Diff(state) = &mut self.mode {
            state.side_by_side = !state.side_by_side;
        }
    }

    fn compute_changed_stats(&self, commit: &CommitInfo) -> HashMap<String, (usize, usize)> {
        let repository = self.repo.repository();
        let Ok(commit_obj) = repository.find_commit(commit.id) else {
            return HashMap::new();
        };
        let parent = match commit_obj.parent(0) {
            Ok(p) => p,
            Err(_) => return HashMap::new(),
        };
        let parent_info = CommitInfo::from_git_commit(&parent);
        compute_diff(&self.repo, &parent_info, commit)
            .map(|r| {
                r.files
                    .iter()
                    .filter_map(|f| {
                        let path = f.change.as_ref().map(|c| c.path().to_string())?;
                        let added = f
                            .lines
                            .iter()
                            .filter(|l| matches!(l, crate::git::diff::DiffLine::Added { .. }))
                            .count();
                        let removed = f
                            .lines
                            .iter()
                            .filter(|l| matches!(l, crate::git::diff::DiffLine::Removed { .. }))
                            .count();
                        Some((path, (added, removed)))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn make_view_state(&self, commit: CommitInfo) -> ViewState {
        let tree = list_tree(&self.repo, &commit).unwrap_or_default();
        let changed_stats = self.compute_changed_stats(&commit);
        let changed_paths = changed_stats.keys().cloned().collect();
        ViewState {
            commit,
            tree,
            selected_file: 0,
            file_content: crate::mode::FileContent::NotLoaded,
            scroll: 0,
            show_ignored: true,
            changed_paths,
            changed_stats,
        }
    }

    fn update_pick_diff(&mut self) {
        if let Mode::Pick(state) = &mut self.mode {
            state.selected_diff = None;
            let Some(&idx) = state.filtered_indices.get(state.selected) else {
                return;
            };
            let commit = &state.commits[idx];
            let repository = self.repo.repository();
            let Ok(commit_obj) = repository.find_commit(commit.id) else {
                return;
            };
            let parent = match commit_obj.parent(0) {
                Ok(p) => p,
                Err(_) => return,
            };
            let parent_info = CommitInfo::from_git_commit(&parent);
            if let Ok(diff) = compute_diff(&self.repo, &parent_info, commit) {
                state.selected_diff = Some(diff);
            }
        }
    }

    fn load_view_file(&mut self) {
        let to_load = match &self.mode {
            Mode::View(state) => state
                .tree
                .get(state.selected_file)
                .filter(|e| matches!(e.kind, EntryKind::File))
                .map(|e| (e.path.clone(), state.commit.clone())),
            _ => None,
        };

        if let Mode::View(vs) = &mut self.mode {
            vs.scroll = 0;
        }

        let Some((path, commit)) = to_load else {
            if let Mode::View(vs) = &mut self.mode {
                vs.file_content = crate::mode::FileContent::NotLoaded;
            }
            return;
        };

        let binary = is_binary_blob(&self.repo, &commit, &path).unwrap_or(false);
        if binary {
            if let Mode::View(vs) = &mut self.mode {
                vs.file_content = crate::mode::FileContent::Binary;
            }
        } else if let Ok(content) = read_blob(&self.repo, &commit, &path) {
            let highlighted = self.highlight.highlight(&content, &path);
            if let Mode::View(vs) = &mut self.mode {
                vs.file_content = crate::mode::FileContent::Text {
                    raw: content,
                    highlighted,
                };
            }
        }
    }

    fn next_theme(&mut self) {
        let names: Vec<&str> = crate::theme::THEMES.iter().map(|(n, _)| *n).collect();
        let current_idx = names
            .iter()
            .position(|&n| n == self.theme_name)
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % names.len();
        self.theme_name = names[next_idx].to_string();
        self.palette = crate::theme::resolve_palette(Some(&self.theme_name));
        self.highlight.set_theme(self.palette.to_highlight_map());
        self.config.theme.name = self.theme_name.clone();
        let _ = self.config.save();
    }
}

fn restore_file_selection(state: &mut ViewState, prev_path: Option<String>) {
    let Some(path) = prev_path else {
        return;
    };
    if let Some(idx) = state.tree.iter().position(|e| e.path == path) {
        state.selected_file = idx;
        return;
    }
    let mut parent = path.as_str();
    while let Some(pos) = parent.rfind('/') {
        parent = &path[..pos];
        let prefix = format!("{}/", parent);
        if let Some(idx) = state.tree.iter().position(|e| e.path.starts_with(&prefix)) {
            state.selected_file = idx;
            return;
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
        let app = App::new(git_repo, Config::default()).unwrap();
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
        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert!(matches!(
            state.search,
            crate::mode::SearchState::Active { .. }
        ));
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('h'));
        app.handle_key(KeyCode::Enter);
        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert!(matches!(
            state.search,
            crate::mode::SearchState::Idle { .. }
        ));
    }

    #[test]
    fn test_view_loads_syntax_highlighted_content() {
        let (dir, repo) = init_test_repo();
        add_file_commit(
            &repo,
            "main.rs",
            b"fn main() {\n    println!(\"hi\");\n}\n",
            "Add rust file",
        );

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);

        let Mode::View(state) = &app.mode else {
            panic!("expected view mode");
        };
        let crate::mode::FileContent::Text { raw, highlighted } = &state.file_content else {
            panic!("expected text content");
        };
        assert!(raw.contains("fn main"));
        assert!(!highlighted.is_empty());
        assert!(highlighted
            .iter()
            .flat_map(|line| line.spans.iter())
            .any(|span| span.style.fg.is_some()));
    }

    #[test]
    fn test_view_highlights_markdown() {
        let (dir, repo) = init_test_repo();
        add_file_commit(
            &repo,
            "readme.md",
            b"# Title\nSome **bold** text.\n",
            "Add markdown",
        );

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);

        let Mode::View(state) = &app.mode else {
            panic!("expected view mode");
        };
        let crate::mode::FileContent::Text { highlighted, .. } = &state.file_content else {
            panic!("expected text content");
        };
        assert!(!highlighted.is_empty());
        assert!(highlighted
            .iter()
            .flat_map(|line| line.spans.iter())
            .any(|span| span.style.fg.is_some()));
    }

    // ── Navigation boundary tests ──

    #[test]
    fn test_move_up_at_top_does_not_underflow() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('k'));
        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_move_down_at_bottom_does_not_overflow() {
        let (_dir, mut app) = test_app();
        let max_idx = {
            let Mode::Pick(state) = &app.mode else {
                panic!("expected pick")
            };
            state.filtered_indices.len() - 1
        };
        for _ in 0..max_idx + 5 {
            app.handle_key(KeyCode::Char('j'));
        }
        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(state.selected, max_idx);
    }

    #[test]
    fn test_view_mode_file_navigation_bounds() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"a", "A");
        add_file_commit(&repo, "b.txt", b"b", "B");
        add_file_commit(&repo, "c.txt", b"c", "C");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);

        let file_count = {
            let Mode::View(s) = &app.mode else {
                panic!("expected view")
            };
            s.tree.len()
        };
        assert!(file_count > 0);

        for _ in 0..file_count + 5 {
            app.handle_key(KeyCode::Char('j'));
        }
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(s.selected_file < file_count);

        for _ in 0..file_count + 5 {
            app.handle_key(KeyCode::Char('k'));
        }
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.selected_file, 0);
    }

    // ── Ctrl key handler tests ──

    #[test]
    fn test_ctrl_c_quits() {
        let (_dir, mut app) = test_app();
        assert!(!app.should_quit);
        app.handle_ctrl_key(KeyCode::Char('c'));
        assert!(app.should_quit);
    }

    #[test]
    fn test_ctrl_d_toggles_debug() {
        let (_dir, mut app) = test_app();
        assert!(!app.debug_overlay);
        app.handle_ctrl_key(KeyCode::Char('d'));
        assert!(app.debug_overlay);
        app.handle_ctrl_key(KeyCode::Char('d'));
        assert!(!app.debug_overlay);
    }

    #[test]
    fn test_ctrl_n_next_commit_in_view() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        // We're at commits[0] (most recent). Ctrl+P goes older (idx+1).
        app.handle_ctrl_key(KeyCode::Char('p'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let older_id = s.commit.id;
        let _ = s;

        // Ctrl+N goes newer (idx-1), back toward most recent
        app.handle_ctrl_key(KeyCode::Char('n'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_ne!(s.commit.id, older_id, "should have moved to newer commit");
    }

    #[test]
    fn test_ctrl_p_prev_commit_in_view() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let first_id = s.commit.id;
        let _ = s;

        // Ctrl+P goes older (idx+1)
        app.handle_ctrl_key(KeyCode::Char('p'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_ne!(s.commit.id, first_id, "should have moved to older commit");
    }

    #[test]
    fn test_ctrl_p_at_oldest_stays() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        let last_commit_id = app.commits.last().unwrap().id;

        // Ctrl+P navigates older (toward last commit)
        loop {
            let Mode::View(s) = &app.mode else {
                panic!("expected view")
            };
            if s.commit.id == last_commit_id {
                break;
            }
            let _ = s;
            app.handle_ctrl_key(KeyCode::Char('p'));
        }
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let id_before = s.commit.id;
        let _ = s;

        // At oldest, Ctrl+P should stay
        app.handle_ctrl_key(KeyCode::Char('p'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.commit.id, id_before, "should stay at oldest");
    }

    // ── Search flow tests ──

    #[test]
    fn test_search_full_flow_filter_and_commit() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('/'));
        app.handle_key(KeyCode::Char('t'));
        app.handle_key(KeyCode::Char('h'));
        app.handle_key(KeyCode::Enter);

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick")
        };
        assert!(matches!(
            state.search,
            crate::mode::SearchState::Idle { query: Some(_) }
        ));
    }

    #[test]
    fn test_search_esc_with_empty_commits_query() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('/'));
        app.handle_key(KeyCode::Esc);

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick")
        };
        assert!(matches!(
            state.search,
            crate::mode::SearchState::Idle { query: None }
        ));
    }

    #[test]
    fn test_search_only_works_in_pick_mode() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));
        app.handle_key(KeyCode::Char('/'));
        assert!(matches!(app.mode, Mode::View(_)));
    }

    #[test]
    fn test_search_backspace_on_empty() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('/'));
        app.handle_key(KeyCode::Backspace);
        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick")
        };
        match &state.search {
            crate::mode::SearchState::Active { input } => assert!(input.is_empty()),
            _ => panic!("expected active search"),
        }
    }

    // ── Toggle tests ──

    #[test]
    fn test_toggle_view_in_diff_mode() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Tab);
        let Mode::Diff(s) = &app.mode else {
            panic!("expected diff")
        };
        let initial = s.side_by_side;
        let _ = s;

        app.handle_key(KeyCode::Char('s'));
        let Mode::Diff(s) = &app.mode else {
            panic!("expected diff")
        };
        assert_ne!(s.side_by_side, initial);
    }

    #[test]
    fn test_toggle_view_in_pick_mode_does_nothing() {
        let (_dir, mut app) = test_app();
        assert!(matches!(app.mode, Mode::Pick(_)));
        app.handle_key(KeyCode::Char('s'));
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    // ── Page scroll tests ──

    #[test]
    fn test_page_down_in_pick_does_nothing() {
        let (_dir, mut app) = test_app();
        assert!(matches!(app.mode, Mode::Pick(_)));
        app.handle_key(KeyCode::Char('J'));
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    #[test]
    fn test_page_up_in_view_does_not_underflow() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"line1\nline2\nline3\n", "A");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);

        app.handle_key(KeyCode::Char('K'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.scroll, 0);
    }

    // ── Back restores selection ──

    #[test]
    fn test_back_from_view_restores_commit_selection() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('j'));
        let selected_idx = {
            let Mode::Pick(s) = &app.mode else {
                panic!("expected pick")
            };
            s.selected
        };
        assert_eq!(selected_idx, 1);

        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));
        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.mode, Mode::Pick(_)));

        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, selected_idx, "back should restore selection");
    }

    #[test]
    fn test_back_from_diff_restores_commit_selection() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('j'));
        let selected_idx = {
            let Mode::Pick(s) = &app.mode else {
                panic!("expected pick")
            };
            s.selected
        };

        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Tab);
        assert!(matches!(app.mode, Mode::Diff(_)));
        app.handle_key(KeyCode::Esc);

        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, selected_idx);
    }

    // ── Switch mode (View <-> Diff) ──

    #[test]
    fn test_switch_mode_view_to_diff_and_back() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);

        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let view_file = s.selected_file;
        let _ = s;

        app.handle_key(KeyCode::Tab);
        assert!(matches!(app.mode, Mode::Diff(_)));

        app.handle_key(KeyCode::Tab);
        assert!(matches!(app.mode, Mode::View(_)));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.selected_file, view_file, "should restore file selection");
    }

    #[test]
    fn test_tab_in_pick_does_nothing() {
        let (_dir, mut app) = test_app();
        assert!(matches!(app.mode, Mode::Pick(_)));
        app.handle_key(KeyCode::Tab);
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    // ── Commits cached ──

    #[test]
    fn test_commits_cached_in_app() {
        let (_dir, app) = test_app();
        assert!(!app.commits.is_empty());
        if let Mode::Pick(state) = &app.mode {
            assert_eq!(app.commits.len(), state.commits.len());
        }
    }

    // ── View loads file content ──

    #[test]
    fn test_view_binary_file_shows_binary_content() {
        let (dir, repo) = init_test_repo();
        let binary_content = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        add_file_commit(&repo, "image.png", &binary_content, "Add binary");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);

        let Mode::View(state) = &app.mode else {
            panic!("expected view")
        };
        assert!(matches!(
            state.file_content,
            crate::mode::FileContent::Binary
        ));
    }

    #[test]
    fn test_view_directory_selected_stays_not_loaded() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "src/main.rs", b"fn main() {}", "Initial");

        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);

        let dir_idx = {
            let Mode::View(state) = &app.mode else {
                panic!("expected view")
            };
            state
                .tree
                .iter()
                .position(|e| matches!(e.kind, EntryKind::Directory))
        };

        if let Some(idx) = dir_idx {
            loop {
                let Mode::View(s) = &app.mode else {
                    panic!("expected view")
                };
                let cur = s.selected_file;
                let _ = s;
                if cur == idx {
                    break;
                }
                if idx > cur {
                    app.handle_key(KeyCode::Char('j'));
                } else {
                    app.handle_key(KeyCode::Char('k'));
                }
            }
            let Mode::View(s) = &app.mode else {
                panic!("expected view")
            };
            assert!(matches!(
                s.file_content,
                crate::mode::FileContent::NotLoaded
            ));
        }
    }
}
