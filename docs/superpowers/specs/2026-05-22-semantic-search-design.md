# Semantic Search for gluck

## Summary

Add hybrid semantic search (BM25 + embedding vector) to gluck, enabling natural language queries across commit messages and file contents. Accessed via `S` key modal popup in any mode. Requires pre-built index via `glc index` subcommand.

## Requirements

- Search targets: commit messages, file paths, file contents (code blobs)
- Index scope: HEAD-based files + all commit messages
- Offline-first: local embedding model, no API calls at runtime
- MVP-oriented: simple before sophisticated
- Existing `/` plain search unchanged; new `S` key for semantic search

## Architecture

```
┌─────────────────────────────────────────────┐
│                  glc (TUI)                   │
│  ┌─────────┐  ┌─────────┐  ┌──────────────┐ │
│  │  Pick    │  │  View   │  │    Diff      │ │
│  └────┬─────┘  └────┬────┘  └──────┬───────┘ │
│       └──────────────┼───────────────┘         │
│                      │  S  (semantic search)   │
│               ┌──────▼──────┐                  │
│               │ SearchEngine│                  │
│               │  (hybrid)   │                  │
│               └──┬───────┬──┘                  │
│          ┌────────▼┐  ┌───▼─────────┐          │
│          │  BM25   │  │   Vector    │          │
│          │(tantivy)│  │(ort+CBERT)  │          │
│          └─────────┘  └─────────────┘          │
│               ┌──────▼──────┐                  │
│               │ RRF Fusion  │                  │
│               └─────────────┘                  │
└─────────────────────────────────────────────┘
```

### New modules

| Module | Responsibility |
|---|---|
| `src/search/mod.rs` | `SearchEngine` facade, query orchestration |
| `src/search/bm25.rs` | Tantivy BM25 full-text index and search |
| `src/search/vector.rs` | ONNX Runtime + CodeBERT embedding + cosine search |
| `src/search/rrf.rs` | Reciprocal Rank Fusion of BM25 and vector results |
| `src/search/indexer.rs` | Index builder logic, invoked by `glc index` |
| `src/search/modal.rs` | Semantic search modal state machine |

## Data Model

### SearchDocument (sum type)

```rust
enum SearchDocument {
    Commit {
        oid: String,
        message_title: String,
        message_body: String,
    },
    File {
        path: String,
        commit_oid: String,
        content: String,
    },
}
```

### Tantivy Schema

| Field | Type | Indexed | Stored |
|---|---|---|---|
| `id` | TEXT | YES | YES |
| `kind` | TEXT (commit/file) | YES | YES |
| `title` | TEXT | YES | YES |
| `body` | TEXT | YES | NO |
| `path` | TEXT | YES | YES |
| `commit_oid` | TEXT | YES | YES |

Body is indexed but not stored (accessible via git blob).

### Vector Storage

Raw f32 matrix in `.glc-index/vectors/embeddings.bin`, document IDs in `doc_ids.bin`. MVP uses brute-force cosine similarity (no HNSW). Sufficient for repos with up to thousands of files.

## Index Storage (`.glc-index/`)

```
.glc-index/
├── meta.toml              # HEAD oid, doc count, timestamp, model name, vector dim, schema version
├── bm25/                  # Tantivy index files
│   └── ...
└── vectors/
    ├── doc_ids.bin         # Document ID list (order = vector index)
    └── embeddings.bin      # f32 vector matrix (N x dim)
```

### meta.toml

```toml
head_oid = "bf94d8f..."
doc_count = 187
indexed_at = 2026-05-22T10:30:00Z
model_name = "codebert-base"
vector_dim = 768
version = 1
```

`version` field supports future schema migration.

## Indexing Pipeline (`glc index`)

```
glc index [PATH]
  │
  ├── 1. HEAD hash check → compare with .glc-index/meta.toml
  │     └── Skip if unchanged
  │
  ├── 2. Collect commit messages
  │     └── CommitStore → SearchDocument::Commit
  │
  ├── 3. Walk HEAD file tree
  │     └── list_tree(HEAD) → read blobs → SearchDocument::File
  │     └── Skip binary files (reuse is_binary_blob())
  │     └── Skip files > max_file_size (default 1MB)
  │
  ├── 4. Build tantivy inverted index
  │     └── SearchDocument → tantivy Document → index
  │
  ├── 5. Generate embedding vectors
  │     └── title+body → CodeBERT embedding (batch size configurable)
  │     └── Save to .glc-index/vectors/
  │
  └── 6. Update meta.toml
```

MVP: always full reindex. Incremental indexing deferred.

## Search Execution

```
User query → "에러 핸들링 로직"
  │
  ├── BM25 search → top K results (default K=50)
  │     [(doc_id, bm25_score), ...]
  │
  ├── Vector search → embed query → cosine similarity → top K
  │     [(doc_id, cosine_score), ...]
  │
  ├── RRF Fusion (k=60)
  │     score = Σ 1/(k + rank) for each ranked list
  │     Merge by doc_id union, sort by RRF score
  │
  └── Top N results (default N=20)
        Vec<SearchResult { doc_id, kind, title, path, score }>
```

RRF advantage: no score normalization needed between BM25 and cosine. Rank-only fusion.

### Context Filtering

- Pick mode: return only `kind == Commit`
- View mode: return only `kind == File` for selected commit
- Diff mode: search disabled

## UI: Semantic Search Modal

### Key Bindings

| Key | Action |
|---|---|
| `S` | Open semantic search modal |
| `j` / `↓` | Next result |
| `k` / `↑` | Previous result |
| `Tab` | Switch section (Files ↔ Commits) |
| `Enter` | Open selected item (close modal, navigate) |
| `Esc` | Close modal, restore previous state |

### Modal Layout

Rendered as overlay popup on current mode. Centered, 70-80% of screen area.

```
┌──────────────────────────────────────────────┐
│  🔍 Semantic Search                          │
│  ┌──────────────────────────────────────────┐│
│  │ 에러 핸들링 로직_                        ││
│  └──────────────────────────────────────────┘│
│                                               │
│  ── Files ──────────────────────────────────  │
│  ● src/error/handler.rs           (0.92)      │
│  ● src/middleware/error.rs        (0.85)      │
│  ● src/parser/mod.rs              (0.73)      │
│                                               │
│  ── Commits ────────────────────────────────  │
│  ● fix: null pointer in parser    (0.88)      │
│  ● refactor error handling        (0.81)      │
│                                               │
│  [Enter] open   [Esc] close   [Tab] section  │
└──────────────────────────────────────────────┘
```

### Modal State

```rust
struct SemanticSearchModal {
    input: String,
    results: Vec<SearchResult>,
    selected: usize,
    focused_section: Section,
    active: bool,
}

enum Section {
    Files,
    Commits,
}
```

### No Index Available

When `.glc-index/` is missing, show:

```
┌──────────────────────────────────┐
│  ⚠ No search index found.       │
│  Run `glc index` to build one.  │
│           [Esc] close           │
└──────────────────────────────────┘
```

### Stale Index Warning

When HEAD mismatches `meta.toml`, show warning in modal: "stale index, results may be incomplete".

## CLI Extension

```rust
enum Commands {
    Index {
        /// Repository path (default: ".")
        path: Option<PathBuf>,
        /// Batch size for embedding generation
        #[arg(long, default_value = "32")]
        batch_size: usize,
        /// Max file size to index in bytes
        #[arg(long, default_value = "1048576")]
        max_file_size: usize,
    },
}
```

`glc [PATH]` TUI launch unchanged. `glc index` added as subcommand.

## Configuration

New `[search]` section in config.toml:

```toml
[search]
model_path = "~/.cache/glc/models/codebert.onnx"
rrf_k = 60
bm25_top_k = 50
vector_top_k = 50
result_limit = 20
```

Model path overridable for custom models.

## Model Management

- First `glc index` run: download CodeBERT ONNX model to `~/.cache/glc/models/`
- Subsequent runs: offline, model loaded from disk
- Model path configurable

## Dependencies

```toml
[dependencies]
tantivy = "0.22"                                    # BM25 full-text search
ort = { version = "2", features = ["load-dynamic"] } # ONNX Runtime
tokenizers = "0.20"                                  # HuggingFace tokenizer for CodeBERT
sha2 = "0.10"                                       # Repo hash for cache directory
```

`ort` uses dynamic loading — users need ONNX Runtime native library installed, or bundled. Only required for `glc index`; TUI search only reads vector files.

## Error Handling

| Scenario | Handling |
|---|---|
| No `.glc-index/` | Modal: "Run `glc index` to build one" |
| Stale index (HEAD mismatch) | Warning in modal |
| Missing ONNX model | `glc index` error + download instructions |
| Empty query | No search executed |
| Binary-only repo | Warning during indexing (0 documents) |
| Large repo (10K+ files) | Progress bar during indexing |
| Zero results | "No results found" in modal |

## Testing

### Unit Tests

| Module | Tests |
|---|---|
| `SearchDocument` | Commit/File construction, tantivy Document conversion |
| `bm25` | Index + query + result verification |
| `vector` | Vector save/load, cosine similarity calculation |
| `rrf` | RRF fusion of two ranked lists |
| `indexer` | Ephemeral repo → build index → verify meta.toml |

### Integration Tests

- `init_test_repo()` + commits/files → run indexer → query → verify results
- Existing test pattern reused

### ONNX Model in Tests

- Embedding tests marked `#[ignore]` when model unavailable
- BM25 + RRF tests work without model (dummy vectors)

## Scope Boundaries (MVP)

**In scope:**
- Full reindexing via `glc index`
- BM25 + vector hybrid search with RRF
- Modal popup UI with j/k navigation
- Commit messages + HEAD file contents

**Out of scope (future):**
- Incremental reindexing
- HNSW or other ANN index for large repos
- Diff mode search
- Multiple embedding model support
- Search result caching between sessions
