# Search Quality Report

- Generated: 2026-05-27T02:20:37Z
- HEAD (working tree): 8d8fe31d2d00aa3169aba2c12c77b0206e2f439c
- Index dir: ./.glc-index (698.92 KiB, 407 docs)

## Aggregate

| Metric | Value |
|--------|-------|
| MRR | 0.544 |
| Recall@5 | 0.857 |
| Recall@10 | 1.000 |
| NDCG@10 | 0.654 |
| Queries | 7 |

## Performance (warmup=3, iters=10)

| p50 | p95 | p99* | mean | QPS |
|-----|-----|------|------|-----|
| 0.28 ms | 0.33 ms | 0.34 ms | 0.25 ms | 4001.5 |

\* iters=10 표본에서 p99는 표본 최댓값 근사

## Index

- Embedding: minishlab/potion-multilingual-128M (256-dim)
- BM25 tokenizer: ngram_2_2
- Vector backend: turboquant_4bit
- HEAD: 8d8fe31d2d00aa3169aba2c12c77b0206e2f439c (indexed 1779848421Z)
- Docs: Commit=248, File=77, Symbol=82

## Per-Query

| # | Query | MRR | R@5 | R@10 | NDCG@10 | Hit Rank | Hit Paths |
|---|-------|-----|-----|------|---------|----------|-----------|
| 1 | incremental indexing fallback | 0.500 | 1.000 | 1.000 | 0.631 | 2 | src/search/indexer.rs |
| 2 | tantivy delete_term | 1.000 | 1.000 | 1.000 | 1.000 | 1 | src/search/bm25.rs |
| 3 | RRF reciprocal rank fusion | 0.200 | 1.000 | 1.000 | 0.387 | 5 | src/search/rrf.rs |
| 4 | embedding model load potion | 1.000 | 1.000 | 1.000 | 1.000 | 1 | src/search/embedding.rs |
| 5 | search modal state machine | 0.500 | 1.000 | 1.000 | 0.631 | 2 | src/search/modal_state.rs |
| 6 | tree sitter highlight configuration | 0.500 | 1.000 | 1.000 | 0.631 | 2 | src/highlight/engine.rs |
| 7 | git revwalk topological commit | 0.111 | 0.000 | 1.000 | 0.301 | 9 | src/git/store.rs |
