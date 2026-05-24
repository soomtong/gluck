use crate::config::Config;
use crate::git::cache::{DiffCache, TreeCache};
use crate::git::commit::CommitInfo;
use crate::git::repo::GitRepo;
use crate::git::store::CommitStore;
use crate::git::tree::{is_binary_blob, read_blob, EntryKind};
use crate::highlight::HighlightEngine;
use crate::mode::{Action, DiffState, KeyBindings, Mode, PickState, SearchState, ViewState};
use crate::search::modal::SemanticSearchModal;
use crate::search::SearchEngine;
use crate::theme::Palette;
use crate::ui;
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::Frame;
use std::collections::HashMap;
use std::sync::mpsc;

pub enum IndexMessage {
    Progress(String),
    Done(Result<(), String>),
}

pub enum EngineMessage {
    Progress(String),
    Ready(Box<SearchEngine>),
    Failed(String),
}

pub struct App {
    pub mode: Mode,
    pub repo: GitRepo,
    pub store: CommitStore,
    pub diff_cache: DiffCache,
    pub tree_cache: TreeCache,
    pub keybindings: KeyBindings,
    pub should_quit: bool,
    pub debug_overlay: bool,
    pub highlight: HighlightEngine,
    pub palette: Palette,
    pub theme_name: String,
    pub config: Config,
    pub saved_search: SearchState,
    pub search_modal: SemanticSearchModal,
    pub search_engine: Option<SearchEngine>,
    pub engine_error: Option<String>,
    pub needs_clear: bool,
    pub index_rx: Option<mpsc::Receiver<IndexMessage>>,
    pub engine_rx: Option<mpsc::Receiver<EngineMessage>>,
}

impl App {
    pub fn new(repo: GitRepo, config: Config) -> Result<Self> {
        let store = CommitStore::new(&repo, 200)?;
        let pick_state = PickState::new(store.loaded.clone());
        let theme_name = config.theme.name.clone();
        let palette = crate::theme::resolve_palette(Some(&theme_name));
        let mut app = Self {
            mode: Mode::Pick(pick_state),
            repo,
            store,
            diff_cache: DiffCache::new(64),
            tree_cache: TreeCache::new(32),
            keybindings: KeyBindings::default_bindings(),
            should_quit: false,
            debug_overlay: false,
            highlight: HighlightEngine::new(),
            palette,
            theme_name,
            config,
            saved_search: SearchState::Idle { query: None },
            search_modal: SemanticSearchModal::new(),
            search_engine: None,
            engine_error: None,
            needs_clear: false,
            index_rx: None,
            engine_rx: None,
        };
        app.highlight.set_theme(app.palette.to_highlight_map());
        app.update_pick_diff();
        app.try_preload_engine();
        Ok(app)
    }

    pub fn render(&self, frame: &mut Frame) {
        match &self.mode {
            Mode::Pick(_) => ui::pick::render_pick(frame, frame.area(), self),
            Mode::View(_) => ui::view::render_view(frame, frame.area(), self),
            Mode::Diff(_) => ui::diff::render_diff(frame, frame.area(), self),
        }

        if self.search_modal.is_open() {
            ui::search_modal::render_search_modal(frame, self);
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
                "Mode: {} | Selected: {} | Loaded: {} | Filtered: {} | Exhausted: {}",
                mode_name,
                s.selected,
                s.commits.len(),
                s.filtered_indices.len(),
                self.store.exhausted,
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
        if self.search_modal.is_open() {
            self.handle_modal_key(code);
            return;
        }

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
            Action::SemanticSearch => self.open_semantic_search(),
            Action::ForceIndex => self.force_rebuild_index(),
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
        if self.search_modal.is_open() {
            match code {
                KeyCode::Char('c') => self.should_quit = true,
                KeyCode::Char('n') => self.search_modal.move_down(),
                KeyCode::Char('p') => self.search_modal.move_up(),
                _ => {}
            }
            return;
        }
        match code {
            KeyCode::Char('c') => self.should_quit = true,
            KeyCode::Char('d') => self.debug_overlay = !self.debug_overlay,
            KeyCode::Char('n') => self.prev_commit(),
            KeyCode::Char('p') => self.next_commit(),
            KeyCode::Char('t') => self.next_theme(),
            KeyCode::Char('f') => self.pick_page_down(),
            KeyCode::Char('b') => self.pick_page_up(),
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

    fn prefetch_if_near_end(&mut self) {
        if self.store.exhausted {
            return;
        }
        let (commit_idx, total) = match &self.mode {
            Mode::Pick(state) => {
                let absolute_idx = state
                    .filtered_indices
                    .get(state.selected)
                    .copied()
                    .unwrap_or(0);
                (absolute_idx, state.commits.len())
            }
            _ => return,
        };
        if commit_idx + 50 >= total {
            let _ = self.store.load_batch(&self.repo);
            if let Mode::Pick(state) = &mut self.mode {
                let prev_selected = state.selected;
                state.commits = self.store.loaded.clone();
                let query = state.query().map(|s| s.to_string());
                if let Some(q) = query {
                    state.update_filter(&q);
                    state.selected = state
                        .filtered_indices
                        .iter()
                        .position(|&i| i == prev_selected)
                        .unwrap_or(0);
                } else {
                    state.filtered_indices = (0..state.commits.len()).collect();
                    state.selected = prev_selected;
                }
            }
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
        if matches!(&self.mode, Mode::Pick(_)) {
            self.prefetch_if_near_end();
            self.update_pick_diff();
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

                let mut pick = PickState::new(self.store.loaded.clone());

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
                self.update_pick_diff();
            }
            Mode::Pick(_) => {}
        }
    }

    fn switch_mode(&mut self) {
        let commits = self.store.loaded.clone();
        match &self.mode {
            Mode::View(state) => {
                let current_idx = commits.iter().position(|c| c.id == state.commit.id);
                if let Some(idx) = current_idx {
                    if idx + 1 < commits.len() {
                        let from = commits[idx + 1].clone();
                        let to = commits[idx].clone();
                        let prev = state.selected_file;
                        let prev_path = state.tree.get(state.selected_file).map(|e| e.path.clone());
                        drop(commits);
                        let diff_result = self
                            .diff_cache
                            .get_or_compute(&self.repo, &from, &to)
                            .cloned();
                        if let Ok(diff_result) = diff_result {
                            let mut diff_state = DiffState::new(from, to, diff_result);
                            diff_state.prev_view_file = prev;
                            if let Some(ref path) = prev_path {
                                if let Some(pos) =
                                    diff_state.diff_result.files.iter().position(|f| {
                                        f.change.as_ref().is_some_and(|c| {
                                            c.new_path() == Some(path.as_str())
                                                || c.old_path() == Some(path.as_str())
                                        })
                                    })
                                {
                                    diff_state.selected_file = pos;
                                }
                            }
                            self.mode = Mode::Diff(diff_state);
                        }
                    }
                }
            }
            Mode::Diff(state) => {
                let prev = state.prev_view_file;
                if let Some(idx) = commits.iter().position(|c| c.id == state.to.id) {
                    let commit = commits[idx].clone();
                    let mut view_state = self.make_view_state(commit);
                    view_state.selected_file = prev.min(view_state.tree.len().saturating_sub(1));
                    self.mode = Mode::View(view_state);
                    self.load_view_file();
                }
            }
            Mode::Pick(state) => {
                let Some(&idx) = state.filtered_indices.get(state.selected) else {
                    return;
                };
                let commit = state.commits[idx].clone();
                let saved_search = state.search.clone();
                let parent_info = {
                    let repository = self.repo.repository();
                    repository
                        .find_commit(commit.id)
                        .ok()
                        .and_then(|c| c.parent(0).ok())
                        .map(|p| CommitInfo::from_git_commit(&p))
                };
                let Some(parent_info) = parent_info else {
                    return;
                };
                drop(commits);
                let diff_result = self
                    .diff_cache
                    .get_or_compute(&self.repo, &parent_info, &commit)
                    .cloned();
                if let Ok(diff_result) = diff_result {
                    self.saved_search = saved_search;
                    self.mode = Mode::Diff(DiffState::new(parent_info, commit, diff_result));
                }
            }
        }
    }

    fn next_commit(&mut self) {
        if matches!(self.mode, Mode::Pick(_)) {
            self.move_up();
            return;
        }
        let commits = self.store.loaded.clone();
        match &self.mode {
            Mode::View(s) => {
                let Some(idx) = commits.iter().position(|c| c.id == s.commit.id) else {
                    return;
                };
                if idx == 0 {
                    return;
                }
                let prev_path = self.current_view_file_path();
                let commit = commits[idx - 1].clone();
                let mut state = self.make_view_state(commit);
                restore_file_selection(&mut state, prev_path);
                self.mode = Mode::View(state);
                self.load_view_file();
            }
            Mode::Diff(s) => {
                let Some(idx) = commits.iter().position(|c| c.id == s.to.id) else {
                    return;
                };
                if idx == 0 {
                    return;
                }
                let prev_file = s.selected_file;
                let prev_side_by_side = s.side_by_side;
                let prev_file_path = s
                    .diff_result
                    .files
                    .get(s.selected_file)
                    .and_then(|f| f.change.as_ref().map(|c| c.path()))
                    .map(|p| p.to_string());
                let from = commits[idx].clone();
                let to = commits[idx - 1].clone();
                drop(commits);
                let diff_result = self
                    .diff_cache
                    .get_or_compute(&self.repo, &from, &to)
                    .cloned();
                if let Ok(diff_result) = diff_result {
                    let mut state = DiffState::new(from, to, diff_result);
                    state.side_by_side = prev_side_by_side;
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
        if matches!(self.mode, Mode::Pick(_)) {
            self.move_down();
            return;
        }
        let commits = self.store.loaded.clone();
        match &self.mode {
            Mode::View(s) => {
                let Some(idx) = commits.iter().position(|c| c.id == s.commit.id) else {
                    return;
                };
                if idx + 1 >= commits.len() {
                    return;
                }
                let prev_path = self.current_view_file_path();
                let commit = commits[idx + 1].clone();
                let mut state = self.make_view_state(commit);
                restore_file_selection(&mut state, prev_path);
                self.mode = Mode::View(state);
                self.load_view_file();
            }
            Mode::Diff(s) => {
                let Some(idx) = commits.iter().position(|c| c.id == s.to.id) else {
                    return;
                };
                if idx + 2 >= commits.len() {
                    return;
                }
                let prev_file = s.selected_file;
                let prev_side_by_side = s.side_by_side;
                let prev_file_path = s
                    .diff_result
                    .files
                    .get(s.selected_file)
                    .and_then(|f| f.change.as_ref().map(|c| c.path()))
                    .map(|p| p.to_string());
                let from = commits[idx + 2].clone();
                let to = commits[idx + 1].clone();
                drop(commits);
                let diff_result = self
                    .diff_cache
                    .get_or_compute(&self.repo, &from, &to)
                    .cloned();
                if let Ok(diff_result) = diff_result {
                    let mut state = DiffState::new(from, to, diff_result);
                    state.side_by_side = prev_side_by_side;
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
            Mode::Pick(state) => {
                let max = state.filtered_indices.len().saturating_sub(1);
                state.selected = (state.selected + n).min(max);
            }
        }
        if matches!(&self.mode, Mode::Pick(_)) {
            self.prefetch_if_near_end();
            self.update_pick_diff();
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
            Mode::Pick(state) => {
                state.selected = state.selected.saturating_sub(n);
            }
        }
        if matches!(&self.mode, Mode::Pick(_)) {
            self.update_pick_diff();
        }
    }

    fn toggle_gitignore(&mut self) {
        if let Mode::View(state) = &mut self.mode {
            let prev_path = state.tree.get(state.selected_file).map(|e| e.path.clone());
            state.show_ignored = !state.show_ignored;
            let commit = state.commit.clone();
            let full_tree = self
                .tree_cache
                .get_or_compute(&self.repo, &commit)
                .cloned()
                .unwrap_or_default();
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

    fn pick_page_down(&mut self) {
        if let Mode::Pick(state) = &mut self.mode {
            let max = state.filtered_indices.len().saturating_sub(1);
            state.selected = (state.selected + 20).min(max);
        }
        if matches!(&self.mode, Mode::Pick(_)) {
            self.prefetch_if_near_end();
            self.update_pick_diff();
        }
    }

    fn pick_page_up(&mut self) {
        if let Mode::Pick(state) = &mut self.mode {
            state.selected = state.selected.saturating_sub(20);
        }
        if matches!(&self.mode, Mode::Pick(_)) {
            self.update_pick_diff();
        }
    }

    fn toggle_view(&mut self) {
        if let Mode::Diff(state) = &mut self.mode {
            state.side_by_side = !state.side_by_side;
        }
    }

    fn make_view_state(&mut self, commit: CommitInfo) -> ViewState {
        let tree = self
            .tree_cache
            .get_or_compute(&self.repo, &commit)
            .cloned()
            .unwrap_or_default();
        let changed_stats = {
            let repository = self.repo.repository();
            if let Ok(commit_obj) = repository.find_commit(commit.id) {
                if let Ok(parent) = commit_obj.parent(0) {
                    let parent_info = CommitInfo::from_git_commit(&parent);
                    self.diff_cache
                        .get_or_compute(&self.repo, &parent_info, &commit)
                        .map(|r| {
                            r.files
                                .iter()
                                .filter_map(|f| {
                                    let path = f.change.as_ref().map(|c| c.path().to_string())?;
                                    let added = f
                                        .lines
                                        .iter()
                                        .filter(|l| {
                                            matches!(l, crate::git::diff::DiffLine::Added { .. })
                                        })
                                        .count();
                                    let removed = f
                                        .lines
                                        .iter()
                                        .filter(|l| {
                                            matches!(l, crate::git::diff::DiffLine::Removed { .. })
                                        })
                                        .count();
                                    Some((path, (added, removed)))
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    HashMap::new()
                }
            } else {
                HashMap::new()
            }
        };
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
        let (parent_info, commit) = {
            let Mode::Pick(state) = &mut self.mode else {
                return;
            };
            state.selected_diff = None;
            let Some(&idx) = state.filtered_indices.get(state.selected) else {
                return;
            };
            let commit = state.commits[idx].clone();
            let repository = self.repo.repository();
            let Ok(commit_obj) = repository.find_commit(commit.id) else {
                return;
            };
            let parent = match commit_obj.parent(0) {
                Ok(p) => p,
                Err(_) => return,
            };
            (CommitInfo::from_git_commit(&parent), commit)
        };
        let diff = self
            .diff_cache
            .get_or_compute(&self.repo, &parent_info, &commit)
            .ok()
            .cloned();
        if let Mode::Pick(state) = &mut self.mode {
            state.selected_diff = diff;
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

    fn force_rebuild_index(&mut self) {
        if self.index_rx.is_some() {
            return;
        }
        self.engine_error = None;
        let repo_workdir = self
            .repo
            .repository()
            .workdir()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        self.search_modal.set_indexing("Starting indexer...");
        self.search_engine = None;

        let (tx, rx) = mpsc::channel::<IndexMessage>();
        self.index_rx = Some(rx);

        std::thread::spawn(move || {
            let opts = crate::search::indexer::IndexOptions {
                force: true,
                ..Default::default()
            };
            let repo = match crate::git::repo::GitRepo::open(&repo_workdir) {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(IndexMessage::Done(Err(e.to_string())));
                    return;
                }
            };
            let progress_tx = tx.clone();
            let result = crate::search::silence::with_silenced_stdio(|| {
                crate::search::indexer::build_index(
                    &repo,
                    &repo_workdir,
                    &opts,
                    |msg| {
                        let _ = progress_tx.send(IndexMessage::Progress(msg.to_string()));
                    },
                )
            });
            let _ = tx.send(IndexMessage::Done(result.map_err(|e| e.to_string())));
        });
    }

    pub fn is_indexing(&self) -> bool {
        self.index_rx.is_some() || self.engine_rx.is_some()
    }

    pub fn drain_index_messages(&mut self) {
        let Some(rx) = self.index_rx.as_ref() else { return };
        let mut done = false;
        let mut failure: Option<String> = None;
        loop {
            match rx.try_recv() {
                Ok(IndexMessage::Progress(msg)) => self.search_modal.set_indexing(msg),
                Ok(IndexMessage::Done(Ok(()))) => {
                    done = true;
                    break;
                }
                Ok(IndexMessage::Done(Err(e))) => {
                    done = true;
                    failure = Some(e);
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
            }
        }
        if done {
            self.index_rx = None;
            self.search_engine = None;
            if let Some(e) = failure {
                self.search_modal
                    .set_indexing(format!("Index build failed: {} (Esc)", e));
            } else {
                self.start_loading_engine();
            }
            self.needs_clear = true;
        }
    }

    pub fn drain_engine_messages(&mut self) {
        let Some(rx) = self.engine_rx.as_ref() else { return };
        let mut done = false;
        let mut failure: Option<String> = None;
        let modal_was_open = self.search_modal.is_open();
        loop {
            match rx.try_recv() {
                Ok(EngineMessage::Progress(msg)) => {
                    if modal_was_open {
                        self.search_modal.set_indexing(msg);
                    }
                }
                Ok(EngineMessage::Ready(engine)) => {
                    self.search_engine = Some(*engine);
                    done = true;
                    break;
                }
                Ok(EngineMessage::Failed(msg)) => {
                    done = true;
                    failure = Some(msg);
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    done = true;
                    break;
                }
            }
        }
        if done {
            self.engine_rx = None;
            self.needs_clear = true;
            if let Some(e) = failure {
                self.engine_error = Some(e.clone());
                if modal_was_open {
                    self.search_modal
                        .set_indexing(format!("Search engine failed: {} (Esc to close, I to rebuild)", e));
                }
            } else if self.search_engine.is_some() {
                self.engine_error = None;
                if modal_was_open {
                    self.search_modal.open();
                }
            } else if modal_was_open {
                self.search_modal.close();
            }
        }
    }

    fn try_preload_engine(&mut self) {
        let index_dir = self.index_dir();
        if crate::search::indexer::index_status(&index_dir)
            != crate::search::indexer::IndexStatus::Ready
        {
            return;
        }
        self.start_loading_engine();
    }

    fn index_dir(&self) -> std::path::PathBuf {
        let repo_workdir = self
            .repo
            .repository()
            .workdir()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        crate::search::indexer::index_dir_for(&repo_workdir)
    }

    fn start_loading_engine(&mut self) {
        if self.engine_rx.is_some() || self.search_engine.is_some() {
            return;
        }
        let index_dir = self.index_dir();
        if crate::search::indexer::index_status(&index_dir)
            != crate::search::indexer::IndexStatus::Ready
        {
            return;
        }
        let (tx, rx) = mpsc::channel::<EngineMessage>();
        self.engine_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(EngineMessage::Progress(
                "Loading embedding model...".to_string(),
            ));
            let result = crate::search::silence::with_silenced_stdio(|| {
                SearchEngine::open(&index_dir)
            });
            let msg = match result {
                Ok(engine) => EngineMessage::Ready(Box::new(engine)),
                Err(e) => EngineMessage::Failed(e.to_string()),
            };
            let _ = tx.send(msg);
        });
    }

    fn open_semantic_search(&mut self) {
        let index_dir = self.index_dir();
        use crate::search::indexer::IndexStatus;
        match crate::search::indexer::index_status(&index_dir) {
            IndexStatus::Missing => {
                self.search_modal.set_no_index();
                return;
            }
            IndexStatus::SchemaOutdated => {
                self.search_modal
                    .set_indexing("Index schema outdated — rebuilding...");
                self.force_rebuild_index();
                return;
            }
            IndexStatus::Ready => {}
        }
        if self.search_engine.is_some() {
            self.search_modal.open();
            return;
        }
        if let Some(ref err) = self.engine_error {
            let msg = format!("Model unavailable: {}. Press I to rebuild index, Esc to close.", err);
            self.search_modal.set_indexing(msg);
            return;
        }
        self.search_modal
            .set_indexing("Loading embedding model...");
        self.start_loading_engine();
    }

    fn handle_modal_key(&mut self, code: KeyCode) {
        use crate::search::modal::ModalState;
        if matches!(self.search_modal.state, ModalState::NoIndex) {
            match code {
                KeyCode::Esc => self.search_modal.close(),
                KeyCode::Char('I') => self.force_rebuild_index(),
                _ => {}
            }
            return;
        }
        if matches!(self.search_modal.state, ModalState::Indexing { .. }) {
            match code {
                KeyCode::Esc => self.search_modal.close(),
                KeyCode::Char('I') => self.force_rebuild_index(),
                _ => {}
            }
            return;
        }
        match code {
            KeyCode::Esc => {
                self.search_modal.close();
            }
            KeyCode::Backspace => {
                self.search_modal.pop_char();
                self.run_semantic_search();
            }
            KeyCode::Down => {
                self.search_modal.move_down();
            }
            KeyCode::Up => {
                self.search_modal.move_up();
            }
            KeyCode::Enter => {
                self.select_search_result();
            }
            KeyCode::Char(c) => {
                self.search_modal.push_char(c);
                self.run_semantic_search();
            }
            _ => {}
        }
    }

    fn run_semantic_search(&mut self) {
        let query = self.search_modal.state.input().to_string();
        if query.is_empty() {
            self.search_modal
                .set_results(vec![]);
            return;
        }
        if let Some(engine) = &self.search_engine {
            let limit = self.config.search.result_limit;
            match engine.search(&query, limit) {
                Ok(results) => self.search_modal.set_results(results),
                Err(_) => self.search_modal.set_results(vec![]),
            }
        }
    }

    fn lookup_commit(&self, oid: git2::Oid) -> Option<CommitInfo> {
        if let Some(c) = self.store.loaded.iter().find(|c| c.id == oid) {
            return Some(c.clone());
        }
        let repository = self.repo.repository();
        repository
            .find_commit(oid)
            .ok()
            .map(|c| CommitInfo::from_git_commit(&c))
    }

    fn select_search_result(&mut self) {
        use crate::search::DocKind;
        let result = self
            .search_modal
            .results()
            .get(self.search_modal.selected)
            .cloned();
        let Some(result) = result else { return };
        self.search_modal.close();

        let Ok(git_oid) = git2::Oid::from_str(&result.meta.commit_oid) else { return };
        let Some(commit) = self.lookup_commit(git_oid) else { return };

        match result.meta.kind {
            DocKind::Commit => {
                let parent_info = {
                    let repository = self.repo.repository();
                    repository
                        .find_commit(commit.id)
                        .ok()
                        .and_then(|c| c.parent(0).ok())
                        .map(|p| CommitInfo::from_git_commit(&p))
                };
                if let Some(parent) = parent_info {
                    if let Ok(diff_result) = self
                        .diff_cache
                        .get_or_compute(&self.repo, &parent, &commit)
                        .cloned()
                    {
                        self.mode = Mode::Diff(DiffState::new(parent, commit, diff_result));
                    }
                } else {
                    let view_state = self.make_view_state(commit);
                    self.mode = Mode::View(view_state);
                    self.load_view_file();
                }
            }
            DocKind::File | DocKind::Symbol => {
                let path = result.meta.path.clone().unwrap_or_default();
                let line = result.meta.line_start;
                let mut view_state = self.make_view_state(commit);
                if let Some(file_idx) = view_state.tree.iter().position(|e| e.path == path) {
                    view_state.selected_file = file_idx;
                }
                if let Some(line_start) = line {
                    view_state.scroll = line_start as usize;
                }
                self.mode = Mode::View(view_state);
                self.load_view_file();
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
    use crate::git::repo::tests::{add_file_commit, init_test_repo, init_test_repo_with_n_commits};

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
    fn test_ctrl_p_next_commit_in_view() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        app.handle_ctrl_key(KeyCode::Char('n'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let older_id = s.commit.id;
        let _ = s;

        app.handle_ctrl_key(KeyCode::Char('p'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_ne!(s.commit.id, older_id, "should have moved to newer commit");
    }

    #[test]
    fn test_ctrl_n_prev_commit_in_view() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let first_id = s.commit.id;
        let _ = s;

        app.handle_ctrl_key(KeyCode::Char('n'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_ne!(s.commit.id, first_id, "should have moved to older commit");
    }

    #[test]
    fn test_ctrl_n_at_oldest_stays() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Enter);
        let last_commit_id = app.store.loaded.last().unwrap().id;

        loop {
            let Mode::View(s) = &app.mode else {
                panic!("expected view")
            };
            if s.commit.id == last_commit_id {
                break;
            }
            let _ = s;
            app.handle_ctrl_key(KeyCode::Char('n'));
        }
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let id_before = s.commit.id;
        let _ = s;

        app.handle_ctrl_key(KeyCode::Char('n'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.commit.id, id_before, "should stay at oldest");
    }

    #[test]
    fn test_ctrl_n_in_pick_moves_down() {
        let (_dir, mut app) = test_app();
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, 0);
        let _ = s;

        app.handle_ctrl_key(KeyCode::Char('n'));
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, 1);
    }

    #[test]
    fn test_ctrl_p_in_pick_moves_up() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('j'));
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, 1);
        let _ = s;

        app.handle_ctrl_key(KeyCode::Char('p'));
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, 0);
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

        app.handle_key(KeyCode::Char('v'));
        let Mode::Diff(s) = &app.mode else {
            panic!("expected diff")
        };
        assert_ne!(s.side_by_side, initial);
    }

    #[test]
    fn test_toggle_view_in_pick_mode_does_nothing() {
        let (_dir, mut app) = test_app();
        assert!(matches!(app.mode, Mode::Pick(_)));
        app.handle_key(KeyCode::Char('v'));
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
    fn test_tab_in_pick_goes_to_diff() {
        let (_dir, mut app) = test_app();
        assert!(matches!(app.mode, Mode::Pick(_)));
        app.handle_key(KeyCode::Tab);
        assert!(matches!(app.mode, Mode::Diff(_)));
    }

    #[test]
    fn test_tab_in_pick_diff_shows_current_commit() {
        let (_dir, mut app) = test_app();
        let commit_id = {
            let Mode::Pick(s) = &app.mode else {
                panic!("expected pick")
            };
            let &idx = s.filtered_indices.get(s.selected).unwrap();
            s.commits[idx].id
        };
        app.handle_key(KeyCode::Tab);
        let Mode::Diff(s) = &app.mode else {
            panic!("expected diff")
        };
        assert_eq!(s.to.id, commit_id);
    }

    #[test]
    fn test_tab_pick_to_diff_esc_back_to_pick() {
        let (_dir, mut app) = test_app();
        app.handle_key(KeyCode::Char('j'));
        let selected_idx = {
            let Mode::Pick(s) = &app.mode else {
                panic!("expected pick")
            };
            s.selected
        };
        app.handle_key(KeyCode::Tab);
        assert!(matches!(app.mode, Mode::Diff(_)));
        app.handle_key(KeyCode::Esc);
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, selected_idx);
    }

    // ── Commits cached ──

    #[test]
    fn test_commits_cached_in_app() {
        let (_dir, app) = test_app();
        assert!(!app.store.loaded.is_empty());
        if let Mode::Pick(state) = &app.mode {
            assert_eq!(app.store.loaded.len(), state.commits.len());
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

    // ── Performance integration tests ──

    #[test]
    fn test_paging_triggers_on_near_end() {
        let (dir, _repo) = init_test_repo_with_n_commits(300);
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();

        // Initial load: 200 (batch_size)
        assert_eq!(app.store.loaded.len(), 200);
        assert!(!app.store.exhausted);

        // Navigate to near end (absolute idx ~150 + 50 >= 200 → triggers prefetch)
        for _ in 0..150 {
            app.handle_key(KeyCode::Char('j'));
        }
        // After reaching near end, loaded count should have increased
        assert!(app.store.loaded.len() > 200 || app.store.exhausted);
    }

    #[test]
    fn test_diff_cache_hit_on_cursor_move() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First");
        add_file_commit(&repo, "a.txt", b"second", "Second");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();

        // Move down (to commit 1) — diff computed and cached
        app.handle_key(KeyCode::Char('j'));
        // Move up (back to commit 0) — should compute new diff
        app.handle_key(KeyCode::Char('k'));

        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert!(s.selected_diff.is_some());
    }

    #[test]
    fn test_tree_cache_hit_on_view_reentry() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "src/main.rs", b"fn main() {}", "Initial");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();

        // Enter view (populates tree cache)
        app.handle_key(KeyCode::Enter);
        // Back to pick
        app.handle_key(KeyCode::Esc);
        // Enter view again (should cache hit)
        app.handle_key(KeyCode::Enter);

        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(!s.tree.is_empty());
    }
}
