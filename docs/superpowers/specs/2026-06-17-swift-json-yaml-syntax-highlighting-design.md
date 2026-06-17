# Swift/JSON/YAML view mode syntax highlighting + Swift symbol chunking

## Summary

Add syntax highlighting in view mode for Swift, JSON, and YAML files, plus symbol chunking support for Swift so semantic search can index top-level symbols.

## Scope

- **View mode syntax highlighting**
  - Swift (`.swift`)
  - JSON (`.json`, `.jsonc`)
  - YAML (`.yaml`, `.yml`)
- **Semantic search symbol chunking**
  - Swift top-level symbols only
  - JSON/YAML are intentionally excluded from symbol chunking

## Dependencies

Add to `Cargo.toml`:

```toml
tree-sitter-swift = "0.7"
tree-sitter-json = "0.23"
tree-sitter-yaml = "0.6"
```

These versions are chosen to stay compatible with the project's existing `tree-sitter = "0.23"`.
`tree-sitter-yaml 0.6` still returns `tree_sitter::Language` directly, so no extra conversion is needed.

## Design decisions

### Hybrid query strategy

- **YAML**: use the grammar's default highlight query. It relies on a small set of capture names, most of which already exist in the project's `HIGHLIGHT_NAMES`.
- **JSON**: use a small custom highlights query. The grammar's default query uses `string.special.key` and `escape`, which are not in the project's `HIGHLIGHT_NAMES`; the custom query maps JSON keys to `property` and escape sequences to `string.escape`.
- **Swift**: write a custom highlight query that maps only to existing `HIGHLIGHT_NAMES`. The default Swift query introduces many additional captures (`function.method`, `function.call`, `keyword.*`, `variable.member`, etc.). Adding all of them would bloat the theme map for one language, so we trade some granularity for consistency.

### Theme additions

The default JSON/YAML queries need these new capture names:

- `number`
- `boolean`
- `constant.builtin`
- `label`

These will be appended to `HIGHLIGHT_NAMES` and mapped to existing palette colors in `theme.rs::to_highlight_map()`:

- `number` → `syn_constant`
- `boolean` → `syn_constant`
- `constant.builtin` → `syn_constant`
- `label` → `syn_type`

## Component changes

### `src/lang.rs`

- Add `Swift` and `Yaml` variants to the `Language` enum. `Json` already exists but is not currently registered for highlighting.
- Update `Language::from_path`:
  - `"swift"` → `.swift`
  - `"json"` → `.json`, `.jsonc`
  - `"yaml"` → `.yaml`, `.yml`
- Update `Language::as_str`.
- Update `Language::supports_symbol_chunking()` to include `Swift`.

### `src/highlight/engine.rs`

- Register highlight configurations for `"swift"`, `"json"`, and `"yaml"` in `register_languages()`.
- `make_swift_config()`: build a custom highlights query using existing capture names only (`keyword`, `function`, `type`, `string`, `comment`, `property`, `attribute`, `punctuation.*`, `operator`).
- `make_json_config()`: use a small custom highlights query. The grammar's default query uses `string.special.key` and `escape`, which are not in the project's `HIGHLIGHT_NAMES`; the custom query maps JSON keys to `property` and escape sequences to `string.escape`.
- `make_yaml_config()`: use `tree_sitter_yaml::HIGHLIGHTS_QUERY`.
- Add unit tests verifying each language produces colored spans.

### `src/theme.rs`

- Append to `HIGHLIGHT_NAMES`:
  - `number`
  - `boolean`
  - `constant.builtin`
  - `label`
- Add mappings in `Palette::to_highlight_map()` using existing palette colors.

### `src/search/chunk/symbol.rs`

- Add Swift to `lang_and_query()`.
- Add `swift_lang()` returning `tree_sitter_swift::LANGUAGE.into()`.
- Add `swift_query()` with a top-level-only symbol query.
- Map Swift declaration nodes to `SymbolKind`:
  - `function_declaration` → `Function`
  - `class_declaration` → `Class`
  - `struct_declaration` → `Struct`
  - `enum_declaration` → `Enum`
  - `protocol_declaration` → `Trait`
  - `typealias_declaration` → `TypeAlias`
  - `extension_declaration` → `Other`
- Add unit tests for Swift symbol extraction (top-level only, nested exclusion, multiple symbol kinds).

## Data flow

### View mode

```
Language::from_path(path)
  → HighlightEngine::highlight(source, path)
    → tree-sitter events
      → theme map lookup
        → Vec<Line<'static>>
```

### Semantic search

```
extract_symbols(source, Language::Swift)
  → parser + query
    → Vec<SymbolSpan>
      → Chunk::Symbol
```

## Error handling

- Highlight parser/query failure falls back to `plain_lines(source)`, preserving existing behavior.
- Symbol extraction parser/query failure returns `ChunkError`, preserving existing behavior.

## Testing

- `src/lang.rs`: extension detection tests for `.swift`, `.json`, `.jsonc`, `.yaml`, `.yml`.
- `src/highlight/engine.rs`: tests verifying Swift, JSON, and YAML highlights produce at least one colored span.
- `src/search/chunk/symbol.rs`: Swift top-level symbol extraction, nested function exclusion, and kind mapping.
- `cargo test` and `cargo clippy --all-targets -D warnings` must pass.

## Out of scope

- JSONC comment parsing (`.jsonc` is highlighted with the JSON highlighter; parser errors fall back to plain text).
- Symbol chunking for JSON/YAML.
- New color roles in the palette; theme additions reuse existing `syn_*` colors.
