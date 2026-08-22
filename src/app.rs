use crate::config::Config;
use crate::git::cache::{DiffCache, TreeCache};
use crate::git::commit::CommitInfo;
use crate::git::repo::GitRepo;
use crate::git::store::CommitStore;
use crate::git::tree::{is_binary_blob, read_blob, EntryKind};
use crate::highlight::HighlightEngine;
use crate::mode::{Action, DiffState, KeyBindings, Mode, PickState, SearchState, ViewState};
use crate::search::modal_state::SemanticSearchModal;
use crate::search::SearchEngine;
use crate::search::SearchResult;
use crate::theme::Palette;
use crate::ui;
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::layout::{Margin, Position, Rect};
use ratatui::Frame;
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub enum IndexMessage {
    Progress(String),
    Done(Result<(), String>),
}

pub enum EngineMessage {
    Progress(String),
    Ready(Box<SearchEngine>),
    Failed(String),
}

pub const HEAD_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// How long j/k navigation must settle on a file before its content is
/// loaded and highlighted. Held-down keys repeat faster than this, so
/// skipped-over files are never loaded.
pub const VIEW_LOAD_DEBOUNCE: Duration = Duration::from_millis(60);

/// Files larger than this skip tree-sitter highlighting and render as plain
/// text — parsing multi-hundred-KB sources blocks the UI thread.
const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;

const MOUSE_WHEEL_LINES: usize = 3;
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

/// LRU cache of rendered file contents keyed by (commit, path), so moving
/// back and forth over already-viewed files is instant.
pub struct FileContentCache {
    entries: HashMap<(git2::Oid, String), crate::mode::FileContent>,
    order: VecDeque<(git2::Oid, String)>,
    max_size: usize,
}

impl FileContentCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            max_size,
        }
    }

    pub fn get(&mut self, key: &(git2::Oid, String)) -> Option<crate::mode::FileContent> {
        if !self.entries.contains_key(key) {
            return None;
        }
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
            self.order.push_back(key.clone());
        }
        self.entries.get(key).cloned()
    }

    pub fn insert(&mut self, key: (git2::Oid, String), content: crate::mode::FileContent) {
        if self.entries.len() >= self.max_size {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key.clone(), content);
        self.order.push_back(key);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }
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
    pub search_tx: Option<mpsc::Sender<String>>,
    pub search_rx: Option<mpsc::Receiver<Vec<SearchResult>>>,
    pub search_pending: bool,
    pub engine_error: Option<String>,
    pub needs_clear: bool,
    pub index_rx: Option<mpsc::Receiver<IndexMessage>>,
    pub engine_rx: Option<mpsc::Receiver<EngineMessage>>,
    pub last_head: Option<(git2::Oid, String)>,
    pub repo_changed: bool,
    pub last_head_check: Instant,
    pub content_cache: FileContentCache,
    pub pending_view_load: Option<Instant>,
    // Panel geometry captured at render time for mouse hit-testing.
    pub view_tree_area: Option<Rect>,
    pub view_content_area: Option<Rect>,
    pub view_tree_offset: usize,
    pub pick_list_area: Option<Rect>,
    pub pick_list_offset: usize,
    last_click: Option<(Instant, usize)>,
}

impl App {
    pub fn new(repo: GitRepo, config: Config) -> Result<Self> {
        let last_head = repo.head_info();
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
            search_tx: None,
            search_rx: None,
            search_pending: false,
            engine_error: None,
            needs_clear: false,
            index_rx: None,
            engine_rx: None,
            last_head,
            repo_changed: false,
            last_head_check: Instant::now(),
            content_cache: FileContentCache::new(32),
            pending_view_load: None,
            view_tree_area: None,
            view_content_area: None,
            view_tree_offset: 0,
            pick_list_area: None,
            pick_list_offset: 0,
            last_click: None,
        };
        app.highlight.set_theme(app.palette.to_highlight_map());
        app.update_pick_diff();
        app.try_preload_engine();
        Ok(app)
    }

    pub fn render(&mut self, frame: &mut Frame) {
        if matches!(self.mode, Mode::Pick(_)) {
            ui::pick::render_pick(frame, frame.area(), self);
        } else if matches!(self.mode, Mode::View(_)) {
            ui::view::render_view(frame, frame.area(), self);
        } else {
            ui::diff::render_diff(frame, frame.area(), self);
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
        use ratatui::style::Style;
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
        // 'i'/'I' should only trigger reindex while the modal is in Loading state
        // (per the modal state machine). Capture this before handle_key advances state.
        let was_loading = matches!(
            self.search_modal.state,
            crate::search::modal_state::ModalState::Loading { .. }
        );
        if self.search_modal.handle_key(code) {
            if code == KeyCode::Enter {
                self.select_search_result();
            } else if was_loading && matches!(code, KeyCode::Char('I') | KeyCode::Char('i')) {
                self.force_rebuild_index();
            }
            if self.search_modal.is_open() {
                self.run_semantic_search();
            }
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

        // View mode: h/l fold/unfold the selected directory. On files they
        // keep their default bindings (Back / open). H/L jump the tree
        // selection between changed (*-marked) files; J/K keep paging.
        if matches!(self.mode, Mode::View(_)) {
            match code {
                KeyCode::Char('h') if self.view_fold_or_parent() => return,
                KeyCode::Char('l') if self.view_unfold_or_first_child() => return,
                KeyCode::Char('L') => {
                    self.view_jump_change(true);
                    return;
                }
                KeyCode::Char('H') => {
                    self.view_jump_change(false);
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
                KeyCode::Char('c') => self.search_modal.close(),
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

    pub fn handle_mouse(&mut self, ev: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        if self.search_modal.is_open() {
            return;
        }
        match ev.kind {
            MouseEventKind::ScrollDown => self.mouse_scroll(ev.column, ev.row, true),
            MouseEventKind::ScrollUp => self.mouse_scroll(ev.column, ev.row, false),
            MouseEventKind::Down(MouseButton::Left) => self.mouse_click(ev.column, ev.row),
            _ => {}
        }
    }

    fn mouse_scroll(&mut self, col: u16, row: u16, down: bool) {
        match &self.mode {
            Mode::View(_) => {
                if rect_contains(self.view_tree_area, col, row) {
                    if down {
                        self.move_down();
                    } else {
                        self.move_up();
                    }
                } else if let Mode::View(state) = &mut self.mode {
                    if down {
                        let max_scroll = state.line_count().saturating_sub(1);
                        state.scroll = (state.scroll + MOUSE_WHEEL_LINES).min(max_scroll);
                    } else {
                        state.scroll = state.scroll.saturating_sub(MOUSE_WHEEL_LINES);
                    }
                }
            }
            Mode::Pick(_) => {
                if down {
                    self.move_down();
                } else {
                    self.move_up();
                }
            }
            Mode::Diff(_) => {
                if let Mode::Diff(state) = &mut self.mode {
                    if down {
                        let line_count = state
                            .diff_result
                            .files
                            .get(state.selected_file)
                            .map(|f| f.lines.len())
                            .unwrap_or(0);
                        state.scroll =
                            (state.scroll + MOUSE_WHEEL_LINES).min(line_count.saturating_sub(1));
                    } else {
                        state.scroll = state.scroll.saturating_sub(MOUSE_WHEEL_LINES);
                    }
                }
            }
        }
    }

    fn mouse_click(&mut self, col: u16, row: u16) {
        match &self.mode {
            Mode::View(_) => {
                if let Some(idx) =
                    list_row_index(self.view_tree_area, self.view_tree_offset, col, row)
                {
                    self.view_tree_click(idx);
                }
            }
            Mode::Pick(_) => {
                if let Some(idx) =
                    list_row_index(self.pick_list_area, self.pick_list_offset, col, row)
                {
                    self.pick_click(idx);
                }
            }
            Mode::Diff(_) => {}
        }
    }

    /// Click in the file tree: select the entry; directories toggle their
    /// fold, files load.
    fn view_tree_click(&mut self, idx: usize) {
        {
            let Mode::View(state) = &mut self.mode else {
                return;
            };
            if idx >= state.visible.len() {
                return;
            }
            state.selected_file = idx;
            state.toggle_fold();
        }
        self.request_view_file_load();
    }

    /// Click in the commit list: select; a double-click opens View mode.
    fn pick_click(&mut self, idx: usize) {
        let double = self
            .last_click
            .take()
            .is_some_and(|(t, i)| i == idx && t.elapsed() < DOUBLE_CLICK_WINDOW);
        {
            let Mode::Pick(state) = &mut self.mode else {
                return;
            };
            if idx >= state.filtered_indices.len() {
                return;
            }
            state.selected = idx;
        }
        self.prefetch_if_near_end();
        self.update_pick_diff();
        if double {
            self.enter();
        } else {
            self.last_click = Some((Instant::now(), idx));
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
                let max = state.visible.len().saturating_sub(1);
                state.selected_file = state.selected_file.saturating_add(1).min(max);
                self.request_view_file_load();
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
                self.request_view_file_load();
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
                let toggled = match &mut self.mode {
                    Mode::View(state) => state.toggle_fold(),
                    _ => false,
                };
                if !toggled {
                    self.load_view_file();
                }
            }
            Mode::Diff(_) => {}
        }
    }

    /// 'h' in View mode: collapse the selected expanded directory, or jump
    /// to the parent of an already-collapsed one. Returns false (falls
    /// through to Back) on files and top-level collapsed directories.
    fn view_fold_or_parent(&mut self) -> bool {
        let handled = {
            let Mode::View(state) = &mut self.mode else {
                return false;
            };
            let Some(entry) = state.selected_entry() else {
                return false;
            };
            if !matches!(entry.kind, EntryKind::Directory) {
                return false;
            }
            let path = entry.path.clone();
            if !state.collapsed.contains(&path) {
                state.collapsed.insert(path.clone());
                state.rebuild_visible();
                state.select_visible_path(&path);
                true
            } else {
                state.select_parent()
            }
        };
        if handled {
            self.request_view_file_load();
        }
        handled
    }

    /// 'l' in View mode: expand the selected collapsed directory, or step
    /// into the first child of an expanded one. Returns false on files so
    /// the default Enter binding opens them.
    fn view_unfold_or_first_child(&mut self) -> bool {
        let handled = {
            let Mode::View(state) = &mut self.mode else {
                return false;
            };
            let Some(entry) = state.selected_entry() else {
                return false;
            };
            if !matches!(entry.kind, EntryKind::Directory) {
                return false;
            }
            let path = entry.path.clone();
            if state.collapsed.remove(&path) {
                state.rebuild_visible();
                state.select_visible_path(&path);
            } else {
                let next = state.selected_file + 1;
                let child_prefix = format!("{}/", path);
                if state
                    .visible_entry(next)
                    .is_some_and(|e| e.path.starts_with(&child_prefix))
                {
                    state.selected_file = next;
                }
            }
            true
        };
        if handled {
            self.request_view_file_load();
        }
        handled
    }

    /// 'L'/'H' in View mode: jump the tree selection to the next/previous
    /// changed file, expanding collapsed ancestors so it becomes visible.
    /// No-op when there is no further change in that direction.
    fn view_jump_change(&mut self, forward: bool) {
        let moved = {
            let Mode::View(state) = &mut self.mode else {
                return;
            };
            let target = if forward {
                state.next_changed_path()
            } else {
                state.prev_changed_path()
            };
            match target {
                Some(path) => state.select_path(&path),
                None => false,
            }
        };
        if moved {
            self.request_view_file_load();
        }
    }

    fn back(&mut self) {
        if self.repo_changed {
            // Deferred refresh from View/Diff: rebuild the store first so the
            // PickState below is built from the fresh commit list.
            self.apply_repo_refresh();
        }
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
                        let prev_path = state.selected_entry().map(|e| e.path.clone());
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
                    view_state.selected_file = prev.min(view_state.visible.len().saturating_sub(1));
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
                let prev_collapsed = s.collapsed.clone();
                let commit = commits[idx - 1].clone();
                let mut state = self.make_view_state(commit);
                state.collapsed = prev_collapsed;
                state.rebuild_visible();
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
                let prev_collapsed = s.collapsed.clone();
                let commit = commits[idx + 1].clone();
                let mut state = self.make_view_state(commit);
                state.collapsed = prev_collapsed;
                state.rebuild_visible();
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
            Mode::View(s) => s.selected_entry().map(|e| e.path.clone()),
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
            let prev_path = state.selected_entry().map(|e| e.path.clone());
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
            state.rebuild_visible();
            let restored = prev_path.map(|p| state.select_path(&p)).unwrap_or(false);
            if !restored {
                state.selected_file = 0;
            }
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
        let visible = (0..tree.len()).collect();
        ViewState {
            commit,
            tree,
            collapsed: std::collections::HashSet::new(),
            visible,
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

    /// Schedule loading the selected file after VIEW_LOAD_DEBOUNCE of
    /// navigation quiet. Cached content and non-file selections apply
    /// immediately.
    fn request_view_file_load(&mut self) {
        let key = {
            let Mode::View(state) = &mut self.mode else {
                return;
            };
            state.scroll = 0;
            state
                .selected_entry()
                .filter(|e| matches!(e.kind, EntryKind::File))
                .map(|e| (state.commit.id, e.path.clone()))
        };
        let Some(key) = key else {
            if let Mode::View(vs) = &mut self.mode {
                vs.file_content = crate::mode::FileContent::NotLoaded;
            }
            self.pending_view_load = None;
            return;
        };
        if let Some(content) = self.content_cache.get(&key) {
            if let Mode::View(vs) = &mut self.mode {
                vs.file_content = content;
            }
            self.pending_view_load = None;
            return;
        }
        if let Mode::View(vs) = &mut self.mode {
            vs.file_content = crate::mode::FileContent::Loading;
        }
        self.pending_view_load = Some(Instant::now());
    }

    /// Called from the main loop: run the debounced load once navigation
    /// has settled.
    pub fn tick_pending_view_load(&mut self) {
        let Some(requested) = self.pending_view_load else {
            return;
        };
        if requested.elapsed() >= VIEW_LOAD_DEBOUNCE {
            self.load_view_file();
        }
    }

    fn load_view_file(&mut self) {
        self.pending_view_load = None;
        let to_load = match &self.mode {
            Mode::View(state) => state
                .selected_entry()
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

        let key = (commit.id, path.clone());
        if let Some(content) = self.content_cache.get(&key) {
            if let Mode::View(vs) = &mut self.mode {
                vs.file_content = content;
            }
            return;
        }

        let binary = is_binary_blob(&self.repo, &commit, &path).unwrap_or(false);
        let content = if binary {
            crate::mode::FileContent::Binary
        } else if let Ok(content) = read_blob(&self.repo, &commit, &path) {
            let highlighted = if content.len() <= MAX_HIGHLIGHT_BYTES {
                self.highlight.highlight(&content, &path)
            } else {
                HighlightEngine::plain_lines(&content)
            };
            crate::mode::FileContent::Text {
                raw: content,
                highlighted,
            }
        } else {
            if let Mode::View(vs) = &mut self.mode {
                vs.file_content = crate::mode::FileContent::NotLoaded;
            }
            return;
        };

        self.content_cache.insert(key, content.clone());
        if let Mode::View(vs) = &mut self.mode {
            vs.file_content = content;
        }
    }

    fn force_rebuild_index(&mut self) {
        if self.index_rx.is_some() {
            if !self.search_modal.is_open() {
                self.search_modal.set_loading("Indexing...");
            }
            return;
        }
        self.engine_error = None;
        self.engine_rx = None;
        self.search_tx = None;
        self.search_rx = None;
        self.search_pending = false;
        self.search_engine = None;
        let repo_workdir = self
            .repo
            .repository()
            .workdir()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        self.search_modal.set_loading("Starting indexer...");

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
                crate::search::indexer::build_index(&repo, &repo_workdir, &opts, |msg| {
                    let _ = progress_tx.send(IndexMessage::Progress(msg.to_string()));
                })
            });
            let _ = tx.send(IndexMessage::Done(result.map_err(|e| e.to_string())));
        });
    }

    /// Compare current HEAD against the last snapshot. Updates the snapshot
    /// on change. Unreadable HEAD (unborn, deleted .git) is treated as no
    /// change so the app keeps running and retries next tick.
    pub fn check_repo_changed(&mut self) -> bool {
        let current = self.repo.head_info();
        if current.is_none() {
            return false;
        }
        if current != self.last_head {
            self.last_head = current;
            true
        } else {
            false
        }
    }

    /// Rebuild the commit store from the current HEAD and, in Pick mode,
    /// re-apply the active filter and restore the selection by commit oid.
    /// Outside Pick mode only the store is rebuilt; callers refresh the
    /// visible state when transitioning back to Pick. Returns `true` if the
    /// store was rebuilt, `false` if the rebuild failed (e.g. concurrent
    /// gc/lock) and the caller should decide how to retry.
    pub fn apply_repo_refresh(&mut self) -> bool {
        let prev_total = self.store.total_loaded();
        let mut new_store = match CommitStore::new(&self.repo, 200) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("repo refresh failed to rebuild commit store: {}", e);
                return false;
            }
        };
        // Keep the previous scroll depth available after the rebuild.
        while new_store.total_loaded() < prev_total && !new_store.exhausted {
            if new_store.load_batch(&self.repo).is_err() {
                break;
            }
        }
        self.store = new_store;
        self.repo_changed = false;

        if let Mode::Pick(state) = &mut self.mode {
            let prev_oid = state
                .filtered_indices
                .get(state.selected)
                .map(|&i| state.commits[i].id);
            let prev_selected = state.selected;
            state.commits = self.store.loaded.clone();
            let query = state.query().map(|s| s.to_string());
            match query {
                Some(q) => state.update_filter(&q),
                None => {
                    state.filtered_indices = (0..state.commits.len()).collect();
                    state.scroll = 0;
                }
            }
            state.selected = prev_oid
                .and_then(|oid| state.commits.iter().position(|c| c.id == oid))
                .and_then(|full_idx| state.filtered_indices.iter().position(|&i| i == full_idx))
                .unwrap_or_else(|| {
                    prev_selected.min(state.filtered_indices.len().saturating_sub(1))
                });
        }
        if matches!(self.mode, Mode::Pick(_)) {
            self.update_pick_diff();
        }
        true
    }

    /// Called every main-loop iteration. Checks HEAD at most once per
    /// HEAD_POLL_INTERVAL. In Pick mode a detected change refreshes the
    /// commit list immediately; in View/Diff it only raises `repo_changed`
    /// so the footer can show a notice without disturbing the viewed content.
    pub fn poll_repo_watch(&mut self) {
        if self.last_head_check.elapsed() < HEAD_POLL_INTERVAL {
            return;
        }
        self.last_head_check = Instant::now();
        let prev_head = self.last_head.clone();
        if self.check_repo_changed() {
            if matches!(self.mode, Mode::Pick(_)) {
                if !self.apply_repo_refresh() {
                    // Store rebuild failed: restore the snapshot so the
                    // next tick detects the change again and retries.
                    self.last_head = prev_head;
                }
            } else {
                self.repo_changed = true;
            }
        }
    }

    pub fn is_indexing(&self) -> bool {
        self.index_rx.is_some() || self.engine_rx.is_some() || self.search_pending
    }

    pub fn drain_index_messages(&mut self) {
        let Some(rx) = self.index_rx.as_ref() else {
            return;
        };
        let mut done = false;
        let mut failure: Option<String> = None;
        loop {
            match rx.try_recv() {
                Ok(IndexMessage::Progress(msg)) => self.search_modal.set_loading(msg),
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
                    .set_loading(format!("Index build failed: {} (Esc)", e));
            } else {
                self.start_loading_engine();
            }
            self.needs_clear = true;
        }
    }

    pub fn drain_engine_messages(&mut self) {
        let Some(rx) = self.engine_rx.as_ref() else {
            return;
        };
        let mut done = false;
        let mut failure: Option<String> = None;
        let modal_was_open = self.search_modal.is_open();
        loop {
            match rx.try_recv() {
                Ok(EngineMessage::Progress(msg)) => {
                    if modal_was_open {
                        self.search_modal.set_loading(msg);
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
                    self.search_modal.set_loading(format!(
                        "Search engine failed: {} (Esc to close, I to rebuild)",
                        e
                    ));
                }
            } else if self.search_engine.is_some() {
                if let Some(engine) = self.search_engine.take() {
                    self.spawn_search_worker(engine);
                }
                self.engine_error = None;
                if modal_was_open {
                    self.search_modal.open();
                    if !self.search_modal.state.input().is_empty() {
                        self.run_semantic_search();
                    }
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
        if self.engine_rx.is_some() || self.search_rx.is_some() {
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
            let result =
                crate::search::silence::with_silenced_stdio(|| SearchEngine::open(&index_dir));
            let msg = match result {
                Ok(engine) => EngineMessage::Ready(Box::new(engine)),
                Err(e) => EngineMessage::Failed(e.to_string()),
            };
            let _ = tx.send(msg);
        });
    }

    fn spawn_search_worker(&mut self, engine: SearchEngine) {
        self.search_engine = None;
        let (stx, worker_rx) = mpsc::channel::<String>();
        let (worker_tx, srx) = mpsc::channel::<Vec<SearchResult>>();
        self.search_tx = Some(stx);
        self.search_rx = Some(srx);
        let limit = self.config.search.result_limit;
        std::thread::spawn(move || {
            while let Ok(query) = worker_rx.recv() {
                if query.is_empty() {
                    let _ = worker_tx.send(vec![]);
                    continue;
                }
                let results = engine.search(&query, limit).unwrap_or_default();
                let _ = worker_tx.send(results);
            }
        });
    }

    fn open_semantic_search(&mut self) {
        let index_dir = self.index_dir();
        use crate::search::indexer::IndexStatus;
        match crate::search::indexer::index_status(&index_dir) {
            IndexStatus::Missing => {
                self.search_modal
                    .set_loading("No index found. Press I to build, Esc to close.");
                return;
            }
            IndexStatus::SchemaOutdated => {
                self.search_modal
                    .set_loading("Index schema outdated — rebuilding...");
                self.force_rebuild_index();
                return;
            }
            IndexStatus::Ready => {}
        }
        if self.search_rx.is_some() {
            self.search_modal.open();
            if !self.search_modal.state.input().is_empty() {
                self.run_semantic_search();
            }
            return;
        }
        if let Some(ref err) = self.engine_error {
            let msg = format!(
                "Model unavailable: {}. Press I to rebuild index, Esc to close.",
                err
            );
            self.search_modal.set_loading(msg);
            return;
        }
        self.search_modal.set_loading("Loading embedding model...");
        self.start_loading_engine();
    }

    fn run_semantic_search(&mut self) {
        let query = self.search_modal.state.input().to_string();
        if query.is_empty() {
            self.search_modal.set_results(vec![]);
            return;
        }
        if let Some(tx) = &self.search_tx {
            self.search_pending = true;
            let _ = tx.send(query);
        }
    }

    pub fn drain_search_results(&mut self) {
        let Some(rx) = self.search_rx.as_ref() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(results) => {
                    self.search_modal.set_results(results);
                    self.search_pending = false;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.search_rx = None;
                    self.search_tx = None;
                    self.search_pending = false;
                    break;
                }
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

        let Ok(git_oid) = git2::Oid::from_str(&result.meta.commit_oid) else {
            return;
        };
        let Some(commit) = self.lookup_commit(git_oid) else {
            return;
        };

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
                view_state.select_path(&path);
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
        // Cached highlights carry the old palette.
        self.content_cache.clear();
        if let Mode::View(s) = &self.mode {
            let prev_scroll = s.scroll;
            self.load_view_file();
            if let Mode::View(s) = &mut self.mode {
                s.scroll = prev_scroll.min(s.line_count().saturating_sub(1));
            }
        }
        self.config.theme.name = self.theme_name.clone();
        let _ = self.config.save();
    }
}

fn rect_contains(rect: Option<Rect>, col: u16, row: u16) -> bool {
    rect.is_some_and(|r| r.contains(Position::new(col, row)))
}

/// Map a click inside a bordered List widget to an item index, using the
/// scroll offset captured at render time. None when the click is outside
/// the list's inner area.
fn list_row_index(rect: Option<Rect>, offset: usize, col: u16, row: u16) -> Option<usize> {
    let inner = rect?.inner(Margin::new(1, 1));
    if !inner.contains(Position::new(col, row)) {
        return None;
    }
    Some(offset + (row - inner.y) as usize)
}

fn restore_file_selection(state: &mut ViewState, prev_path: Option<String>) {
    let Some(path) = prev_path else {
        return;
    };
    if state.select_path(&path) {
        return;
    }
    // Path gone in this commit: fall back to the nearest existing ancestor.
    let mut parent = path.as_str();
    while let Some(pos) = parent.rfind('/') {
        parent = &path[..pos];
        if state.select_path(parent) {
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

    fn test_app_with_repo() -> (tempfile::TempDir, git2::Repository, App) {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"first", "First commit");
        add_file_commit(&repo, "b.txt", b"second", "Second commit");
        add_file_commit(&repo, "a.txt", b"third", "Third commit");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let app = App::new(git_repo, Config::default()).unwrap();
        (dir, repo, app)
    }

    #[test]
    fn test_check_repo_changed_noop_without_changes() {
        let (_dir, _repo, mut app) = test_app_with_repo();
        assert!(!app.check_repo_changed());
    }

    #[test]
    fn test_check_repo_changed_detects_external_commit() {
        let (_dir, repo, mut app) = test_app_with_repo();
        add_file_commit(&repo, "c.txt", b"new", "External commit");
        assert!(app.check_repo_changed());
        // Snapshot updated: second call is a no-op
        assert!(!app.check_repo_changed());
    }

    #[test]
    fn test_apply_repo_refresh_shows_new_commit_and_preserves_selection() {
        let (_dir, repo, mut app) = test_app_with_repo();
        // Select "Second commit" (index 1)
        app.handle_key(KeyCode::Char('j'));
        let selected_oid = {
            let Mode::Pick(state) = &app.mode else {
                panic!("expected pick mode")
            };
            state.commits[state.filtered_indices[state.selected]].id
        };

        add_file_commit(&repo, "c.txt", b"new", "External commit");
        assert!(app.check_repo_changed());
        app.apply_repo_refresh();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 4);
        // Same commit still selected, shifted down by the new commit
        assert_eq!(
            state.commits[state.filtered_indices[state.selected]].id,
            selected_oid
        );
        assert_eq!(state.selected, 2);
        assert!(!app.repo_changed);
    }

    #[test]
    fn test_apply_repo_refresh_clamps_selection_after_history_rewrite() {
        let (_dir, repo, mut app) = test_app_with_repo();
        // Selection stays on newest commit (index 0)
        let first_commit_oid = {
            let Mode::Pick(state) = &app.mode else {
                panic!("expected pick mode")
            };
            state.commits[2].id
        };
        // Hard-reset to the oldest commit: the selected (newest) oid disappears
        let obj = repo.find_object(first_commit_oid, None).unwrap();
        repo.reset(&obj, git2::ResetType::Hard, None).unwrap();

        assert!(app.check_repo_changed());
        app.apply_repo_refresh();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 1);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_apply_repo_refresh_keeps_active_filter() {
        let (_dir, repo, mut app) = test_app_with_repo();
        // Filter to "second" → 1 match
        app.handle_key(KeyCode::Char('/'));
        for c in "second".chars() {
            app.handle_key(KeyCode::Char(c));
        }
        app.handle_key(KeyCode::Enter);

        add_file_commit(&repo, "d.txt", b"x", "Second helping");
        assert!(app.check_repo_changed());
        app.apply_repo_refresh();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        // Filter still applied and re-run over the new list: both "Second*" match
        assert_eq!(state.filtered_indices.len(), 2);
        assert_eq!(state.commits.len(), 4);
    }

    #[test]
    fn test_back_applies_pending_refresh() {
        let (_dir, repo, mut app) = test_app_with_repo();
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));

        add_file_commit(&repo, "c.txt", b"new", "External commit");
        assert!(app.check_repo_changed());
        app.repo_changed = true; // as poll_repo_watch sets it outside Pick

        app.handle_key(KeyCode::Esc);
        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 4);
        assert!(!app.repo_changed);
    }

    #[test]
    fn test_poll_repo_watch_refreshes_immediately_in_pick() {
        let (_dir, repo, mut app) = test_app_with_repo();
        add_file_commit(&repo, "c.txt", b"new", "External commit");
        app.last_head_check = std::time::Instant::now() - HEAD_POLL_INTERVAL;

        app.poll_repo_watch();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 4);
        assert!(!app.repo_changed);
    }

    #[test]
    fn test_poll_repo_watch_defers_refresh_outside_pick() {
        let (_dir, repo, mut app) = test_app_with_repo();
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.mode, Mode::View(_)));

        add_file_commit(&repo, "c.txt", b"new", "External commit");
        app.last_head_check = std::time::Instant::now() - HEAD_POLL_INTERVAL;

        app.poll_repo_watch();

        // Viewed content untouched; only the flag is raised
        assert!(app.repo_changed);
        assert!(matches!(app.mode, Mode::View(_)));
    }

    #[test]
    fn test_poll_repo_watch_respects_interval() {
        let (_dir, repo, mut app) = test_app_with_repo();
        add_file_commit(&repo, "c.txt", b"new", "External commit");
        // last_head_check was just set in App::new → within the interval
        app.poll_repo_watch();

        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 3);
        assert!(!app.repo_changed);
    }

    #[test]
    fn test_poll_repo_watch_retries_after_failed_refresh() {
        let (dir, repo, mut app) = test_app_with_repo();
        add_file_commit(&repo, "c.txt", b"new", "External commit");
        // Corrupt the object store so CommitStore::new fails while HEAD
        // refs remain readable.
        let objects = dir.path().join(".git").join("objects");
        let backup = dir.path().join("objects-backup");
        std::fs::rename(&objects, &backup).unwrap();

        app.last_head_check = std::time::Instant::now() - HEAD_POLL_INTERVAL;
        app.poll_repo_watch();
        // Refresh failed: list unchanged, snapshot rolled back for retry.
        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 3);

        std::fs::rename(&backup, &objects).unwrap();
        app.last_head_check = std::time::Instant::now() - HEAD_POLL_INTERVAL;
        app.poll_repo_watch();
        let Mode::Pick(state) = &app.mode else {
            panic!("expected pick mode")
        };
        assert_eq!(state.commits.len(), 4);
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

    #[test]
    fn test_ctrl_c_closes_search_modal() {
        let (_dir, mut app) = test_app();
        app.search_modal.open();
        assert!(app.search_modal.is_open());
        app.handle_ctrl_key(KeyCode::Char('c'));
        assert!(!app.search_modal.is_open());
        assert!(!app.should_quit);
    }

    #[test]
    fn test_ctrl_n_in_modal_moves_down() {
        use crate::search::modal_state::ModalState;
        use crate::search::{DocKind, DocMeta, SearchResult};
        let (_dir, mut app) = test_app();
        let results: Vec<SearchResult> = (0..5)
            .map(|i| SearchResult {
                score: 0.0,
                meta: DocMeta {
                    doc_id: i,
                    kind: DocKind::File,
                    title: format!("file{}.rs", i),
                    commit_oid: String::new(),
                    path: None,
                    line_start: None,
                    line_end: None,
                },
            })
            .collect();
        app.search_modal.state = ModalState::Results {
            input: "test".into(),
            results,
        };
        assert_eq!(app.search_modal.selected, 0);
        app.handle_ctrl_key(KeyCode::Char('n'));
        assert_eq!(app.search_modal.selected, 1);
    }

    #[test]
    fn test_ctrl_p_in_modal_moves_up() {
        use crate::search::modal_state::ModalState;
        use crate::search::{DocKind, DocMeta, SearchResult};
        let (_dir, mut app) = test_app();
        let results: Vec<SearchResult> = (0..5)
            .map(|i| SearchResult {
                score: 0.0,
                meta: DocMeta {
                    doc_id: i,
                    kind: DocKind::File,
                    title: format!("file{}.rs", i),
                    commit_oid: String::new(),
                    path: None,
                    line_start: None,
                    line_end: None,
                },
            })
            .collect();
        app.search_modal.state = ModalState::Results {
            input: "test".into(),
            results,
        };
        app.search_modal.selected = 2;
        app.handle_ctrl_key(KeyCode::Char('p'));
        assert_eq!(app.search_modal.selected, 1);
    }

    #[test]
    fn test_ctrl_n_in_modal_does_not_move_pick() {
        use crate::search::modal_state::ModalState;
        use crate::search::{DocKind, DocMeta, SearchResult};
        let (_dir, mut app) = test_app();
        let results: Vec<SearchResult> = (0..3)
            .map(|i| SearchResult {
                score: 0.0,
                meta: DocMeta {
                    doc_id: i,
                    kind: DocKind::File,
                    title: format!("file{}.rs", i),
                    commit_oid: String::new(),
                    path: None,
                    line_start: None,
                    line_end: None,
                },
            })
            .collect();
        app.search_modal.state = ModalState::Results {
            input: "test".into(),
            results,
        };
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, 0);
        let _ = s;
        app.handle_ctrl_key(KeyCode::Char('n'));
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, 0, "pick mode cursor should not move");
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
    fn test_scroll_up_in_view_does_not_underflow() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"line1\nline2\nline3\n", "A");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);

        app.handle_key(KeyCode::Char('u'));
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

    // ── Force index / modal ──

    #[test]
    fn test_i_key_in_pick_opens_indexing_modal() {
        use crate::search::modal_state::ModalState;
        let (_dir, mut app) = test_app();
        assert!(matches!(app.mode, Mode::Pick(_)));
        assert!(!app.search_modal.is_open());
        app.handle_key(KeyCode::Char('I'));
        assert!(app.search_modal.is_open());
        assert!(matches!(app.search_modal.state, ModalState::Loading { .. }));
    }

    #[test]
    fn test_typing_i_in_search_modal_does_not_trigger_reindex() {
        use crate::search::modal_state::ModalState;
        let (_dir, mut app) = test_app();
        app.search_modal.open();
        assert!(matches!(app.search_modal.state, ModalState::Typing { .. }));

        app.handle_key(KeyCode::Char('h'));
        assert!(matches!(app.search_modal.state, ModalState::Typing { .. }));
        assert_eq!(app.search_modal.state.input(), "h");

        app.handle_key(KeyCode::Char('i'));
        assert!(
            !matches!(app.search_modal.state, ModalState::Loading { .. }),
            "typing 'i' in Typing state must not trigger reindex (Loading state)",
        );
        assert_eq!(app.search_modal.state.input(), "hi");
        assert!(
            app.index_rx.is_none(),
            "no indexing thread should be spawned"
        );
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

    // ── File tree folding ──

    fn test_app_with_dirs() -> (tempfile::TempDir, App) {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"root", "Root file");
        add_file_commit(&repo, "src/main.rs", b"fn main() {}", "Add main");
        add_file_commit(&repo, "src/lib.rs", b"pub fn lib() {}", "Add lib");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let app = App::new(git_repo, Config::default()).unwrap();
        (dir, app)
    }

    fn select_view_path(app: &mut App, path: &str) {
        let Mode::View(state) = &mut app.mode else {
            panic!("expected view mode")
        };
        assert!(state.select_path(path), "path {} not found", path);
    }

    #[test]
    fn test_enter_toggles_directory_fold() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        select_view_path(&mut app, "src");

        let full_len = {
            let Mode::View(s) = &app.mode else {
                panic!("expected view")
            };
            s.visible.len()
        };

        app.handle_key(KeyCode::Enter);
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(s.collapsed.contains("src"));
        assert!(s.visible.len() < full_len);
        assert_eq!(s.selected_entry().unwrap().path, "src");
        let _ = s;

        app.handle_key(KeyCode::Enter);
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(!s.collapsed.contains("src"));
        assert_eq!(s.visible.len(), full_len);
    }

    // ── Changed-file jump (H/L) ──

    fn view_selected_path(app: &App) -> String {
        let Mode::View(state) = &app.mode else {
            panic!("expected view mode")
        };
        state.selected_entry().unwrap().path.clone()
    }

    #[test]
    fn test_shift_l_jumps_to_next_changed_file() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        // HEAD commit "Add lib" changed src/lib.rs only.
        app.handle_key(KeyCode::Char('L'));
        assert_eq!(view_selected_path(&app), "src/lib.rs");

        // No further change below: selection stays put.
        app.handle_key(KeyCode::Char('L'));
        assert_eq!(view_selected_path(&app), "src/lib.rs");
    }

    #[test]
    fn test_shift_h_jumps_back_to_previous_changed_file() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        select_view_path(&mut app, "src/main.rs");

        app.handle_key(KeyCode::Char('H'));
        assert_eq!(view_selected_path(&app), "src/lib.rs");
    }

    #[test]
    fn test_shift_l_expands_collapsed_dir_to_reach_change() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        select_view_path(&mut app, "src");
        app.handle_key(KeyCode::Char('h'));
        for _ in 0..5 {
            app.handle_key(KeyCode::Char('k'));
        }

        app.handle_key(KeyCode::Char('L'));
        assert_eq!(view_selected_path(&app), "src/lib.rs");
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(!s.collapsed.contains("src"));
    }

    #[test]
    fn test_shift_l_without_changes_is_noop() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"root", "Root commit");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);

        // Root commit has no parent, so nothing is marked changed. L must
        // neither move the selection nor scroll the content.
        app.handle_key(KeyCode::Char('L'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.selected_file, 0);
        assert_eq!(s.scroll, 0);
    }

    #[test]
    fn test_shift_j_pages_view_content() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        select_view_path(&mut app, "src/lib.rs");
        app.handle_key(KeyCode::Enter);

        // J/K page the content pane again; the single-line file clamps
        // scroll to the last line instead of moving the tree selection.
        app.handle_key(KeyCode::Char('J'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(view_selected_path(&app), "src/lib.rs");
        assert_eq!(s.scroll, s.line_count().saturating_sub(1));

        app.handle_key(KeyCode::Char('K'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.scroll, 0);
    }

    #[test]
    fn test_h_collapses_expanded_directory() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        select_view_path(&mut app, "src");

        app.handle_key(KeyCode::Char('h'));
        let Mode::View(s) = &app.mode else {
            panic!("expected view, h on dir must not go back")
        };
        assert!(s.collapsed.contains("src"));
    }

    #[test]
    fn test_h_on_collapsed_top_level_dir_goes_back() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        select_view_path(&mut app, "src");
        app.handle_key(KeyCode::Char('h')); // collapse
        app.handle_key(KeyCode::Char('h')); // no parent → Back
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    #[test]
    fn test_h_on_file_goes_back() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        select_view_path(&mut app, "a.txt");
        app.handle_key(KeyCode::Char('h'));
        assert!(matches!(app.mode, Mode::Pick(_)));
    }

    #[test]
    fn test_l_expands_collapsed_directory_then_steps_in() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        select_view_path(&mut app, "src");
        app.handle_key(KeyCode::Char('h')); // collapse

        app.handle_key(KeyCode::Char('l')); // expand
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(!s.collapsed.contains("src"));
        assert_eq!(s.selected_entry().unwrap().path, "src");
        let _ = s;

        app.handle_key(KeyCode::Char('l')); // step into first child
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(s.selected_entry().unwrap().path.starts_with("src/"));
    }

    #[test]
    fn test_fold_survives_commit_switch() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        select_view_path(&mut app, "src");
        app.handle_key(KeyCode::Char('h')); // collapse src

        app.handle_ctrl_key(KeyCode::Char('n')); // older commit
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(s.collapsed.contains("src"));
    }

    // ── Debounced file loading ──

    fn expire_pending_load(app: &mut App) {
        assert!(app.pending_view_load.is_some(), "expected a pending load");
        app.pending_view_load = Some(std::time::Instant::now() - VIEW_LOAD_DEBOUNCE);
        app.tick_pending_view_load();
    }

    #[test]
    fn test_view_navigation_defers_file_load() {
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        // First file loads immediately on entering View
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(matches!(
            s.file_content,
            crate::mode::FileContent::Text { .. }
        ));
        let _ = s;

        select_view_path(&mut app, "src/lib.rs");
        app.handle_key(KeyCode::Char('j')); // → src/main.rs, uncached → debounced
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.selected_entry().unwrap().path, "src/main.rs");
        assert!(matches!(s.file_content, crate::mode::FileContent::Loading));
        assert!(app.pending_view_load.is_some());
        let _ = s;

        expire_pending_load(&mut app);
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let crate::mode::FileContent::Text { raw, .. } = &s.file_content else {
            panic!("expected text content after the debounce elapsed")
        };
        assert!(raw.contains("fn main"));
        assert!(app.pending_view_load.is_none());
    }

    #[test]
    fn test_view_navigation_cache_hit_is_instant() {
        let (dir, repo) = init_test_repo();
        add_file_commit(&repo, "a.txt", b"aaa", "A");
        add_file_commit(&repo, "b.txt", b"bbb", "B");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter); // View: a.txt loaded + cached

        app.handle_key(KeyCode::Char('j')); // b.txt: debounced
        expire_pending_load(&mut app); // loads + caches b.txt

        app.handle_key(KeyCode::Char('k')); // back to a.txt: cache hit
        assert!(
            app.pending_view_load.is_none(),
            "cached file must not debounce"
        );
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let crate::mode::FileContent::Text { raw, .. } = &s.file_content else {
            panic!("expected cached text content")
        };
        assert!(raw.contains("aaa"));
    }

    #[test]
    fn test_large_file_skips_highlighting() {
        let (dir, repo) = init_test_repo();
        let big = "fn main() { println!(\"hi\"); }\n".repeat(10_000); // ~300KB
        add_file_commit(&repo, "big.rs", big.as_bytes(), "Add big file");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);

        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        let crate::mode::FileContent::Text { highlighted, .. } = &s.file_content else {
            panic!("expected text content")
        };
        assert!(!highlighted.is_empty());
        assert!(
            highlighted
                .iter()
                .flat_map(|line| line.spans.iter())
                .all(|span| span.style.fg.is_none()),
            "oversized file must render as plain text"
        );
    }

    // ── Mouse ──

    fn mouse(
        kind: crossterm::event::MouseEventKind,
        col: u16,
        row: u16,
    ) -> crossterm::event::MouseEvent {
        crossterm::event::MouseEvent {
            kind,
            column: col,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }
    }

    #[test]
    fn test_mouse_wheel_moves_pick_selection() {
        use crossterm::event::MouseEventKind;
        let (_dir, mut app) = test_app();
        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 5, 5));
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, 1);
        let _ = s;
        app.handle_mouse(mouse(MouseEventKind::ScrollUp, 5, 5));
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick")
        };
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn test_mouse_click_selects_and_folds_tree_entry() {
        use crossterm::event::{MouseButton, MouseEventKind};
        use ratatui::layout::Rect;
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        app.view_tree_area = Some(Rect::new(0, 0, 30, 10));
        app.view_tree_offset = 0;

        let src_row = {
            let Mode::View(s) = &app.mode else {
                panic!("expected view")
            };
            let idx = s
                .visible
                .iter()
                .position(|&i| s.tree[i].path == "src")
                .unwrap();
            1 + idx as u16 // +1 for the top border
        };

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, src_row));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.selected_entry().unwrap().path, "src");
        assert!(s.collapsed.contains("src"), "click on dir should fold it");
        let _ = s;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, src_row));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert!(!s.collapsed.contains("src"), "second click should unfold");
    }

    #[test]
    fn test_mouse_click_outside_tree_area_ignored() {
        use crossterm::event::{MouseButton, MouseEventKind};
        use ratatui::layout::Rect;
        let (_dir, mut app) = test_app_with_dirs();
        app.handle_key(KeyCode::Enter);
        app.view_tree_area = Some(Rect::new(0, 0, 30, 10));
        let before = {
            let Mode::View(s) = &app.mode else {
                panic!("expected view")
            };
            s.selected_file
        };
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 50, 5));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.selected_file, before);
    }

    #[test]
    fn test_mouse_double_click_opens_view() {
        use crossterm::event::{MouseButton, MouseEventKind};
        use ratatui::layout::Rect;
        let (_dir, mut app) = test_app();
        app.pick_list_area = Some(Rect::new(0, 0, 40, 10));
        app.pick_list_offset = 0;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, 2));
        let Mode::Pick(s) = &app.mode else {
            panic!("expected pick after single click")
        };
        assert_eq!(s.selected, 1);
        let _ = s;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, 2));
        assert!(matches!(app.mode, Mode::View(_)));
    }

    #[test]
    fn test_mouse_wheel_scrolls_view_content() {
        use crossterm::event::MouseEventKind;
        use ratatui::layout::Rect;
        let (dir, repo) = init_test_repo();
        let content = (0..50).map(|i| format!("line {}\n", i)).collect::<String>();
        add_file_commit(&repo, "a.txt", content.as_bytes(), "A");
        let git_repo = GitRepo::open(dir.path()).unwrap();
        let mut app = App::new(git_repo, Config::default()).unwrap();
        app.handle_key(KeyCode::Enter);
        app.view_tree_area = Some(Rect::new(0, 0, 30, 10));
        app.view_content_area = Some(Rect::new(30, 0, 50, 10));

        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 40, 5));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.scroll, 3);
        let _ = s;
        app.handle_mouse(mouse(MouseEventKind::ScrollUp, 40, 5));
        let Mode::View(s) = &app.mode else {
            panic!("expected view")
        };
        assert_eq!(s.scroll, 0);
    }

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
