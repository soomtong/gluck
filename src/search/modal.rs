use crate::search::SearchResult;

#[derive(Debug, Clone)]
pub enum ModalState {
    Closed,
    Typing {
        input: String,
    },
    Loading {
        input: String,
        message: String,
    },
    Results {
        input: String,
        results: Vec<SearchResult>,
    },
}

impl ModalState {
    pub fn input(&self) -> &str {
        match self {
            ModalState::Typing { input }
            | ModalState::Loading { input, .. }
            | ModalState::Results { input, .. } => input,
            ModalState::Closed => "",
        }
    }
}

pub struct SemanticSearchModal {
    pub state: ModalState,
    pub selected: usize,
}

impl SemanticSearchModal {
    pub fn new() -> Self {
        Self {
            state: ModalState::Closed,
            selected: 0,
        }
    }

    pub fn open(&mut self) {
        self.state = ModalState::Typing {
            input: String::new(),
        };
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.state = ModalState::Closed;
        self.selected = 0;
    }

    pub fn push_char(&mut self, c: char) {
        if let ModalState::Typing { input } | ModalState::Results { input, .. } = &mut self.state {
            input.push(c);
        }
    }

    pub fn pop_char(&mut self) {
        if let ModalState::Typing { input } | ModalState::Results { input, .. } = &mut self.state {
            input.pop();
        }
    }

    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        let input = self.state.input().to_string();
        self.state = ModalState::Results { input, results };
        self.selected = 0;
    }

    pub fn set_loading(&mut self, message: impl Into<String>) {
        let input = self.state.input().to_string();
        self.state = ModalState::Loading {
            input,
            message: message.into(),
        };
    }

    pub fn move_down(&mut self) {
        if let ModalState::Results { results, .. } = &self.state {
            let max = results.len().saturating_sub(1);
            self.selected = (self.selected + 1).min(max);
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn is_open(&self) -> bool {
        !matches!(self.state, ModalState::Closed)
    }

    pub fn results(&self) -> &[SearchResult] {
        if let ModalState::Results { results, .. } = &self.state {
            results
        } else {
            &[]
        }
    }
}

impl Default for SemanticSearchModal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modal_open_close() {
        let mut m = SemanticSearchModal::new();
        assert!(matches!(m.state, ModalState::Closed));
        assert!(!m.is_open());
        m.open();
        assert!(matches!(m.state, ModalState::Typing { .. }));
        assert!(m.is_open());
        m.close();
        assert!(matches!(m.state, ModalState::Closed));
        assert!(!m.is_open());
    }

    #[test]
    fn test_push_pop_char() {
        let mut m = SemanticSearchModal::new();
        m.open();
        m.push_char('a');
        m.push_char('b');
        assert_eq!(m.state.input(), "ab");
        m.pop_char();
        assert_eq!(m.state.input(), "a");
    }

    #[test]
    fn test_set_loading_preserves_input() {
        let mut m = SemanticSearchModal::new();
        m.open();
        m.push_char('t');
        m.push_char('e');
        m.push_char('s');
        m.push_char('t');
        m.set_loading("Indexing...");
        assert!(matches!(m.state, ModalState::Loading { .. }));
        assert_eq!(m.state.input(), "test");
    }
}
