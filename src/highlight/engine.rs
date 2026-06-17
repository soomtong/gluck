use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::collections::HashMap;
use std::sync::OnceLock;
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
            theme: HashMap::new(),
        };
        engine.register_languages();
        engine
    }

    pub fn set_theme(&mut self, theme: HashMap<String, Style>) {
        self.theme = theme;
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
        crate::lang::Language::from_path(path)
            .map(|l| l.as_str().to_string())
            .unwrap_or_default()
    }

    fn register_languages(&mut self) {
        if let Ok(config) = Self::make_rust_config() {
            self.configs.insert("rust".to_string(), config);
        }
        if let Ok(config) = Self::make_markdown_config() {
            self.configs.insert("markdown".to_string(), config);
        }
        if let Ok(config) = Self::make_json_config() {
            self.configs.insert("json".to_string(), config);
        }
        if let Ok(config) = Self::make_typescript_config() {
            self.configs.insert("typescript".to_string(), config);
        }
        if let Ok(config) = Self::make_tsx_config() {
            self.configs.insert("tsx".to_string(), config);
        }
        if let Ok(config) = Self::make_javascript_config() {
            self.configs.insert("javascript".to_string(), config);
        }
        if let Ok(config) = Self::make_yaml_config() {
            self.configs.insert("yaml".to_string(), config);
        }
        if let Ok(config) = Self::make_swift_config() {
            self.configs.insert("swift".to_string(), config);
        }
    }

    fn make_rust_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_rust::LANGUAGE.into(),
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

    fn make_json_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_json::LANGUAGE.into(),
            "json",
            JSON_HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_typescript_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            ts_with_js_keywords(),
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_tsx_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            "tsx",
            ts_with_js_keywords(),
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_javascript_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_yaml_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_yaml::language(),
            "yaml",
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_swift_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_swift::LANGUAGE.into(),
            "swift",
            SWIFT_HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }
}

fn ts_with_js_keywords() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    Q.get_or_init(|| {
        format!(
            "{}\n{}",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        )
    })
}

pub const HIGHLIGHT_NAMES: &[&str] = &[
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
    "number",
    "boolean",
    "constant.builtin",
    "label",
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

const JSON_HIGHLIGHTS_QUERY: &str = r#"
(pair
  key: (_) @property)

(string) @string

(number) @number

[
  (null)
  (true)
  (false)
] @constant.builtin

(escape_sequence) @string.escape

(comment) @comment
"#;

const SWIFT_HIGHLIGHTS_QUERY: &str = r#"[
  "."
  ";"
  ":"
  ","
] @punctuation.delimiter

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
  "<"
  ">"
] @punctuation.bracket

(type_identifier) @type

[
  (self_expression)
  (super_expression)
] @variable.builtin

(simple_identifier) @variable

(function_declaration
  name: (simple_identifier) @function)

(protocol_function_declaration
  name: (simple_identifier) @function)

(init_declaration
  "init" @function)

(parameter
  external_name: (simple_identifier) @variable.parameter)

(parameter
  name: (simple_identifier) @variable.parameter)

(type_parameter
  (type_identifier) @variable.parameter)

[
  "protocol"
  "extension"
  "indirect"
  "nonisolated"
  "override"
  "convenience"
  "required"
  "some"
  "any"
  "weak"
  "unowned"
  "didSet"
  "willSet"
  "subscript"
  "let"
  "var"
  (throws)
  (where_keyword)
  (getter_specifier)
  (setter_specifier)
  (modify_specifier)
  (else)
  (as_operator)
  "func"
  "deinit"
  "enum"
  "struct"
  "class"
  "typealias"
  "async"
  "await"
  "import"
  "if"
  "guard"
  "switch"
  "case"
  "for"
  "while"
  "repeat"
  "continue"
  "break"
  "return"
  "do"
  (throw_keyword)
  (catch_keyword)
  "in"
] @keyword

(class_body
  (property_declaration
    (pattern
      (simple_identifier) @variable)))

(protocol_property_declaration
  (pattern
    (simple_identifier) @variable))

(navigation_expression
  (navigation_suffix
    (simple_identifier) @variable))

(value_argument
  name: (value_argument_label
    (simple_identifier) @variable))

(modifiers
  (attribute
    "@" @attribute
    (user_type
      (type_identifier) @attribute)))

(call_expression
  (simple_identifier) @function)

(call_expression
  (navigation_expression
    (navigation_suffix
      (simple_identifier) @function)))

(call_expression
  (prefix_expression
    (simple_identifier) @function))

(directive) @keyword

[
  (comment)
  (multiline_comment)
] @comment

(line_str_text) @string

(str_escaped_char) @string.escape

(multi_line_str_text) @string

(raw_str_part) @string

(raw_str_end_part) @string

[
  "\""
  "\"\"\""
] @string

(line_string_literal
  [
    "\\("
    ")"
  ] @punctuation.special)

(multi_line_string_literal
  [
    "\\("
    ")"
  ] @punctuation.special)

(raw_str_interpolation
  [
    (raw_str_interpolation_start)
    ")"
  ] @punctuation.special)

[
  (integer_literal)
  (hex_literal)
  (oct_literal)
  (bin_literal)
  (real_literal)
] @number

(boolean_literal) @boolean

"nil" @constant.builtin

(custom_operator) @operator

[
  "+"
  "-"
  "*"
  "/"
  "%"
  "="
  "+="
  "-="
  "*="
  "/="
  "<"
  ">"
  "<<"
  ">>"
  "<="
  ">="
  "++"
  "--"
  "^"
  "&"
  "&&"
  "|"
  "||"
  "~"
  "%="
  "!="
  "!=="
  "=="
  "==="
  "?"
  "??"
  "->"
  "..<"
  "..."
  (bang)
] @operator
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight("# Title\n**bold** text\n", "readme.md");
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in markdown highlight output");
    }

    #[test]
    fn test_highlight_without_theme_produces_no_colors() {
        let mut engine = HighlightEngine::new();
        let lines = engine.highlight("fn main() {}", "main.rs");
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(!has_color, "expected no colors without theme set");
    }

    #[test]
    fn test_typescript_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "const greet = (name: string): string => `hi ${name}`;\n",
            "app.ts",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in typescript highlight output");
    }

    #[test]
    fn test_tsx_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "export const App = ({ label }: { label: string }) => <div>{label}</div>;\n",
            "App.tsx",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in tsx highlight output");
    }

    #[test]
    fn test_javascript_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "export const greet = (name) => `hi ${name}`;\nfunction add(a, b) { return a + b; }\n",
            "app.js",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in javascript highlight output");
    }

    #[test]
    fn test_typescript_keywords_are_colored() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "if (x) { return 1; } else { return 2; }\nfunction f(): void {}\n",
            "a.ts",
        );
        let keyword_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                ["if", "return", "else", "function"].contains(&s.content.as_ref())
                    && s.style.fg.is_some()
            })
            .collect();
        assert!(
            keyword_spans.len() >= 4,
            "expected `if`/`return`/`else`/`function` to be keyword-colored, got spans: {:#?}",
            lines
        );
    }

    #[test]
    fn test_tsx_keywords_are_colored() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "if (x) { return 1; } else { return 2; }\nconst C = () => <div>hi</div>;\n",
            "C.tsx",
        );
        let keyword_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                ["if", "return", "else", "const"].contains(&s.content.as_ref())
                    && s.style.fg.is_some()
            })
            .collect();
        assert!(
            keyword_spans.len() >= 4,
            "expected `if`/`return`/`else`/`const` to be keyword-colored in tsx, got spans: {:#?}",
            lines
        );
    }

    #[test]
    fn test_json_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            r#"{"name": "gluck", "count": 42, "active": true}"#,
            "config.json",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in json highlight output");
    }

    #[test]
    fn test_yaml_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "name: gluck\ncount: 42\nactive: true\n",
            "config.yaml",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in yaml highlight output");
    }

    #[test]
    fn test_swift_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "import Foundation\n\nstruct Point {\n    let x: Int\n    func distance() -> Double { return 0.0 }\n}\n",
            "Point.swift",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in swift highlight output");
    }
}
