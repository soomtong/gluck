use crate::git::commit::CommitInfo;
use crate::git::diff::DiffResult;
use crate::git::tree::FileEntry;
use ratatui::text::Line;

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Pick(PickState),
    View(ViewState),
    Diff(DiffState),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickState {
    pub commits: Vec<CommitInfo>,
    pub filtered_indices: Vec<usize>,
    pub selected: usize,
    pub scroll: usize,
    pub query: Option<String>,
}

impl PickState {
    pub fn new(commits: Vec<CommitInfo>) -> Self {
        let filtered_indices = (0..commits.len()).collect();
        Self {
            commits,
            filtered_indices,
            selected: 0,
            scroll: 0,
            query: None,
        }
    }

    pub fn visible_commits(&self) -> Vec<&CommitInfo> {
        self.filtered_indices
            .iter()
            .map(|&i| &self.commits[i])
            .collect()
    }

    pub fn apply_search(&mut self, query: &str) {
        use crate::git::commit::search_commits;
        self.query = if query.is_empty() {
            None
        } else {
            Some(query.to_string())
        };
        self.filtered_indices = if query.is_empty() {
            (0..self.commits.len()).collect()
        } else {
            search_commits(&self.commits, query)
        };
        self.selected = 0;
        self.scroll = 0;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    pub commit: CommitInfo,
    pub tree: Vec<FileEntry>,
    pub selected_file: usize,
    pub content: Option<String>,
    pub highlighted: Vec<Line<'static>>,
    pub scroll: usize,
    pub show_ignored: bool,
}

impl ViewState {
    pub fn new(commit: CommitInfo, tree: Vec<FileEntry>) -> Self {
        Self {
            commit,
            tree,
            selected_file: 0,
            content: None,
            highlighted: Vec::new(),
            scroll: 0,
            show_ignored: true,
        }
    }
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
    Quit,
    ToggleView,
    SwitchMode,
    NextCommit,
    PrevCommit,
    ToggleGitignore,
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
        bindings.insert(KeyCode::Char('k'), Action::MoveUp);
        bindings.insert(KeyCode::Up, Action::MoveUp);
        bindings.insert(KeyCode::Enter, Action::Enter);
        bindings.insert(KeyCode::Char('l'), Action::Enter);
        bindings.insert(KeyCode::Esc, Action::Back);
        bindings.insert(KeyCode::Char('h'), Action::Back);
        bindings.insert(KeyCode::Char('/'), Action::Search);
        bindings.insert(KeyCode::Char('q'), Action::Quit);
        bindings.insert(KeyCode::Char('s'), Action::ToggleView);
        bindings.insert(KeyCode::Tab, Action::SwitchMode);
        bindings.insert(KeyCode::Char('J'), Action::PrevCommit);
        bindings.insert(KeyCode::Char('K'), Action::NextCommit);
        bindings.insert(KeyCode::Char('.'), Action::ToggleGitignore);
        Self { bindings }
    }

    pub fn resolve(&self, code: crossterm::event::KeyCode) -> Option<Action> {
        self.bindings.get(&code).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

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
    fn test_pick_state_search() {
        let commits = vec![
            CommitInfo {
                id: git2::Oid::zero(),
                short_id: "abc1234".into(),
                author: "Alice".into(),
                date: std::time::UNIX_EPOCH,
                message: "Add auth module".into(),
            },
            CommitInfo {
                id: git2::Oid::zero(),
                short_id: "def5678".into(),
                author: "Bob".into(),
                date: std::time::UNIX_EPOCH,
                message: "Fix login bug".into(),
            },
        ];
        let mut state = PickState::new(commits);
        state.apply_search("auth");
        assert_eq!(state.filtered_indices.len(), 1);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_pick_state_clear_search() {
        let commits = vec![
            CommitInfo {
                id: git2::Oid::zero(),
                short_id: "abc".into(),
                author: "A".into(),
                date: std::time::UNIX_EPOCH,
                message: "First".into(),
            },
            CommitInfo {
                id: git2::Oid::zero(),
                short_id: "def".into(),
                author: "B".into(),
                date: std::time::UNIX_EPOCH,
                message: "Second".into(),
            },
        ];
        let mut state = PickState::new(commits);
        state.apply_search("first");
        assert_eq!(state.filtered_indices.len(), 1);
        state.apply_search("");
        assert_eq!(state.filtered_indices.len(), 2);
        assert!(state.query.is_none());
    }
}