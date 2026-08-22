use crate::git::commit::CommitInfo;
use crate::git::diff::DiffResult;
use crate::git::tree::FileEntry;
use ratatui::text::Line;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Pick(PickState),
    View(ViewState),
    Diff(DiffState),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchState {
    Idle { query: Option<String> },
    Active { input: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickState {
    pub commits: Arc<Vec<CommitInfo>>,
    pub filtered_indices: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
    pub search: SearchState,
    pub selected_diff: Option<DiffResult>,
}

impl PickState {
    pub fn new(commits: Arc<Vec<CommitInfo>>) -> Self {
        let filtered_indices = (0..commits.len()).collect();
        Self {
            commits,
            filtered_indices,
            selected: 0,
            scroll: 0,
            search: SearchState::Idle { query: None },
            selected_diff: None,
        }
    }

    pub fn visible_commits(&self) -> Vec<&CommitInfo> {
        self.filtered_indices
            .iter()
            .map(|&i| &self.commits[i])
            .collect()
    }

    pub fn query(&self) -> Option<&str> {
        match &self.search {
            SearchState::Idle { query } => query.as_deref(),
            SearchState::Active { input } => {
                if input.is_empty() {
                    None
                } else {
                    Some(input)
                }
            }
        }
    }

    pub fn update_filter(&mut self, query: &str) {
        let q = query.to_lowercase();
        self.filtered_indices = if query.is_empty() {
            (0..self.commits.len()).collect()
        } else {
            self.commits
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    c.message.to_lowercase().contains(&q)
                        || c.author.to_lowercase().contains(&q)
                        || c.short_id.starts_with(&q)
                })
                .map(|(i, _)| i)
                .collect()
        };
        self.selected = 0;
        self.scroll = 0;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileContent {
    NotLoaded,
    Loading,
    Binary,
    Text {
        raw: String,
        highlighted: Vec<Line<'static>>,
    },
}

/// `tree` is the full (possibly gitignore-filtered) DFS-ordered entry list;
/// `visible` holds indices into `tree` after applying `collapsed` folds and
/// `selected_file` indexes into `visible`, not `tree`.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    pub commit: CommitInfo,
    pub tree: Vec<FileEntry>,
    pub collapsed: std::collections::HashSet<String>,
    pub visible: Vec<usize>,
    pub selected_file: usize,
    pub file_content: FileContent,
    pub scroll: usize,
    pub show_ignored: bool,
    pub changed_paths: std::collections::HashSet<String>,
    pub changed_stats: std::collections::HashMap<String, (usize, usize)>,
}

impl ViewState {
    pub fn new(commit: CommitInfo, tree: Vec<FileEntry>) -> Self {
        let visible = (0..tree.len()).collect();
        Self {
            commit,
            tree,
            collapsed: std::collections::HashSet::new(),
            visible,
            selected_file: 0,
            file_content: FileContent::NotLoaded,
            scroll: 0,
            show_ignored: true,
            changed_paths: std::collections::HashSet::new(),
            changed_stats: std::collections::HashMap::new(),
        }
    }

    pub fn line_count(&self) -> usize {
        match &self.file_content {
            FileContent::NotLoaded => 0,
            FileContent::Loading => 0,
            FileContent::Binary => 1,
            FileContent::Text { highlighted, raw } => {
                if !highlighted.is_empty() {
                    highlighted.len()
                } else {
                    raw.lines().count()
                }
            }
        }
    }

    /// Recompute `visible` from `tree` and `collapsed`, clamping the selection.
    pub fn rebuild_visible(&mut self) {
        self.visible = compute_visible(&self.tree, &self.collapsed);
        self.selected_file = self.selected_file.min(self.visible.len().saturating_sub(1));
    }

    pub fn visible_entry(&self, vis_idx: usize) -> Option<&FileEntry> {
        self.visible.get(vis_idx).and_then(|&i| self.tree.get(i))
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.visible_entry(self.selected_file)
    }

    /// Move selection to the visible entry with this exact path.
    pub fn select_visible_path(&mut self, path: &str) -> bool {
        let pos = self.visible.iter().position(|&i| self.tree[i].path == path);
        if let Some(pos) = pos {
            self.selected_file = pos;
            true
        } else {
            false
        }
    }

    /// Select the entry with this path, expanding collapsed ancestors so it
    /// becomes visible. Returns false when the path is not in the tree.
    pub fn select_path(&mut self, path: &str) -> bool {
        let mut expanded = false;
        let mut parent = path;
        while let Some(pos) = parent.rfind('/') {
            parent = &path[..pos];
            if self.collapsed.remove(parent) {
                expanded = true;
            }
        }
        if expanded {
            self.rebuild_visible();
        }
        self.select_visible_path(path)
    }

    /// Move selection to the parent directory of the selected entry.
    pub fn select_parent(&mut self) -> bool {
        let Some(entry) = self.selected_entry() else {
            return false;
        };
        let path = entry.path.clone();
        let Some(pos) = path.rfind('/') else {
            return false;
        };
        self.select_visible_path(&path[..pos])
    }

    /// Fold/unfold the selected directory. Returns false when the selection
    /// is not a directory.
    pub fn toggle_fold(&mut self) -> bool {
        let Some(entry) = self.selected_entry() else {
            return false;
        };
        if !matches!(entry.kind, crate::git::tree::EntryKind::Directory) {
            return false;
        }
        let path = entry.path.clone();
        if !self.collapsed.remove(&path) {
            self.collapsed.insert(path.clone());
        }
        self.rebuild_visible();
        self.select_visible_path(&path);
        true
    }

    fn is_changed_file(&self, entry: &FileEntry) -> bool {
        matches!(entry.kind, crate::git::tree::EntryKind::File)
            && self.changed_paths.contains(&entry.path)
    }

    /// Path of the next changed file after the selection, in tree (DFS)
    /// order. Scans the full tree so files hidden inside collapsed
    /// directories are found too.
    pub fn next_changed_path(&self) -> Option<String> {
        let cur = self.visible.get(self.selected_file).copied()?;
        self.tree
            .iter()
            .skip(cur + 1)
            .find(|e| self.is_changed_file(e))
            .map(|e| e.path.clone())
    }

    /// Path of the previous changed file before the selection, in tree order.
    pub fn prev_changed_path(&self) -> Option<String> {
        let cur = self.visible.get(self.selected_file).copied()?;
        self.tree[..cur]
            .iter()
            .rev()
            .find(|e| self.is_changed_file(e))
            .map(|e| e.path.clone())
    }
}

/// Entries under a collapsed directory are hidden. The tree is DFS-ordered
/// with each directory preceding its contents, so a single active skip
/// prefix is enough.
fn compute_visible(
    tree: &[FileEntry],
    collapsed: &std::collections::HashSet<String>,
) -> Vec<usize> {
    let mut visible = Vec::with_capacity(tree.len());
    let mut skip: Option<&str> = None;
    for (i, entry) in tree.iter().enumerate() {
        if let Some(prefix) = skip {
            if entry.path.len() > prefix.len()
                && entry.path.starts_with(prefix)
                && entry.path.as_bytes()[prefix.len()] == b'/'
            {
                continue;
            }
            skip = None;
        }
        visible.push(i);
        if matches!(entry.kind, crate::git::tree::EntryKind::Directory)
            && collapsed.contains(&entry.path)
        {
            skip = Some(&entry.path);
        }
    }
    visible
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffState {
    pub from: CommitInfo,
    pub to: CommitInfo,
    pub diff_result: DiffResult,
    pub selected_file: usize,
    pub side_by_side: bool,
    pub scroll: usize,
    pub prev_view_file: usize,
}

impl DiffState {
    pub fn new(from: CommitInfo, to: CommitInfo, diff_result: DiffResult) -> Self {
        Self {
            from,
            to,
            diff_result,
            selected_file: 0,
            side_by_side: true,
            scroll: 0,
            prev_view_file: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    MoveDown,
    MoveUp,
    Enter,
    Back,
    Search,
    SemanticSearch,
    ForceIndex,
    Quit,
    ToggleView,
    SwitchMode,
    PageDown,
    PageUp,
    ToggleGitignore,
    ScrollDown,
    ScrollUp,
}

#[derive(Debug, Clone)]
pub struct KeyBindings {
    pub bindings: std::collections::HashMap<crossterm::event::KeyCode, Action>,
}

impl KeyBindings {
    pub fn default_bindings() -> Self {
        use crossterm::event::KeyCode;
        let mut bindings = std::collections::HashMap::new();
        bindings.insert(KeyCode::Char('j'), Action::MoveDown);
        bindings.insert(KeyCode::Down, Action::MoveDown);
        bindings.insert(KeyCode::Right, Action::MoveDown);
        bindings.insert(KeyCode::Char('k'), Action::MoveUp);
        bindings.insert(KeyCode::Up, Action::MoveUp);
        bindings.insert(KeyCode::Left, Action::MoveUp);
        bindings.insert(KeyCode::Enter, Action::Enter);
        bindings.insert(KeyCode::Char('l'), Action::Enter);
        bindings.insert(KeyCode::Esc, Action::Back);
        bindings.insert(KeyCode::Char('h'), Action::Back);
        bindings.insert(KeyCode::Char('/'), Action::Search);
        bindings.insert(KeyCode::Char('q'), Action::Quit);
        bindings.insert(KeyCode::Char('v'), Action::ToggleView);
        bindings.insert(KeyCode::Tab, Action::SwitchMode);
        bindings.insert(KeyCode::Char('K'), Action::PageUp);
        bindings.insert(KeyCode::Char('J'), Action::PageDown);
        bindings.insert(KeyCode::Char('.'), Action::ToggleGitignore);
        bindings.insert(KeyCode::Char('u'), Action::ScrollUp);
        bindings.insert(KeyCode::Char('d'), Action::ScrollDown);
        bindings.insert(KeyCode::Char('s'), Action::SemanticSearch);
        bindings.insert(KeyCode::Char('I'), Action::ForceIndex);
        Self { bindings }
    }

    pub fn resolve(&self, code: crossterm::event::KeyCode) -> Option<Action> {
        self.bindings.get(&code).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tree::EntryKind;
    use crossterm::event::KeyCode;

    fn make_commit(msg: &str) -> CommitInfo {
        CommitInfo {
            id: git2::Oid::ZERO_SHA1,
            short_id: "abc1234".into(),
            author: "Test".into(),
            date: std::time::UNIX_EPOCH,
            message: msg.into(),
        }
    }

    // ── KeyBindings ──

    #[test]
    fn test_keybindings_resolve() {
        let kb = KeyBindings::default_bindings();
        assert_eq!(kb.resolve(KeyCode::Char('j')), Some(Action::MoveDown));
        assert_eq!(kb.resolve(KeyCode::Down), Some(Action::MoveDown));
        assert_eq!(kb.resolve(KeyCode::Char('k')), Some(Action::MoveUp));
        assert_eq!(kb.resolve(KeyCode::Enter), Some(Action::Enter));
        assert_eq!(kb.resolve(KeyCode::Esc), Some(Action::Back));
        assert_eq!(kb.resolve(KeyCode::Char('x')), None);
    }

    #[test]
    fn test_keybindings_all_actions_mapped() {
        let kb = KeyBindings::default_bindings();
        assert!(
            kb.bindings
                .values()
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 5
        );
        assert!(kb.bindings.contains_key(&KeyCode::Char('q')));
        assert!(kb.bindings.contains_key(&KeyCode::Char('/')));
        assert!(kb.bindings.contains_key(&KeyCode::Tab));
    }

    // ── SearchState ──

    #[test]
    fn test_search_state_active_idle_transition() {
        let active = SearchState::Active {
            input: "auth".into(),
        };
        let query = match &active {
            SearchState::Active { input } if !input.is_empty() => Some(input.clone()),
            _ => None,
        };
        let idle = SearchState::Idle { query };
        match idle {
            SearchState::Idle { query: Some(q) } => assert_eq!(q, "auth"),
            _ => panic!("expected Idle with query"),
        }
    }

    #[test]
    fn test_search_state_active_empty_input() {
        let state = SearchState::Active {
            input: String::new(),
        };
        match &state {
            SearchState::Active { input } => assert!(input.is_empty()),
            _ => panic!("expected Active"),
        }
    }

    #[test]
    fn test_search_state_idle_no_query() {
        let state = SearchState::Idle { query: None };
        assert!(matches!(state, SearchState::Idle { query: None }));
    }

    #[test]
    fn test_search_state_cancel_preserves_query() {
        let state = SearchState::Active {
            input: "test".into(),
        };
        let query = match &state {
            SearchState::Active { input } if !input.is_empty() => Some(input.clone()),
            _ => None,
        };
        let idle = SearchState::Idle { query };
        match idle {
            SearchState::Idle { query: Some(q) } => assert_eq!(q, "test"),
            _ => panic!("expected Idle with query"),
        }
    }

    // ── PickState ──

    #[test]
    fn test_pick_state_new_initial_state() {
        let commits = Arc::new(vec![make_commit("A"), make_commit("B")]);
        let state = PickState::new(commits);
        assert_eq!(state.filtered_indices, vec![0, 1]);
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll, 0);
        assert!(matches!(state.search, SearchState::Idle { query: None }));
        assert!(state.selected_diff.is_none());
    }

    #[test]
    fn test_pick_state_visible_commits_matches_filter() {
        let commits = Arc::new(vec![
            make_commit("Alpha"),
            make_commit("Beta"),
            make_commit("Gamma"),
        ]);
        let mut state = PickState::new(commits);
        state.update_filter("a");
        assert_eq!(state.visible_commits().len(), state.filtered_indices.len());
    }

    #[test]
    fn test_pick_state_filter_no_matches() {
        let commits = Arc::new(vec![make_commit("Alpha"), make_commit("Beta")]);
        let mut state = PickState::new(commits);
        state.update_filter("zzz");
        assert!(state.filtered_indices.is_empty());
        assert!(state.visible_commits().is_empty());
    }

    #[test]
    fn test_pick_state_filter_resets_selection() {
        let commits = Arc::new(vec![make_commit("Alpha"), make_commit("Beta")]);
        let mut state = PickState::new(commits);
        state.selected = 1;
        state.scroll = 5;
        state.update_filter("Alpha");
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn test_pick_state_empty_commits() {
        let state = PickState::new(Arc::new(vec![]));
        assert!(state.filtered_indices.is_empty());
        assert!(state.visible_commits().is_empty());
    }

    #[test]
    fn test_pick_state_filter_case_insensitive() {
        let commits = Arc::new(vec![make_commit("Hello World"), make_commit("goodbye")]);
        let mut state = PickState::new(commits);
        state.update_filter("HELLO");
        assert_eq!(state.filtered_indices.len(), 1);
    }

    // ── FileContent ──

    #[test]
    fn test_file_content_not_loaded_line_count() {
        let state = ViewState::new(make_commit("C"), vec![]);
        assert_eq!(state.line_count(), 0);
    }

    #[test]
    fn test_file_content_binary_line_count() {
        let mut state = ViewState::new(make_commit("C"), vec![]);
        state.file_content = FileContent::Binary;
        assert_eq!(state.line_count(), 1);
    }

    #[test]
    fn test_file_content_text_line_count_prefers_highlighted() {
        let mut state = ViewState::new(make_commit("C"), vec![]);
        state.file_content = FileContent::Text {
            raw: "line1\nline2\nline3\n".into(),
            highlighted: vec![Line::from("a"), Line::from("b")],
        };
        assert_eq!(state.line_count(), 2);
    }

    #[test]
    fn test_file_content_text_line_count_falls_back_to_raw() {
        let mut state = ViewState::new(make_commit("C"), vec![]);
        state.file_content = FileContent::Text {
            raw: "line1\nline2\nline3".into(),
            highlighted: vec![],
        };
        assert_eq!(state.line_count(), 3);
    }

    // ── Folding ──

    fn make_entry(path: &str, kind: EntryKind) -> FileEntry {
        let name = path.rsplit('/').next().unwrap().to_string();
        FileEntry {
            name,
            path: path.into(),
            kind,
        }
    }

    fn folded_tree() -> Vec<FileEntry> {
        vec![
            make_entry("a.txt", EntryKind::File),
            make_entry("src", EntryKind::Directory),
            make_entry("src/nested", EntryKind::Directory),
            make_entry("src/nested/deep.rs", EntryKind::File),
            make_entry("src/main.rs", EntryKind::File),
            make_entry("z.txt", EntryKind::File),
        ]
    }

    #[test]
    fn test_visible_defaults_to_full_tree() {
        let state = ViewState::new(make_commit("C"), folded_tree());
        assert_eq!(state.visible, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_toggle_fold_hides_and_restores_children() {
        let mut state = ViewState::new(make_commit("C"), folded_tree());
        state.selected_file = 1; // src
        assert!(state.toggle_fold());
        // src stays visible, everything under it is hidden
        assert_eq!(state.visible, vec![0, 1, 5]);
        assert_eq!(state.selected_entry().unwrap().path, "src");

        assert!(state.toggle_fold());
        assert_eq!(state.visible, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(state.selected_entry().unwrap().path, "src");
    }

    #[test]
    fn test_toggle_fold_on_file_is_noop() {
        let mut state = ViewState::new(make_commit("C"), folded_tree());
        state.selected_file = 0; // a.txt
        assert!(!state.toggle_fold());
        assert_eq!(state.visible.len(), 6);
    }

    #[test]
    fn test_nested_collapse_inside_expanded_parent() {
        let mut state = ViewState::new(make_commit("C"), folded_tree());
        state.selected_file = 2; // src/nested
        assert!(state.toggle_fold());
        // Only deep.rs is hidden
        assert_eq!(state.visible, vec![0, 1, 2, 4, 5]);
    }

    #[test]
    fn test_collapse_does_not_hide_sibling_prefix_dir() {
        // "src" collapsed must not hide "src2/..." (prefix but not a child)
        let tree = vec![
            make_entry("src", EntryKind::Directory),
            make_entry("src/main.rs", EntryKind::File),
            make_entry("src2", EntryKind::Directory),
            make_entry("src2/other.rs", EntryKind::File),
        ];
        let mut state = ViewState::new(make_commit("C"), tree);
        state.selected_file = 0;
        state.toggle_fold();
        assert_eq!(state.visible, vec![0, 2, 3]);
    }

    #[test]
    fn test_select_path_expands_collapsed_ancestors() {
        let mut state = ViewState::new(make_commit("C"), folded_tree());
        state.selected_file = 1; // src
        state.toggle_fold();
        assert_eq!(state.visible.len(), 3);

        assert!(state.select_path("src/nested/deep.rs"));
        assert_eq!(state.selected_entry().unwrap().path, "src/nested/deep.rs");
        assert!(!state.collapsed.contains("src"));
    }

    #[test]
    fn test_select_path_missing_returns_false() {
        let mut state = ViewState::new(make_commit("C"), folded_tree());
        assert!(!state.select_path("does/not/exist.rs"));
    }

    #[test]
    fn test_select_parent() {
        let mut state = ViewState::new(make_commit("C"), folded_tree());
        state.selected_file = 3; // src/nested/deep.rs
        assert!(state.select_parent());
        assert_eq!(state.selected_entry().unwrap().path, "src/nested");
        assert!(state.select_parent());
        assert_eq!(state.selected_entry().unwrap().path, "src");
        // Top-level entry has no parent
        assert!(!state.select_parent());
    }

    // ── Changed-file jump ──

    fn changed_tree_state() -> ViewState {
        let mut state = ViewState::new(make_commit("C"), folded_tree());
        state.changed_paths.insert("src/nested/deep.rs".into());
        state.changed_paths.insert("z.txt".into());
        state
    }

    #[test]
    fn test_next_changed_path_skips_unchanged_entries() {
        let state = changed_tree_state(); // selection on a.txt
        assert_eq!(
            state.next_changed_path().as_deref(),
            Some("src/nested/deep.rs")
        );
    }

    #[test]
    fn test_next_changed_path_moves_past_current_change() {
        let mut state = changed_tree_state();
        state.selected_file = 3; // src/nested/deep.rs
        assert_eq!(state.next_changed_path().as_deref(), Some("z.txt"));
        state.selected_file = 5; // z.txt, last change
        assert_eq!(state.next_changed_path(), None);
    }

    #[test]
    fn test_prev_changed_path() {
        let mut state = changed_tree_state();
        state.selected_file = 5; // z.txt
        assert_eq!(
            state.prev_changed_path().as_deref(),
            Some("src/nested/deep.rs")
        );
        state.selected_file = 0; // a.txt
        assert_eq!(state.prev_changed_path(), None);
    }

    #[test]
    fn test_next_changed_path_finds_files_under_collapsed_dir() {
        let mut state = changed_tree_state();
        state.collapsed.insert("src".into());
        state.rebuild_visible(); // visible: a.txt, src, z.txt
        state.selected_file = 0;
        assert_eq!(
            state.next_changed_path().as_deref(),
            Some("src/nested/deep.rs")
        );
    }

    #[test]
    fn test_changed_directory_is_not_a_jump_target() {
        let mut state = ViewState::new(make_commit("C"), folded_tree());
        state.changed_paths.insert("src".into());
        assert_eq!(state.next_changed_path(), None);
    }

    #[test]
    fn test_rebuild_visible_clamps_selection() {
        let mut state = ViewState::new(make_commit("C"), folded_tree());
        state.selected_file = 5; // z.txt
        state.collapsed.insert("src".into());
        state.rebuild_visible();
        assert!(state.selected_file < state.visible.len());
    }

    #[test]
    fn test_view_state_new_defaults() {
        let commit = make_commit("C");
        let tree = vec![
            FileEntry {
                name: "src/".into(),
                path: "src".into(),
                kind: EntryKind::Directory,
            },
            FileEntry {
                name: "a.rs".into(),
                path: "src/a.rs".into(),
                kind: EntryKind::File,
            },
        ];
        let state = ViewState::new(commit.clone(), tree);
        assert_eq!(state.commit, commit);
        assert_eq!(state.tree.len(), 2);
        assert_eq!(state.selected_file, 0);
        assert!(matches!(state.file_content, FileContent::NotLoaded));
        assert_eq!(state.scroll, 0);
        assert!(state.show_ignored);
        assert!(state.changed_paths.is_empty());
    }

    // ── DiffState ──

    #[test]
    fn test_diff_state_new_defaults() {
        let from = make_commit("From");
        let to = make_commit("To");
        let diff = DiffResult {
            files: vec![],
            from_id: "abc".into(),
            to_id: "def".into(),
        };
        let state = DiffState::new(from.clone(), to.clone(), diff);
        assert_eq!(state.from, from);
        assert_eq!(state.to, to);
        assert_eq!(state.selected_file, 0);
        assert!(state.side_by_side);
        assert_eq!(state.scroll, 0);
        assert_eq!(state.prev_view_file, 0);
    }
}
