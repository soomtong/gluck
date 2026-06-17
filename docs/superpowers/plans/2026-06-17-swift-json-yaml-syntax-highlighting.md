# Swift/JSON/YAML Syntax Highlighting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add view-mode syntax highlighting for Swift, JSON, and YAML files, and enable Swift symbol chunking for semantic search.

**Architecture:** Follow the existing language plugin pattern: detect file extensions in `lang.rs`, register tree-sitter highlight configurations in `highlight/engine.rs`, extend `theme.rs` for new capture names, and add a tree-sitter symbol query in `search/chunk/symbol.rs`. Use custom highlight queries for Swift and JSON to stay within the existing `HIGHLIGHT_NAMES` set, and the default query for YAML.

**Tech Stack:** Rust, tree-sitter 0.23, ratatui, cargo

---

## File structure

- `Cargo.toml` — add `tree-sitter-swift`, `tree-sitter-json`, `tree-sitter-yaml` dependencies.
- `src/lang.rs` — add `Swift` and `Yaml` variants; update extension detection and symbol-chunking support.
- `src/theme.rs` — append `number`, `boolean`, `constant.builtin`, `label` to `HIGHLIGHT_NAMES` and map them to existing palette colors.
- `src/highlight/engine.rs` — register Swift/JSON/YAML configs; add custom highlight queries for Swift and JSON.
- `src/search/chunk/symbol.rs` — add Swift language/query cache and top-level symbol extraction.

---

## Task 1: Add tree-sitter dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add crates to `Cargo.toml`**

Insert these lines after `tree-sitter-markdown-fork = "0.7.3"` in the `[dependencies]` section:

```toml
tree-sitter-swift = "0.7"
tree-sitter-json = "0.23"
tree-sitter-yaml = "0.6"
```

- [ ] **Step 2: Verify dependencies resolve**

Run:

```bash
cargo check --lib
```

Expected: dependencies download and the library compiles (there may be unrelated warnings, but no errors).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "의존성 추가: tree-sitter-swift/json/yaml"
```

---

## Task 2: Language enum and extension detection

**Files:**
- Modify: `src/lang.rs`

- [ ] **Step 1: Write the failing test**

Append this test to the `#[cfg(test)] mod tests` block at the bottom of `src/lang.rs`:

```rust
    #[test]
    fn detects_swift_jsonc_yaml() {
        assert_eq!(Language::from_path("main.swift"), Some(Language::Swift));
        assert_eq!(Language::from_path("config.yaml"), Some(Language::Yaml));
        assert_eq!(Language::from_path("config.yml"), Some(Language::Yaml));
        assert_eq!(Language::from_path("data.jsonc"), Some(Language::Json));
        assert!(Language::Swift.supports_symbol_chunking());
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --lib detects_swift_jsonc_yaml
```

Expected: compile error because `Language::Swift` and `Language::Yaml` do not exist.

- [ ] **Step 3: Implement language variants and detection**

In `src/lang.rs`:

1. Add variants to the `Language` enum:

```rust
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Go,
    C,
    Cpp,
    Java,
    Bash,
    Toml,
    Json,
    Markdown,
    Html,
    Css,
    Swift,
    Yaml,
}
```

2. Update `Language::from_path`:

```rust
            "json" | "jsonc" => Some(Self::Json),
            "md" => Some(Self::Markdown),
            "html" => Some(Self::Html),
            "css" => Some(Self::Css),
            "swift" => Some(Self::Swift),
            "yaml" | "yml" => Some(Self::Yaml),
```

3. Update `Language::as_str`:

```rust
            Self::Json => "json",
            Self::Markdown => "markdown",
            Self::Html => "html",
            Self::Css => "css",
            Self::Swift => "swift",
            Self::Yaml => "yaml",
```

4. Update `Language::supports_symbol_chunking`:

```rust
        matches!(
            self,
            Self::Rust
                | Self::Python
                | Self::JavaScript
                | Self::TypeScript
                | Self::Tsx
                | Self::Go
                | Self::Swift
        )
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test --lib detects_swift_jsonc_yaml
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/lang.rs
git commit -m "언어 감지에 Swift/YAML 추가, JSONC 매핑 및 Swift symbol chunking 지원"
```

---

## Task 3: Theme additions for new capture names

**Files:**
- Modify: `src/theme.rs`

- [ ] **Step 1: Write the failing test**

Append this test to the `#[cfg(test)] mod tests` block at the bottom of `src/theme.rs`:

```rust
    #[test]
    fn test_highlight_map_includes_new_capture_names() {
        let p = Palette::plain();
        let map = p.to_highlight_map();
        assert!(map.contains_key("number"));
        assert!(map.contains_key("boolean"));
        assert!(map.contains_key("constant.builtin"));
        assert!(map.contains_key("label"));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --lib test_highlight_map_includes_new_capture_names
```

Expected: FAIL — assertion failed because the keys are missing.

- [ ] **Step 3: Add capture names and theme mappings**

1. Append to the `HIGHLIGHT_NAMES` slice in `src/theme.rs`:

```rust
    "number",
    "boolean",
    "constant.builtin",
    "label",
```

2. Add these entries inside `Palette::to_highlight_map` before the final `m` return:

```rust
        m.insert("number".into(), Style::new().fg(self.syn_constant));
        m.insert("boolean".into(), Style::new().fg(self.syn_constant));
        m.insert("constant.builtin".into(), Style::new().fg(self.syn_constant));
        m.insert("label".into(), Style::new().fg(self.syn_type));
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test --lib test_highlight_map_includes_new_capture_names
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/theme.rs
git commit -m "테마에 number/boolean/constant.builtin/label capture 추가"
```

---

## Task 4: JSON highlight configuration

**Files:**
- Modify: `src/highlight/engine.rs`

- [ ] **Step 1: Write the failing test**

Append this test to the `#[cfg(test)] mod tests` block at the bottom of `src/highlight/engine.rs`:

```rust
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --lib test_json_highlight_produces_colors
```

Expected: FAIL — JSON files fall back to plain text because no config is registered.

- [ ] **Step 3: Implement JSON highlight config**

1. Add `make_json_config` call in `register_languages`:

```rust
        if let Ok(config) = Self::make_json_config() {
            self.configs.insert("json".to_string(), config);
        }
```

2. Add the config function and custom query constant near the other `make_*_config` functions:

```rust
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
```

3. Add the query constant near `MARKDOWN_HIGHLIGHTS_QUERY`:

```rust
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test --lib test_json_highlight_produces_colors
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/highlight/engine.rs
git commit -m "JSON view mode syntax highlighting 추가"
```

---

## Task 5: YAML highlight configuration

**Files:**
- Modify: `src/highlight/engine.rs`

- [ ] **Step 1: Write the failing test**

Append this test to the `#[cfg(test)] mod tests` block at the bottom of `src/highlight/engine.rs`:

```rust
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --lib test_yaml_highlight_produces_colors
```

Expected: FAIL — YAML files fall back to plain text because no config is registered.

- [ ] **Step 3: Implement YAML highlight config**

1. Add `make_yaml_config` call in `register_languages`:

```rust
        if let Ok(config) = Self::make_yaml_config() {
            self.configs.insert("yaml".to_string(), config);
        }
```

2. Add the config function near the other `make_*_config` functions:

```rust
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test --lib test_yaml_highlight_produces_colors
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/highlight/engine.rs
git commit -m "YAML view mode syntax highlighting 추가"
```

---

## Task 6: Swift highlight configuration

**Files:**
- Modify: `src/highlight/engine.rs`

- [ ] **Step 1: Write the failing test**

Append this test to the `#[cfg(test)] mod tests` block at the bottom of `src/highlight/engine.rs`:

```rust
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --lib test_swift_highlight_produces_colors
```

Expected: FAIL — Swift files fall back to plain text because no config is registered.

- [ ] **Step 3: Implement Swift highlight config**

1. Add `make_swift_config` call in `register_languages`:

```rust
        if let Ok(config) = Self::make_swift_config() {
            self.configs.insert("swift".to_string(), config);
        }
```

2. Add the config function near the other `make_*_config` functions:

```rust
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
```

3. Add the custom query constant near `JSON_HIGHLIGHTS_QUERY`:

```rust
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
  "default"
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test --lib test_swift_highlight_produces_colors
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/highlight/engine.rs
git commit -m "Swift view mode syntax highlighting 추가"
```

---

## Task 7: Swift symbol chunking

**Files:**
- Modify: `src/search/chunk/symbol.rs`

- [ ] **Step 1: Write the failing test**

Append this test to the `#[cfg(test)] mod tests` block at the bottom of `src/search/chunk/symbol.rs`:

```rust
    #[test]
    fn swift_top_level_symbols_only() {
        let src = r#"
import Foundation

func global() {}

struct Point {
    let x: Int
    func distance() -> Double { return 0.0 }
}

class Foo {
    func method() {}
}

enum Status {
    case ok
}

protocol Greet {
    func hello()
}

typealias ID = String

extension Foo {
    func ext() {}
}
"#;
        let spans = extract_symbols(src, Language::Swift).unwrap();
        let kinds_names: Vec<_> = spans.iter().map(|s| (s.kind, s.name.as_str())).collect();
        assert!(kinds_names.contains(&(SymbolKind::Function, "global")));
        assert!(kinds_names.contains(&(SymbolKind::Struct, "Point")));
        assert!(kinds_names.contains(&(SymbolKind::Class, "Foo")));
        assert!(kinds_names.contains(&(SymbolKind::Enum, "Status")));
        assert!(kinds_names.contains(&(SymbolKind::Trait, "Greet")));
        assert!(kinds_names.contains(&(SymbolKind::TypeAlias, "ID")));
        assert!(kinds_names.contains(&(SymbolKind::Other, "Foo")));
        assert!(
            !kinds_names.iter().any(|(_, n)| *n == "distance"),
            "nested method should not be extracted"
        );
        assert!(
            !kinds_names.iter().any(|(_, n)| *n == "method"),
            "nested method should not be extracted"
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --lib swift_top_level_symbols_only
```

Expected: FAIL — `extract_symbols` returns an empty vector for Swift because it is not in `lang_and_query`.

- [ ] **Step 3: Implement Swift symbol extraction**

1. Add the Swift branch to `lang_and_query`:

```rust
        Language::Swift => Some((swift_lang(), swift_query())),
```

2. Add the language cache function after `go_lang`:

```rust
fn swift_lang() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_swift::LANGUAGE.into())
}
```

3. Add the query cache function after `go_query`:

```rust
fn swift_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    Q.get_or_init(|| Query::new(swift_lang(), SWIFT_QUERY).expect("valid swift query"))
}
```

4. Add the query constant after `GO_QUERY`:

```rust
const SWIFT_QUERY: &str = r#"
((source_file
   (function_declaration
     name: (simple_identifier) @name) @symbol.function))

((source_file
   (class_declaration
     "class"
     name: (type_identifier) @name) @symbol.class))

((source_file
   (class_declaration
     "struct"
     name: (type_identifier) @name) @symbol.struct))

((source_file
   (class_declaration
     "enum"
     name: (type_identifier) @name) @symbol.enum))

((source_file
   (protocol_declaration
     name: (type_identifier) @name) @symbol.trait))

((source_file
   (typealias_declaration
     name: (type_identifier) @name) @symbol.type))

((source_file
   (class_declaration
     "extension"
     name: (_) @name) @symbol.other))
"#;
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cargo test --lib swift_top_level_symbols_only
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/search/chunk/symbol.rs
git commit -m "Swift symbol chunking 추가"
```

---

## Task 8: Final verification

**Files:**
- All modified files

- [ ] **Step 1: Run the full test suite**

Run:

```bash
cargo test
```

Expected: all tests pass, including the new tests.

- [ ] **Step 2: Run clippy**

Run:

```bash
cargo clippy --all-targets -D warnings
```

Expected: no warnings or errors.

- [ ] **Step 3: Format changed files**

Run:

```bash
rustfmt src/lang.rs src/theme.rs src/highlight/engine.rs src/search/chunk/symbol.rs
```

Expected: formatting completes without errors.

- [ ] **Step 4: Re-run tests after formatting**

Run:

```bash
cargo test
```

Expected: all tests still pass.

- [ ] **Step 5: Final commit (if formatting produced changes)**

If `git diff` shows formatting changes:

```bash
git add src/lang.rs src/theme.rs src/highlight/engine.rs src/search/chunk/symbol.rs
git commit -m "코드 포맷팅"
```

---

## Self-review checklist

- [ ] Spec coverage: every requirement in `2026-06-17-swift-json-yaml-syntax-highlighting-design.md` maps to a task above.
- [ ] Placeholder scan: no "TBD", "TODO", or vague steps remain.
- [ ] Type consistency: `Language::Swift`, `Language::Yaml`, and new capture names are used consistently across all tasks.
