use crossterm::event::KeyCode;
use super::SearchResult;

#[derive(Debug, Clone, PartialEq)]
pub enum Section {
    Files,
    Commits,
}

#[derive(Debug, Clone)]
pub struct SemanticSearchModal {
    pub input: String,
    pub file_results: Vec<SearchResult>,
    pub commit_results: Vec<SearchResult>,
    pub selected: usize,
    pub focused_section: Section,
    pub active: bool,
    pub warning: Option<String>,   // stale index 경고
    pub no_index: bool,            // 인덱스 없음
    pub incompatible: bool,        // 버전 불일치
}

#[derive(Debug, Clone)]
pub enum ModalAction {
    None,
    Close,
    Navigate(SearchResult),
}

impl SemanticSearchModal {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            file_results: vec![],
            commit_results: vec![],
            selected: 0,
            focused_section: Section::Files,
            active: false,
            warning: None,
            no_index: false,
            incompatible: false,
        }
    }

    pub fn open(
        &mut self,
        is_available: bool,
        is_stale: bool,
        is_incompatible: bool,
    ) {
        self.active = true;
        self.input.clear();
        self.file_results.clear();
        self.commit_results.clear();
        self.selected = 0;
        self.focused_section = Section::Files;
        self.warning = None;
        self.no_index = false;
        self.incompatible = false;

        if !is_available {
            self.no_index = true;
        } else if is_incompatible {
            self.incompatible = true;
        } else if is_stale {
            self.warning = Some("Index may be stale — run `glc index` to refresh.".to_string());
        }
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn handle_key(
        &mut self,
        code: KeyCode,
        search_fn: impl FnOnce(&str) -> Vec<SearchResult>,
    ) -> ModalAction {
        match code {
            KeyCode::Esc => {
                self.close();
                ModalAction::Close
            }
            KeyCode::Enter => {
                let result = self.selected_result().cloned();
                self.close();
                match result {
                    Some(r) => ModalAction::Navigate(r),
                    None => ModalAction::Close,
                }
            }
            KeyCode::Tab => {
                self.toggle_section();
                ModalAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                ModalAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                ModalAction::None
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.update_results(search_fn);
                ModalAction::None
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                self.update_results(search_fn);
                ModalAction::None
            }
            _ => ModalAction::None,
        }
    }

    fn update_results(&mut self, search_fn: impl FnOnce(&str) -> Vec<SearchResult>) {
        if self.input.is_empty() || self.no_index || self.incompatible {
            self.file_results.clear();
            self.commit_results.clear();
            self.selected = 0;
            return;
        }
        let all = search_fn(&self.input);
        self.file_results = all.iter()
            .filter(|r| r.kind == super::DocKind::File)
            .cloned()
            .collect();
        self.commit_results = all.iter()
            .filter(|r| r.kind == super::DocKind::Commit)
            .cloned()
            .collect();
        self.selected = 0;
    }

    fn current_section_results(&self) -> &[SearchResult] {
        match self.focused_section {
            Section::Files => &self.file_results,
            Section::Commits => &self.commit_results,
        }
    }

    fn move_down(&mut self) {
        let max = self.current_section_results().len().saturating_sub(1);
        if self.current_section_results().is_empty() { return; }
        self.selected = (self.selected + 1).min(max);
    }

    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn toggle_section(&mut self) {
        self.focused_section = match self.focused_section {
            Section::Files => Section::Commits,
            Section::Commits => Section::Files,
        };
        self.selected = 0;
    }

    pub fn selected_result(&self) -> Option<&SearchResult> {
        self.current_section_results().get(self.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::DocKind;

    fn make_result(id: u64, kind: DocKind, title: &str) -> SearchResult {
        let kind_clone = kind.clone();
        SearchResult {
            doc_id: id,
            kind,
            title: title.to_string(),
            path: if kind_clone == DocKind::File { Some(title.to_string()) } else { None },
            commit_oid: if kind_clone == DocKind::Commit { Some("abc".to_string()) } else { None },
            score: 1.0,
        }
    }

    fn no_search(_: &str) -> Vec<SearchResult> { vec![] }

    fn some_results(_: &str) -> Vec<SearchResult> {
        vec![
            make_result(1, DocKind::File, "src/main.rs"),
            make_result(2, DocKind::File, "src/lib.rs"),
            make_result(3, DocKind::Commit, "Fix bug"),
        ]
    }

    #[test]
    fn test_modal_opens_with_no_index() {
        let mut m = SemanticSearchModal::new();
        m.open(false, false, false);
        assert!(m.active);
        assert!(m.no_index);
    }

    #[test]
    fn test_modal_opens_with_incompatible_index() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, true);
        assert!(m.active);
        assert!(m.incompatible);
    }

    #[test]
    fn test_modal_opens_with_stale_warning() {
        let mut m = SemanticSearchModal::new();
        m.open(true, true, false);
        assert!(m.active);
        assert!(m.warning.is_some());
    }

    #[test]
    fn test_esc_closes() {
        let mut m = SemanticSearchModal::new();
        m.active = true;
        let action = m.handle_key(KeyCode::Esc, no_search);
        assert!(!m.active);
        assert!(matches!(action, ModalAction::Close));
    }

    #[test]
    fn test_tab_toggles_section() {
        let mut m = SemanticSearchModal::new();
        m.active = true;
        assert_eq!(m.focused_section, Section::Files);
        m.handle_key(KeyCode::Tab, no_search);
        assert_eq!(m.focused_section, Section::Commits);
        m.handle_key(KeyCode::Tab, no_search);
        assert_eq!(m.focused_section, Section::Files);
    }

    #[test]
    fn test_typing_updates_input_and_calls_search() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, false);
        m.handle_key(KeyCode::Char('h'), some_results);
        m.handle_key(KeyCode::Char('i'), some_results);
        assert_eq!(m.input, "hi");
        assert_eq!(m.file_results.len(), 2);
        assert_eq!(m.commit_results.len(), 1);
    }

    #[test]
    fn test_backspace_updates_input() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, false);
        m.handle_key(KeyCode::Char('a'), no_search);
        m.handle_key(KeyCode::Char('b'), no_search);
        m.handle_key(KeyCode::Backspace, no_search);
        assert_eq!(m.input, "a");
    }

    #[test]
    fn test_navigation_bounds() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, false);
        m.handle_key(KeyCode::Down, no_search);
        assert_eq!(m.selected, 0);
        m.handle_key(KeyCode::Up, no_search);
        assert_eq!(m.selected, 0);
    }

    #[test]
    fn test_navigate_to_result_closes_modal() {
        let mut m = SemanticSearchModal::new();
        m.open(true, false, false);
        m.handle_key(KeyCode::Char('x'), some_results);
        assert_eq!(m.file_results.len(), 2);

        let action = m.handle_key(KeyCode::Enter, no_search);
        assert!(!m.active);
        assert!(matches!(action, ModalAction::Navigate(_)));
    }

    #[test]
    fn test_enter_empty_returns_close() {
        let mut m = SemanticSearchModal::new();
        m.active = true;
        let action = m.handle_key(KeyCode::Enter, no_search);
        assert!(matches!(action, ModalAction::Close));
    }
}
