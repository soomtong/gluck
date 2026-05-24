use crate::search::SearchResult;
use crossterm::event::KeyCode;

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

    pub fn handle_key(&mut self, code: KeyCode) -> bool {
        match &self.state {
            ModalState::Closed => return false,
            ModalState::Loading { .. } => {
                match code {
                    KeyCode::Esc => self.close(),
                    KeyCode::Char('I') | KeyCode::Char('i') => {}
                    _ => return false,
                }
            }
            _ => {}
        }
        match code {
            KeyCode::Esc => self.close(),
            KeyCode::Backspace => {
                self.pop_char();
            }
            KeyCode::Down => {
                self.move_down();
            }
            KeyCode::Up => {
                self.move_up();
            }
            KeyCode::Enter => {}
            KeyCode::Char(c) => {
                self.push_char(c);
            }
            _ => return false,
        }
        true
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

    #[test]
    fn test_handle_key_closed_returns_false() {
        let mut m = SemanticSearchModal::new();
        assert!(!m.handle_key(KeyCode::Esc));
        assert!(!m.handle_key(KeyCode::Char('a')));
        assert!(!m.handle_key(KeyCode::Enter));
        assert!(!m.handle_key(KeyCode::Down));
    }

    #[test]
    fn test_handle_key_typing() {
        let mut m = SemanticSearchModal::new();
        m.open();
        assert!(m.handle_key(KeyCode::Char('h')));
        assert!(m.handle_key(KeyCode::Char('i')));
        assert_eq!(m.state.input(), "hi");
        assert!(m.handle_key(KeyCode::Backspace));
        assert_eq!(m.state.input(), "h");
        assert!(m.handle_key(KeyCode::Esc));
        assert!(matches!(m.state, ModalState::Closed));
    }

    #[test]
    fn test_handle_key_loading_only_esc_and_i() {
        let mut m = SemanticSearchModal::new();
        m.set_loading("Loading...");
        assert!(m.handle_key(KeyCode::Esc));
        assert!(matches!(m.state, ModalState::Closed));

        m.set_loading("Loading...");
        assert!(m.handle_key(KeyCode::Char('I')));
        assert!(m.handle_key(KeyCode::Char('i')));
        assert!(!m.handle_key(KeyCode::Char('x')));
        assert!(!m.handle_key(KeyCode::Down));
    }
}
