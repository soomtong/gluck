use crate::search::SearchResult;

#[derive(Debug, Clone)]
pub enum ModalState {
    Idle,
    Typing {
        input: String,
    },
    Loading {
        input: String,
    },
    Results {
        input: String,
        results: Vec<SearchResult>,
    },
    NoIndex,
    Indexing {
        message: String,
    },
}

impl ModalState {
    pub fn input(&self) -> &str {
        match self {
            ModalState::Typing { input } => input,
            ModalState::Loading { input } => input,
            ModalState::Results { input, .. } => input,
            _ => "",
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModalAction {
    Close,
    SelectResult(usize),
    OpenBuildIndex,
}

pub struct SemanticSearchModal {
    pub state: ModalState,
    pub selected: usize,
}

impl SemanticSearchModal {
    pub fn new() -> Self {
        Self {
            state: ModalState::Idle,
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
        self.state = ModalState::Idle;
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

    pub fn set_no_index(&mut self) {
        self.state = ModalState::NoIndex;
    }

    pub fn set_indexing(&mut self, message: impl Into<String>) {
        self.state = ModalState::Indexing {
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
        !matches!(self.state, ModalState::Idle { .. })
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
        assert!(!m.is_open());
        m.open();
        assert!(m.is_open());
        m.close();
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
}
