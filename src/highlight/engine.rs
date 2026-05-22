use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::HashMap;
use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

impl Default for HighlightEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct HighlightEngine {
    configs: HashMap<String, HighlightConfiguration>,
    theme: HashMap<String, Style>,
}

impl HighlightEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            configs: HashMap::new(),
            theme: default_theme(),
        };
        engine.register_languages();
        engine
    }

    pub fn highlight(&mut self, source: &str, path: &str) -> Vec<Line<'static>> {
        let lang = Self::detect_language(path);
        let config = match self.configs.get(&lang) {
            Some(c) => c,
            None => return Self::plain_lines(source),
        };

        let mut highlighter = Highlighter::new();
        let events = match highlighter.highlight(config, source.as_bytes(), None, |_| None) {
            Ok(e) => e,
            Err(_) => return Self::plain_lines(source),
        };

        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut current_spans: Vec<Span<'static>> = Vec::new();
        let mut current_style = Style::new();
        let mut source_iter = source.bytes();
        let mut byte_pos = 0;

        for event in events {
            match event {
                Ok(HighlightEvent::HighlightStart(h)) => {
                    current_style = self
                        .theme
                        .get(HIGHLIGHT_NAMES.get(h.0).copied().unwrap_or_default())
                        .copied()
                        .unwrap_or_default();
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    current_style = Style::new();
                }
                Ok(HighlightEvent::Source { start, end }) => {
                    while byte_pos < start {
                        source_iter.next();
                        byte_pos += 1;
                    }
                    let len = end - start;
                    let mut buf = Vec::with_capacity(len);
                    while byte_pos < end {
                        if let Some(b) = source_iter.next() {
                            buf.push(b);
                            byte_pos += 1;
                        } else {
                            break;
                        }
                    }
                    let text = String::from_utf8_lossy(&buf).into_owned();
                    if text.contains('\n') {
                        let parts: Vec<&str> = text.split('\n').collect();
                        for (i, part) in parts.iter().enumerate() {
                            if i > 0 {
                                lines.push(Line::from(std::mem::take(&mut current_spans)));
                            }
                            if !part.is_empty() {
                                current_spans.push(Span::styled(part.to_string(), current_style));
                            }
                        }
                    } else if !text.is_empty() {
                        current_spans.push(Span::styled(text, current_style));
                    }
                }
                Err(_) => break,
            }
        }

        if !current_spans.is_empty() {
            lines.push(Line::from(current_spans));
        }

        if lines.is_empty() {
            Self::plain_lines(source)
        } else {
            lines
        }
    }

    fn plain_lines(source: &str) -> Vec<Line<'static>> {
        source.lines().map(|l| Line::from(l.to_string())).collect()
    }

    fn detect_language(path: &str) -> String {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        match ext {
            "rs" => "rust",
            "py" => "python",
            "js" | "mjs" => "javascript",
            "ts" => "typescript",
            "go" => "go",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" => "cpp",
            "java" => "java",
            "sh" | "bash" => "bash",
            "toml" => "toml",
            "json" => "json",
            "md" => "markdown",
            "html" => "html",
            "css" => "css",
            _ => ext,
        }
        .to_string()
    }

    fn register_languages(&mut self) {
        if let Ok(config) = Self::make_rust_config() {
            self.configs.insert("rust".to_string(), config);
        }
        if let Ok(config) = Self::make_markdown_config() {
            self.configs.insert("markdown".to_string(), config);
        }
    }

    fn make_rust_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let raw_fn = tree_sitter_rust::LANGUAGE.into_raw();
        let raw_ptr = unsafe { raw_fn() };
        let language = unsafe { Language::from_raw(raw_ptr as *const _) };
        let mut config = HighlightConfiguration::new(
            language,
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_markdown_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let language = tree_sitter_markdown_fork::language();
        let mut config =
            HighlightConfiguration::new(language, "markdown", MARKDOWN_HIGHLIGHTS_QUERY, "", "")?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }
}

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "constant",
    "function.builtin",
    "function",
    "keyword",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "comment",
    "text.title",
    "text.literal",
    "text.emphasis",
    "text.strong",
    "text.uri",
    "text.reference",
    "punctuation.special",
    "string.escape",
    "none",
];

const MARKDOWN_HIGHLIGHTS_QUERY: &str = r#"[
  (atx_heading)
  (setext_heading)
] @text.title

(code_fence_content) @none

[
  (indented_code_block)
  (fenced_code_block)
  (code_span)
] @text.literal

(emphasis) @text.emphasis

(strong_emphasis) @text.strong

(link_destination) @text.uri

[
  (backslash_escape)
  (hard_line_break)
] @string.escape
"#;

fn default_theme() -> HashMap<String, Style> {
    let mut theme = HashMap::new();
    theme.insert(
        "keyword".into(),
        Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
    );
    theme.insert("function".into(), Style::new().fg(Color::Blue));
    theme.insert("function.builtin".into(), Style::new().fg(Color::Cyan));
    theme.insert("string".into(), Style::new().fg(Color::Green));
    theme.insert("string.special".into(), Style::new().fg(Color::Cyan));
    theme.insert("comment".into(), Style::new().fg(Color::DarkGray));
    theme.insert("type".into(), Style::new().fg(Color::Cyan));
    theme.insert("type.builtin".into(), Style::new().fg(Color::Cyan));
    theme.insert("constant".into(), Style::new().fg(Color::Yellow));
    theme.insert("variable".into(), Style::new().fg(Color::White));
    theme.insert("variable.builtin".into(), Style::new().fg(Color::Cyan));
    theme.insert("variable.parameter".into(), Style::new().fg(Color::White));
    theme.insert("operator".into(), Style::new().fg(Color::Yellow));
    theme.insert("punctuation".into(), Style::new().fg(Color::DarkGray));
    theme.insert(
        "punctuation.bracket".into(),
        Style::new().fg(Color::DarkGray),
    );
    theme.insert(
        "punctuation.delimiter".into(),
        Style::new().fg(Color::DarkGray),
    );
    theme.insert("property".into(), Style::new().fg(Color::White));
    theme.insert("attribute".into(), Style::new().fg(Color::Yellow));
    theme.insert("tag".into(), Style::new().fg(Color::Cyan));
    theme.insert(
        "text.title".into(),
        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    );
    theme.insert("text.literal".into(), Style::new().fg(Color::Green));
    theme.insert("text.emphasis".into(), Style::new().fg(Color::Magenta));
    theme.insert(
        "text.strong".into(),
        Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
    );
    theme.insert(
        "text.uri".into(),
        Style::new()
            .fg(Color::Cyan)
            .add_modifier(Modifier::UNDERLINED),
    );
    theme.insert("text.reference".into(), Style::new().fg(Color::Cyan));
    theme.insert(
        "punctuation.special".into(),
        Style::new().fg(Color::DarkGray),
    );
    theme.insert("string.escape".into(), Style::new().fg(Color::Yellow));
    theme
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        let lines = engine.highlight("# Title\n**bold** text\n", "readme.md");
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in markdown highlight output");
    }
}
