use std::sync::OnceLock;
use tree_sitter::{Language as TsLanguage, Parser, Query, QueryCursor};

use super::ChunkError;
use crate::lang::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Class,
    Other,
}

#[derive(Debug, Clone)]
pub struct SymbolSpan {
    pub kind: SymbolKind,
    pub name: String,
    pub line_start: u32,
    pub line_end: u32,
    pub byte_start: usize,
    pub byte_end: usize,
}

pub fn extract_symbols(source: &str, language: Language) -> Result<Vec<SymbolSpan>, ChunkError> {
    let Some((ts_lang, query)) = lang_and_query(language) else {
        return Ok(Vec::new());
    };

    let mut parser = Parser::new();
    parser
        .set_language(ts_lang)
        .map_err(|e| ChunkError::Parse {
            language: language.as_str(),
            message: e.to_string(),
        })?;
    let tree = parser
        .parse(source.as_bytes(), None)
        .ok_or_else(|| ChunkError::Parse {
            language: language.as_str(),
            message: "parse returned None".into(),
        })?;

    let mut cursor = QueryCursor::new();
    let matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    let mut spans = Vec::new();
    for m in matches {
        if let Some(s) = build_symbol_span(&m, query, source) {
            spans.push(s);
        }
    }
    Ok(spans)
}

fn build_symbol_span(
    m: &tree_sitter::QueryMatch,
    query: &Query,
    source: &str,
) -> Option<SymbolSpan> {
    let mut symbol_node = None;
    let mut name_node = None;
    let mut kind = SymbolKind::Other;
    for cap in m.captures {
        let cap_name = query.capture_names()[cap.index as usize];
        match cap_name {
            "symbol.function" => {
                symbol_node = Some(cap.node);
                kind = SymbolKind::Function;
            }
            "symbol.method" => {
                symbol_node = Some(cap.node);
                kind = SymbolKind::Method;
            }
            "symbol.struct" => {
                symbol_node = Some(cap.node);
                kind = SymbolKind::Struct;
            }
            "symbol.enum" => {
                symbol_node = Some(cap.node);
                kind = SymbolKind::Enum;
            }
            "symbol.trait" => {
                symbol_node = Some(cap.node);
                kind = SymbolKind::Trait;
            }
            "symbol.type" => {
                symbol_node = Some(cap.node);
                kind = SymbolKind::TypeAlias;
            }
            "symbol.class" => {
                symbol_node = Some(cap.node);
                kind = SymbolKind::Class;
            }
            "symbol.other" => {
                symbol_node = Some(cap.node);
                kind = SymbolKind::Other;
            }
            "name" => {
                name_node = Some(cap.node);
            }
            _ => {}
        }
    }
    let symbol_node = symbol_node?;
    let name_node = name_node?;
    let name = name_node.utf8_text(source.as_bytes()).ok()?.to_string();
    Some(SymbolSpan {
        kind,
        name,
        line_start: (symbol_node.start_position().row + 1) as u32,
        line_end: (symbol_node.end_position().row + 1) as u32,
        byte_start: symbol_node.start_byte(),
        byte_end: symbol_node.end_byte(),
    })
}

fn lang_and_query(language: Language) -> Option<(&'static TsLanguage, &'static Query)> {
    match language {
        Language::Rust => Some((rust_lang(), rust_query())),
        Language::Python => Some((python_lang(), python_query())),
        Language::JavaScript => Some((javascript_lang(), javascript_query())),
        Language::TypeScript => Some((typescript_lang(), typescript_query())),
        Language::Tsx => Some((tsx_lang(), tsx_query())),
        Language::Go => Some((go_lang(), go_query())),
        Language::Swift => Some((swift_lang(), swift_query())),
        _ => None,
    }
}

// Parser는 thread-safe가 아니라 매 호출 새로 만들지만,
// Language와 Query는 immutable이므로 OnceLock으로 캐싱 — 파일마다 재컴파일 비용 제거.
fn rust_lang() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_rust::LANGUAGE.into())
}

fn python_lang() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_python::LANGUAGE.into())
}

fn javascript_lang() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_javascript::LANGUAGE.into())
}

fn typescript_lang() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
}

fn tsx_lang() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_typescript::LANGUAGE_TSX.into())
}

fn go_lang() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_go::LANGUAGE.into())
}

fn swift_lang() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_swift::LANGUAGE.into())
}

fn rust_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    Q.get_or_init(|| Query::new(rust_lang(), RUST_QUERY).expect("valid rust query"))
}

fn python_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    Q.get_or_init(|| Query::new(python_lang(), PYTHON_QUERY).expect("valid python query"))
}

fn javascript_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    Q.get_or_init(|| {
        Query::new(javascript_lang(), JAVASCRIPT_QUERY).expect("valid javascript query")
    })
}

fn typescript_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    Q.get_or_init(|| Query::new(typescript_lang(), TYPESCRIPT_QUERY).expect("valid ts query"))
}

// TS와 TSX는 grammar가 달라 Query 인스턴스도 분리 — Query는 컴파일된 grammar에 바인딩.
fn tsx_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    Q.get_or_init(|| Query::new(tsx_lang(), TYPESCRIPT_QUERY).expect("valid tsx query"))
}

fn go_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    Q.get_or_init(|| Query::new(go_lang(), GO_QUERY).expect("valid go query"))
}

fn swift_query() -> &'static Query {
    static Q: OnceLock<Query> = OnceLock::new();
    Q.get_or_init(|| Query::new(swift_lang(), SWIFT_QUERY).expect("valid swift query"))
}

// 핵심 원칙: top-level 심볼만. nested function/method 제외.
// 예외: impl_item / trait_item 내부의 function_item은 메서드로 추출 (컨테이너는 청크 안 함).
const RUST_QUERY: &str = r#"
((source_file
   (function_item name: (identifier) @name) @symbol.function))

((source_file
   (struct_item name: (type_identifier) @name) @symbol.struct))

((source_file
   (enum_item name: (type_identifier) @name) @symbol.enum))

((source_file
   (trait_item name: (type_identifier) @name) @symbol.trait))

((source_file
   (type_item name: (type_identifier) @name) @symbol.type))

((source_file
   (impl_item
     (declaration_list
       (function_item name: (identifier) @name) @symbol.method))))

((source_file
   (trait_item
     (declaration_list
       (function_item name: (identifier) @name) @symbol.method))))
"#;

const PYTHON_QUERY: &str = r#"
((module
   (function_definition name: (identifier) @name) @symbol.function))

((module
   (class_definition name: (identifier) @name) @symbol.class))
"#;

const JAVASCRIPT_QUERY: &str = r#"
((program
   (function_declaration name: (identifier) @name) @symbol.function))

((program
   (class_declaration name: (identifier) @name) @symbol.class))

((program
   (lexical_declaration
     (variable_declarator
       name: (identifier) @name
       value: [(arrow_function) (function_expression)])) @symbol.function))
"#;

const TYPESCRIPT_QUERY: &str = r#"
((program
   (function_declaration name: (identifier) @name) @symbol.function))

((program
   (class_declaration name: (type_identifier) @name) @symbol.class))

((program
   (interface_declaration name: (type_identifier) @name) @symbol.struct))

((program
   (type_alias_declaration name: (type_identifier) @name) @symbol.struct))
"#;

const GO_QUERY: &str = r#"
((source_file
   (function_declaration name: (identifier) @name) @symbol.function))

((source_file
   (method_declaration name: (field_identifier) @name) @symbol.method))

((source_file
   (type_declaration
     (type_spec name: (type_identifier) @name)) @symbol.struct))
"#;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_top_level_functions_only() {
        let src = r#"
fn outer() {
    fn inner() {}
}
fn another() {}
"#;
        let spans = extract_symbols(src, Language::Rust).unwrap();
        let names: Vec<_> = spans.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"outer"));
        assert!(names.contains(&"another"));
        assert!(!names.contains(&"inner"));
    }

    #[test]
    fn rust_impl_methods_extracted_not_container() {
        let src = r#"
struct Foo { x: i32 }
impl Foo {
    fn bar(&self) {}
    fn baz(&self) {}
}
"#;
        let spans = extract_symbols(src, Language::Rust).unwrap();
        let kinds_names: Vec<_> = spans.iter().map(|s| (s.kind, s.name.as_str())).collect();
        assert!(kinds_names.contains(&(SymbolKind::Struct, "Foo")));
        assert!(kinds_names.contains(&(SymbolKind::Method, "bar")));
        assert!(kinds_names.contains(&(SymbolKind::Method, "baz")));
    }

    #[test]
    fn rust_trait_default_methods_extracted() {
        let src = r#"
trait Greet {
    fn name(&self) -> &str;
    fn hello(&self) -> String {
        format!("Hello, {}", self.name())
    }
}
"#;
        let spans = extract_symbols(src, Language::Rust).unwrap();
        let methods: Vec<_> = spans
            .iter()
            .filter(|s| s.kind == SymbolKind::Method)
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            methods.contains(&"hello"),
            "default method should be extracted"
        );
    }

    #[test]
    fn python_top_level_only() {
        let src = r#"
def outer():
    def inner():
        pass

class Bar:
    def method(self):
        pass
"#;
        let spans = extract_symbols(src, Language::Python).unwrap();
        let names: Vec<_> = spans.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"outer"));
        assert!(names.contains(&"Bar"));
        assert!(!names.contains(&"inner"));
        assert!(!names.contains(&"method"));
    }

    #[test]
    fn unsupported_language_returns_empty() {
        let spans = extract_symbols("int main() { return 0; }", Language::C).unwrap();
        assert!(spans.is_empty());
    }

    #[test]
    fn tsx_parses_with_dedicated_grammar() {
        let src = r#"
function Button() {
    return <button>click</button>;
}
"#;
        let spans = extract_symbols(src, Language::Tsx).unwrap();
        assert!(spans.iter().any(|s| s.name == "Button"));
    }

    #[test]
    fn go_method_declaration() {
        let src = r#"
package main

type Foo struct { x int }

func (f *Foo) Bar() {}
func Top() {}
"#;
        let spans = extract_symbols(src, Language::Go).unwrap();
        let kinds_names: Vec<_> = spans.iter().map(|s| (s.kind, s.name.as_str())).collect();
        assert!(kinds_names.contains(&(SymbolKind::Function, "Top")));
        assert!(kinds_names.contains(&(SymbolKind::Method, "Bar")));
    }

    #[test]
    fn rust_top_level_trait_extracted() {
        let src = r#"
trait Greet {
    fn name(&self) -> &str;
    fn hello(&self) -> String { String::new() }
}
"#;
        let spans = extract_symbols(src, Language::Rust).unwrap();
        let has_trait_container = spans
            .iter()
            .any(|s| s.kind == SymbolKind::Trait && s.name == "Greet");
        assert!(
            has_trait_container,
            "top-level trait declaration must be extracted as Trait, not only its methods"
        );
    }

    #[test]
    fn rust_top_level_type_alias_extracted() {
        let src = r#"
type CommitId = String;
type Result<T> = std::result::Result<T, MyError>;
"#;
        let spans = extract_symbols(src, Language::Rust).unwrap();
        let names: Vec<_> = spans
            .iter()
            .filter(|s| s.kind == SymbolKind::TypeAlias)
            .map(|s| s.name.as_str())
            .collect();
        assert!(names.contains(&"CommitId"));
        assert!(names.contains(&"Result"));
    }

    #[test]
    fn queries_are_cached() {
        // 두 번 호출해도 같은 포인터를 받아야 함 — 매 호출 재컴파일 없음
        let q1 = rust_query() as *const Query;
        let q2 = rust_query() as *const Query;
        assert_eq!(q1, q2);
    }

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
}
