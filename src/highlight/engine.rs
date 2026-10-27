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
        let events = match highlighter.highlight(config, source.as_bytes(), None, None, |_| None) {
            Ok(e) => e,
            Err(_) => return Self::plain_lines(source),
        };

        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut current_spans: Vec<Span<'static>> = Vec::new();
        let mut current_style = Style::new();
        // Captures nest (an escape inside a string, `$` inside a PHP
        // variable); ending the inner one must restore the outer style.
        let mut style_stack: Vec<Style> = Vec::new();
        let mut source_iter = source.bytes();
        let mut byte_pos = 0;

        for event in events {
            match event {
                Ok(HighlightEvent::HighlightStart(h)) => {
                    style_stack.push(current_style);
                    current_style = self
                        .theme
                        .get(HIGHLIGHT_NAMES.get(h.0).copied().unwrap_or_default())
                        .copied()
                        .unwrap_or_default();
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    current_style = style_stack.pop().unwrap_or_default();
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
                    let text = expand_tabs(&String::from_utf8_lossy(&buf));
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

    /// Unstyled per-line rendering; also the fast path for files too large
    /// to highlight.
    pub fn plain_lines(source: &str) -> Vec<Line<'static>> {
        source.lines().map(|l| Line::from(expand_tabs(l))).collect()
    }

    fn detect_language(path: &str) -> String {
        crate::lang::Language::from_path(path)
            .map(|l| l.as_str().to_string())
            .unwrap_or_default()
    }
}

/// ratatui gives '\t' zero display width, so tab indentation silently
/// disappears from rendered spans — expand tabs before building them.
pub fn expand_tabs(text: &str) -> String {
    text.replace('\t', "    ")
}

impl HighlightEngine {
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
        if let Ok(config) = Self::make_bash_config() {
            self.configs.insert("bash".to_string(), config);
        }
        if let Ok(config) = Self::make_zig_config() {
            self.configs.insert("zig".to_string(), config);
        }
        if let Ok(config) = Self::make_c_config() {
            self.configs.insert("c".to_string(), config);
        }
        if let Ok(config) = Self::make_asm_config() {
            self.configs.insert("asm".to_string(), config);
        }
        if let Ok(config) = Self::make_linkerscript_config() {
            self.configs.insert("linkerscript".to_string(), config);
        }
        if let Ok(config) = Self::make_go_config() {
            self.configs.insert("go".to_string(), config);
        }
        if let Ok(config) = Self::make_lisette_config() {
            self.configs.insert("lisette".to_string(), config);
        }
        if let Ok(config) = Self::make_gleam_config() {
            self.configs.insert("gleam".to_string(), config);
        }
        if let Ok(config) = Self::make_python_config() {
            self.configs.insert("python".to_string(), config);
        }
        if let Ok(config) = Self::make_cpp_config() {
            self.configs.insert("cpp".to_string(), config);
        }
        if let Ok(config) = Self::make_java_config() {
            self.configs.insert("java".to_string(), config);
        }
        if let Ok(config) = Self::make_ruby_config() {
            self.configs.insert("ruby".to_string(), config);
        }
        if let Ok(config) = Self::make_lua_config() {
            self.configs.insert("lua".to_string(), config);
        }
        if let Ok(config) = Self::make_kotlin_config() {
            self.configs.insert("kotlin".to_string(), config);
        }
        if let Ok(config) = Self::make_scala_config() {
            self.configs.insert("scala".to_string(), config);
        }
        if let Ok(config) = Self::make_elixir_config() {
            self.configs.insert("elixir".to_string(), config);
        }
        if let Ok(config) = Self::make_erlang_config() {
            self.configs.insert("erlang".to_string(), config);
        }
        if let Ok(config) = Self::make_odin_config() {
            self.configs.insert("odin".to_string(), config);
        }
        if let Ok(config) = Self::make_php_config() {
            self.configs.insert("php".to_string(), config);
        }
        if let Ok(config) = Self::make_llvm_config() {
            self.configs.insert("llvm".to_string(), config);
        }
        if let Ok(config) = Self::make_clojure_config() {
            self.configs.insert("clojure".to_string(), config);
        }
        if let Ok(config) = Self::make_janet_config() {
            self.configs.insert("janet".to_string(), config);
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

    fn make_bash_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_bash::LANGUAGE.into(),
            "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_zig_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_zig::LANGUAGE.into(),
            "zig",
            tree_sitter_zig::HIGHLIGHTS_QUERY,
            tree_sitter_zig::INJECTIONS_QUERY,
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_c_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_c::LANGUAGE.into(),
            "c",
            tree_sitter_c::HIGHLIGHT_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_asm_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_asm::LANGUAGE.into(),
            "asm",
            tree_sitter_asm::HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_linkerscript_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_linkerscript::LANGUAGE.into(),
            "linkerscript",
            tree_sitter_linkerscript::HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_go_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_go::LANGUAGE.into(),
            "go",
            tree_sitter_go::HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_lisette_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_lisette::LANGUAGE.into(),
            "lisette",
            tree_sitter_lisette::HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    // Locals drive the query's `#is-not? local` predicates and parameter
    // tracking; the doc-comment markdown injection is skipped since we never
    // resolve injected languages.
    fn make_gleam_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_gleam::LANGUAGE.into(),
            "gleam",
            tree_sitter_gleam::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_gleam::LOCALS_QUERY,
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_python_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_python::LANGUAGE.into(),
            "python",
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_cpp_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_cpp::LANGUAGE.into(),
            "cpp",
            cpp_highlights(),
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_java_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_java::LANGUAGE.into(),
            "java",
            tree_sitter_java::HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    // Locals keep block parameters and assigned names from being styled as
    // method calls.
    fn make_ruby_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_ruby::LANGUAGE.into(),
            "ruby",
            tree_sitter_ruby::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_ruby::LOCALS_QUERY,
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_lua_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_lua::LANGUAGE.into(),
            "lua",
            tree_sitter_lua::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_lua::LOCALS_QUERY,
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_kotlin_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_kotlin::LANGUAGE.into(),
            "kotlin",
            kotlin_highlights(),
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_scala_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_scala::LANGUAGE.into(),
            "scala",
            tree_sitter_scala::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_scala::LOCALS_QUERY,
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_elixir_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_elixir::LANGUAGE.into(),
            "elixir",
            tree_sitter_elixir::HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_erlang_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_erlang::LANGUAGE.into(),
            "erlang",
            tree_sitter_erlang::HIGHLIGHTS_QUERY,
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_odin_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_odin::LANGUAGE.into(),
            "odin",
            odin_highlights(),
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    // LANGUAGE_PHP (not _ONLY) so the HTML around `<?php` parses as text
    // instead of erroring the whole file.
    fn make_php_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_php::LANGUAGE_PHP.into(),
            "php",
            php_highlights(),
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_llvm_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_llvm::LANGUAGE.into(),
            "llvm",
            llvm_highlights(),
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }

    fn make_clojure_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_clojure::LANGUAGE.into(),
            "clojure",
            clojure_highlights(),
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }
    fn make_janet_config() -> Result<HighlightConfiguration, Box<dyn std::error::Error>> {
        let mut config = HighlightConfiguration::new(
            tree_sitter_janet_simple::LANGUAGE.into(),
            "janet",
            janet_highlights(),
            "",
            "",
        )?;
        config.configure(HIGHLIGHT_NAMES);
        Ok(config)
    }
}

/// Joins query parts once. When two patterns capture the same node the later
/// one wins, so parts appended here override the upstream query.
fn layered(cell: &'static OnceLock<String>, parts: &[&str]) -> &'static str {
    cell.get_or_init(|| parts.join("\n"))
}

/// tree-sitter-cpp only ships the C++ additions; its own tree-sitter.json
/// layers them over tree-sitter-c's query, so do the same here.
fn cpp_highlights() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    layered(
        &Q,
        &[
            tree_sitter_c::HIGHLIGHT_QUERY,
            tree_sitter_cpp::HIGHLIGHT_QUERY,
        ],
    )
}

/// Upstream paints the whole `package` header as a namespace, keyword included.
fn kotlin_highlights() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    layered(
        &Q,
        &[
            tree_sitter_kotlin::HIGHLIGHTS_QUERY,
            r#"(package_header "package" @keyword)"#,
        ],
    )
}

/// Function names are `global_var` nodes, which the trailing `@variable`
/// pattern would otherwise claim.
fn llvm_highlights() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    layered(
        &Q,
        &[
            tree_sitter_llvm::HIGHLIGHTS_QUERY,
            "(function_header name: _ @function)\n(_ callee: _ @function)",
        ],
    )
}

/// Upstream leaves declared type names unstyled.
fn php_highlights() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    layered(
        &Q,
        &[tree_sitter_php::HIGHLIGHTS_QUERY, PHP_EXTRA_HIGHLIGHTS],
    )
}

/// The vendored Lisp queries stop at literals; lists carry no grammar-level
/// meaning, so special forms, call heads and defined names come from here.
fn clojure_highlights() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    layered(
        &Q,
        &[
            tree_sitter_clojure::HIGHLIGHTS_QUERY,
            CLOJURE_EXTRA_HIGHLIGHTS,
        ],
    )
}

fn janet_highlights() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    layered(
        &Q,
        &[
            tree_sitter_janet_simple::HIGHLIGHTS_QUERY,
            JANET_EXTRA_HIGHLIGHTS,
        ],
    )
}

// Order matters: the generic call-head pattern comes first so the special
// form and definition patterns below override it. Definition heads are
// captured as `@keyword`, not a throwaway `@_def` — an unstyled capture would
// still win the node and leave `defn` uncolored.
const CLOJURE_EXTRA_HIGHLIGHTS: &str = r#"
(sym_lit namespace: (sym_ns) @module)

(list_lit . (sym_lit name: (sym_name) @function))

(list_lit . (sym_lit name: (sym_name) @keyword
  (#any-of? @keyword
    "def" "defn" "defn-" "defmacro" "defmulti" "defmethod" "defonce"
    "defprotocol" "defrecord" "deftype" "definterface" "ns" "fn" "fn*"
    "let" "letfn" "loop" "recur" "if" "if-not" "if-let" "if-some" "when"
    "when-not" "when-let" "when-some" "when-first" "cond" "condp" "cond->"
    "cond->>" "case" "do" "doseq" "dotimes" "for" "while" "try" "catch"
    "finally" "throw" "quote" "var" "new" "set!" "binding" "with-open"
    "->" "->>" "as->" "some->" "some->>" "doto" "and" "or" "reify" "proxy"
    "extend-type" "extend-protocol" "comment")))

(list_lit
  . (sym_lit name: (sym_name) @keyword)
  . (sym_lit name: (sym_name) @function)
  (#any-of? @keyword "defn" "defn-" "defmacro" "defmulti" "defmethod"))

(list_lit
  . (sym_lit name: (sym_name) @keyword)
  . (sym_lit name: (sym_name) @type)
  (#any-of? @keyword "defprotocol" "defrecord" "deftype" "definterface"))

(list_lit
  . (sym_lit name: (sym_name) @keyword)
  . (sym_lit) @module
  (#eq? @keyword "ns"))

(regex_lit) @string.special
"#;

const JANET_EXTRA_HIGHLIGHTS: &str = r#"
(par_tup_lit . (sym_lit) @function)

(par_tup_lit . (sym_lit) @keyword
  (#any-of? @keyword
    "def" "def-" "var" "var-" "defn" "defn-" "defmacro" "defmacro-" "varfn"
    "fn" "let" "if" "if-let" "if-not" "when" "when-let" "unless" "cond"
    "case" "match" "do" "while" "for" "forv" "loop" "seq" "generate" "each"
    "eachp" "eachk" "eachy" "repeat" "break" "set" "quote" "quasiquote"
    "unquote" "splice" "upscope" "try" "defer" "edefer" "and" "or" "import"
    "use" "label" "return" "with" "->" "->>" "-?>" "-?>>" "as->" "default"
    "comptime" "compwhen" "comment"))

(par_tup_lit
  . (sym_lit) @keyword
  . (sym_lit) @function
  (#any-of? @keyword "defn" "defn-" "defmacro" "defmacro-" "varfn"))
"#;

const PHP_EXTRA_HIGHLIGHTS: &str = r#"
(class_declaration name: (name) @type)
(interface_declaration name: (name) @type)
(trait_declaration name: (name) @type)
(enum_declaration name: (name) @type)
"#;

/// The Odin query targets nvim: `#lua-match?` would be ignored as an unknown
/// predicate (painting every identifier as a type/constant), so turn it into
/// `#match?` — the patterns used are valid regexes — and drop the
/// nvim-only `#not-has-parent?` filters.
fn odin_highlights() -> &'static str {
    static Q: OnceLock<String> = OnceLock::new();
    Q.get_or_init(|| {
        let mut q = tree_sitter_odin::HIGHLIGHTS_QUERY.replace("#lua-match?", "#match?");
        // Cut just the `(#not-has-parent? ...)` form; the pattern's own
        // closing paren follows it on the same line.
        while let Some(start) = q.find("(#not-has-parent?") {
            let end = start + q[start..].find(')').map_or(q.len() - start, |i| i + 1);
            q.replace_range(start..end, "");
        }
        q
    })
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
    "constructor",
    "module",
    // Without these, `keyword.function` resolves to `function`, and the
    // older nvim-style names (Lua, Zig, Scala, Odin) match nothing; the theme
    // maps them onto the styles above (`theme::HIGHLIGHT_ALIASES`).
    "keyword.function",
    "conditional",
    "repeat",
    "include",
    "storageclass",
    "exception",
    "preproc",
    "namespace",
    "parameter",
    "method",
    "float",
    "field",
    "character",
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
        let lines = engine.highlight("name: gluck\ncount: 42\nactive: true\n", "config.yaml");
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

    #[test]
    fn test_bash_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "#!/bin/bash\nfor f in *.txt; do\n  echo \"$f\"\ndone\n",
            "build.sh",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in bash highlight output");
    }

    #[test]
    fn test_zsh_uses_bash_highlighting() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "if [ -f config ]; then\n  source config\nfi\n",
            ".zshrc.zsh",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in zsh highlight output");
    }

    #[test]
    fn test_zig_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "const std = @import(\"std\");\n\npub fn main() !void {\n    var x: u32 = 42;\n    std.debug.print(\"{}\\n\", .{x});\n}\n",
            "main.zig",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in zig highlight output");
    }

    #[test]
    fn test_zig_keywords_are_colored() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "pub fn add(a: i32, b: i32) i32 {\n    return a + b;\n}\n",
            "lib.zig",
        );
        let keyword_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                ["pub", "fn", "return"].contains(&s.content.as_ref()) && s.style.fg.is_some()
            })
            .collect();
        assert!(
            keyword_spans.len() >= 3,
            "expected `pub`/`fn`/`return` to be keyword-colored in zig, got spans: {:#?}",
            lines
        );
    }

    #[test]
    fn test_c_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "#include <stdio.h>\n\nint main(void) {\n    printf(\"hi\\n\");\n    return 0;\n}\n",
            "main.c",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in c highlight output");
    }

    #[test]
    fn test_c_keywords_are_colored() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "static int add(int a, int b) {\n    if (a) { return a + b; }\n    return b;\n}\n",
            "add.h",
        );
        let keyword_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                ["static", "if", "return", "int"].contains(&s.content.as_ref())
                    && s.style.fg.is_some()
            })
            .collect();
        assert!(
            keyword_spans.len() >= 4,
            "expected `static`/`if`/`return`/`int` to be colored in c, got spans: {:#?}",
            lines
        );
    }

    #[test]
    fn test_asm_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            ".section .text\n.global _start\n_start:\n    mov $60, %rax  # exit\n    syscall\n",
            "start.s",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in asm highlight output");
    }

    #[test]
    fn test_linkerscript_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "ENTRY(_start)\n\nSECTIONS\n{\n  . = 0x10000;\n  .text : { *(.text) }\n  .data : { *(.data) }\n}\n",
            "kernel.ld",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(
            has_color,
            "no colored spans in linkerscript highlight output"
        );
    }

    #[test]
    fn test_linkerscript_keywords_are_colored() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "MEMORY\n{\n  FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 256K\n}\nSECTIONS { }\n",
            "stm32.lds",
        );
        let keyword_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                ["MEMORY", "SECTIONS"].contains(&s.content.as_ref()) && s.style.fg.is_some()
            })
            .collect();
        assert!(
            keyword_spans.len() >= 2,
            "expected `MEMORY`/`SECTIONS` to be keyword-colored in linkerscript, got spans: {:#?}",
            lines
        );
    }

    #[test]
    fn test_go_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "package main\n\nimport \"fmt\"\n\nfunc main() {\n    fmt.Println(\"hi\")\n}\n",
            "main.go",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in go highlight output");
    }

    #[test]
    fn test_go_keywords_are_colored() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "func add(a int, b int) int {\n    if a > 0 {\n        return a + b\n    }\n    return b\n}\n",
            "add.go",
        );
        let keyword_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                ["func", "if", "return", "int"].contains(&s.content.as_ref())
                    && s.style.fg.is_some()
            })
            .collect();
        assert!(
            keyword_spans.len() >= 4,
            "expected `func`/`if`/`return`/`int` to be colored in go, got spans: {:#?}",
            lines
        );
    }

    #[test]
    fn test_lisette_highlight_produces_colors() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "enum Shape {\n  Circle(float64),\n}\n\nfn area(shape: Shape) -> float64 {\n  match shape {\n    Shape.Circle(r) => 3.14 * r * r,\n  }\n}\n",
            "shape.lis",
        );
        assert!(!lines.is_empty());
        let has_color = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg.is_some());
        assert!(has_color, "no colored spans in lisette highlight output");
    }

    #[test]
    fn test_lisette_keywords_are_colored() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(
            "fn main() {\n  let mut count = 0\n  match count {\n    0 => (),\n    _ => (),\n  }\n}\n",
            "main.lis",
        );
        let keyword_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                ["fn", "let", "mut", "match"].contains(&s.content.as_ref()) && s.style.fg.is_some()
            })
            .collect();
        assert!(
            keyword_spans.len() >= 4,
            "expected `fn`/`let`/`mut`/`match` to be keyword-colored in lisette, got spans: {:#?}",
            lines
        );
    }

    const GLEAM_SAMPLE: &str = "import gleam/io\n\nconst limit = 10\n\npub fn main() -> Nil {\n  let pets = [Dog(\"rex\", 3)]\n  case pets {\n    [] -> todo\n    _ -> io.println(\"hi\")\n  }\n}\n";

    fn span_fg(lines: &[Line<'static>], text: &str) -> Option<ratatui::style::Color> {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.as_ref() == text)
            .and_then(|s| s.style.fg)
    }

    #[test]
    fn test_gleam_keywords_are_colored() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(GLEAM_SAMPLE, "src/app.gleam");
        let keyword_spans: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| {
                ["import", "const", "pub", "fn", "let", "case", "todo"]
                    .contains(&s.content.as_ref())
                    && s.style.fg.is_some()
            })
            .collect();
        assert!(
            keyword_spans.len() >= 7,
            "expected gleam keywords to be colored, got spans: {:#?}",
            lines
        );
    }

    #[test]
    fn test_gleam_names_beat_variable_catch_all() {
        // The query starts with an `(identifier) @variable` catch-all; later,
        // more specific patterns must win for definitions and module calls.
        let palette = crate::theme::Palette::plain();
        let mut engine = HighlightEngine::new();
        engine.set_theme(palette.to_highlight_map());
        let lines = engine.highlight(GLEAM_SAMPLE, "src/app.gleam");
        assert_eq!(span_fg(&lines, "main"), Some(palette.syn_function));
        assert_eq!(span_fg(&lines, "println"), Some(palette.syn_function));
        assert_eq!(span_fg(&lines, "limit"), Some(palette.syn_constant));
        assert_eq!(span_fg(&lines, "Dog"), Some(palette.syn_type));
        assert_eq!(span_fg(&lines, "\"rex\""), Some(palette.syn_string));
    }

    /// Asserts each `(text, color)` pair against the first span with that
    /// exact text — a failed config build or a catch-all `@variable` that
    /// beats the specific patterns both show up here.
    fn assert_span_colors(path: &str, source: &str, expected: &[(&str, ratatui::style::Color)]) {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight(source, path);
        for (text, color) in expected {
            assert_eq!(
                span_fg(&lines, text),
                Some(*color),
                "{path}: `{text}` has the wrong color, spans: {lines:#?}"
            );
        }
    }

    #[test]
    fn test_zig_fn_is_keyword_not_function() {
        // `@keyword.function` used to resolve to the `function` style.
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "add.zig",
            "fn add(a: i32) i32 {\n    return a;\n}\n",
            &[("fn", p.syn_keyword), ("add", p.syn_function)],
        );
    }

    #[test]
    fn test_python_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "app.py",
            "import os\n\ndef greet(name):\n    if name:\n        return \"hi\"\n    print(name)\n",
            &[
                ("import", p.syn_keyword),
                ("def", p.syn_keyword),
                ("greet", p.syn_function),
                ("return", p.syn_keyword),
                ("\"hi\"", p.syn_string),
                ("print", p.syn_type),
            ],
        );
    }

    #[test]
    fn test_cpp_highlight_colors() {
        // `return`/`int` come from the layered C query, `namespace`/`class`
        // from the C++ one.
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "vec.hpp",
            "namespace geo {\nclass Vec {\npublic:\n  int len() const { return 0; }\n};\n}\nauto s = \"v\";\n",
            &[
                ("namespace", p.syn_keyword),
                ("class", p.syn_keyword),
                ("public", p.syn_keyword),
                ("int", p.syn_type),
                ("len", p.syn_function),
                ("return", p.syn_keyword),
                ("\"v\"", p.syn_string),
            ],
        );
    }

    #[test]
    fn test_java_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "Main.java",
            "public class Main {\n  static int run(String s) {\n    return s.length();\n  }\n}\n",
            &[
                ("public", p.syn_keyword),
                ("class", p.syn_keyword),
                ("Main", p.syn_type),
                ("run", p.syn_function),
                ("String", p.syn_type),
                ("return", p.syn_keyword),
                ("length", p.syn_function),
            ],
        );
    }

    #[test]
    fn test_ruby_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "app.rb",
            "class Greeter\n  def greet(name)\n    return \"hi\" if name\n  end\nend\n",
            &[
                ("class", p.syn_keyword),
                ("Greeter", p.syn_type),
                ("def", p.syn_keyword),
                ("greet", p.syn_function),
                ("return", p.syn_keyword),
                ("end", p.syn_keyword),
            ],
        );
    }

    #[test]
    fn test_lua_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "init.lua",
            "local function greet(name)\n  if name then\n    return \"hi\"\n  end\nend\nprint(greet(\"x\"))\n",
            &[
                ("local", p.syn_keyword),
                ("function", p.syn_keyword),
                ("if", p.syn_keyword),
                ("greet", p.syn_function),
                ("then", p.syn_keyword),
                ("return", p.syn_keyword),
                ("end", p.syn_keyword),
            ],
        );
    }

    #[test]
    fn test_kotlin_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "Main.kt",
            "package app\n\nclass Greeter(val name: String) {\n    fun greet(): String {\n        if (name.isEmpty()) return \"hi\"\n        return name\n    }\n}\n",
            &[
                ("package", p.syn_keyword),
                ("class", p.syn_keyword),
                ("Greeter", p.syn_type),
                ("fun", p.syn_keyword),
                ("greet", p.syn_function),
                ("String", p.syn_type),
                ("if", p.syn_keyword),
                // The jump expression is captured with its trailing space.
                ("return ", p.syn_keyword),
                ("isEmpty", p.syn_function),
            ],
        );
    }

    #[test]
    fn test_scala_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "App.scala",
            "object App {\n  def greet(name: String): String =\n    if (name.isEmpty) \"hi\" else name\n}\n",
            &[
                ("object", p.syn_keyword),
                ("def", p.syn_keyword),
                ("greet", p.syn_function),
                ("String", p.syn_type),
                ("if", p.syn_keyword),
                ("else", p.syn_keyword),
                ("\"hi\"", p.syn_string),
            ],
        );
    }

    #[test]
    fn test_elixir_highlight_colors() {
        // `App` is @module, styled like variables.
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "lib/app.ex",
            "defmodule App do\n  def greet(name) do\n    if name, do: \"hi\", else: nil\n  end\nend\n",
            &[
                ("defmodule", p.syn_keyword),
                ("App", p.syn_variable),
                ("def", p.syn_keyword),
                ("greet", p.syn_function),
                ("do", p.syn_keyword),
                ("end", p.syn_keyword),
                ("\"hi\"", p.syn_string),
            ],
        );
    }

    #[test]
    fn test_erlang_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "src/app.erl",
            "-module(app).\n-export([greet/1]).\n\ngreet(Name) ->\n    case Name of\n        [] -> \"hi\";\n        _ -> io:format(Name)\n    end.\n",
            &[
                ("module", p.syn_keyword),
                ("greet", p.syn_function),
                ("case", p.syn_keyword),
                ("of", p.syn_keyword),
                ("end", p.syn_keyword),
                ("format", p.syn_function),
                ("\"hi\"", p.syn_string),
            ],
        );
    }

    #[test]
    fn test_odin_highlight_colors() {
        // `a` guards the `#lua-match?` rewrite: unfiltered it would turn into a type.
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "main.odin",
            "package main\n\nimport \"core:fmt\"\n\nMAX :: 10\n\nadd :: proc(a: int, b: int) -> int {\n\tif a > 0 {\n\t\treturn a + b\n\t}\n\treturn b\n}\n",
            &[
                ("package", p.syn_keyword),
                ("import", p.syn_keyword),
                ("MAX", p.syn_constant),
                ("add", p.syn_function),
                ("proc", p.syn_keyword),
                ("int", p.syn_type),
                ("if", p.syn_keyword),
                ("return", p.syn_keyword),
                ("a", p.syn_variable),
            ],
        );
    }

    #[test]
    fn test_php_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "index.php",
            "<?php\nnamespace App;\n\nclass Greeter {\n    public function greet(string $name): string {\n        if ($name) { return \"hi\"; }\n        return strlen($name);\n    }\n}\n",
            &[
                ("namespace", p.syn_keyword),
                ("class", p.syn_keyword),
                ("Greeter", p.syn_type),
                ("public", p.syn_keyword),
                ("function", p.syn_keyword),
                ("greet", p.syn_function),
                ("if", p.syn_keyword),
                ("return", p.syn_keyword),
                ("hi", p.syn_string),
                ("strlen", p.syn_function),
            ],
        );
    }

    #[test]
    fn test_llvm_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "add.ll",
            "define i32 @add(i32 %a, i32 %b) {\nentry:\n  %sum = add i32 %a, %b\n  ret i32 %sum\n}\n",
            &[
                ("define", p.syn_keyword),
                ("i32", p.syn_type),
                ("@add", p.syn_function),
                ("add", p.syn_keyword),
                ("ret", p.syn_keyword),
            ],
        );
    }

    #[test]
    fn test_outer_style_survives_nested_capture() {
        // PHP captures `"` inside `@string`; ending that inner capture used to
        // reset the style, leaving the closing quote unstyled.
        let p = crate::theme::Palette::plain();
        let mut engine = HighlightEngine::new();
        engine.set_theme(p.to_highlight_map());
        let lines = engine.highlight("<?php\necho \"hi\";\n", "a.php");
        let quotes: Vec<_> = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.content.as_ref() == "\"")
            .collect();
        assert_eq!(quotes.len(), 2, "spans: {lines:#?}");
        assert!(quotes.iter().all(|s| s.style.fg == Some(p.syn_string)));
    }

    #[test]
    fn test_clojure_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "src/app/core.clj",
            "(ns app.core\n  (:require [clojure.string :as str]))\n\n(defn greet [name]\n  (if (str/blank? name) \"hi\" (println name 42)))\n",
            &[
                ("ns", p.syn_keyword),
                ("app.core", p.syn_variable),
                ("defn", p.syn_keyword),
                ("greet", p.syn_function),
                ("if", p.syn_keyword),
                ("str", p.syn_variable),
                ("blank?", p.syn_function),
                ("println", p.syn_function),
                ("\"hi\"", p.syn_string),
                ("42", p.syn_constant),
            ],
        );
    }

    #[test]
    fn test_janet_highlight_colors() {
        let p = crate::theme::Palette::plain();
        assert_span_colors(
            "main.janet",
            "(defn greet [name]\n  (if (empty? name) \"hi\" (print name :k)))\n",
            &[
                ("defn", p.syn_keyword),
                ("greet", p.syn_function),
                ("if", p.syn_keyword),
                ("empty?", p.syn_function),
                ("print", p.syn_function),
                ("\"hi\"", p.syn_string),
                (":k", p.syn_constant),
            ],
        );
    }

    #[test]
    fn test_tab_indentation_is_expanded_in_highlight() {
        let mut engine = HighlightEngine::new();
        engine.set_theme(crate::theme::Palette::plain().to_highlight_map());
        let lines = engine.highlight("func main() {\n\tfmt.Println(\"hi\")\n}\n", "main.go");
        let second: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            second.starts_with("    "),
            "expected tab indent expanded to spaces, got: {:?}",
            second
        );
        assert!(!second.contains('\t'), "tab survived in: {:?}", second);
    }

    #[test]
    fn test_tab_indentation_is_expanded_in_plain_lines() {
        let lines = HighlightEngine::plain_lines("a\n\tindented\n");
        let second: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(second, "    indented");
    }
}
