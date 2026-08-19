# TuckDB-AI — Progress: Proposed vs Implemented

**Tagline:** *Process what changed. Leave everything else untouched.*

An incremental AI data engine on top of TuckDB: AI representations (chunks,
embeddings, vectors) are derived from database data and maintained
incrementally as the source data changes.

All code is Rust (edition 2024). Build/test: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`.

---

## Phase-by-phase evidence

| # | What was proposed | What was implemented | Where (source) | Evidence |
|---|---|---|---|---|
| 0 | Understand the existing TuckDB architecture | Architecture report; **no code changed** | `docs/architecture.md` | — |
| 1 | Simple lifecycle metadata (id, version, content hash, state) | `LifecycleState`, `SourceState`, `LifecycleManager::register/apply_update` (identical content → no version bump / no work) | `src/lifecycle/state.rs` | `examples/lifecycle_demo`; unit + integration tests |
| 2 | Exact content deduplication | `ContentDedup` keyed by FNV-1a content hash; counts unique/duplicate/ratio, exposes canonical content | `src/lifecycle/dedup.rs` | `examples/dedup_demo`; 10,000-input test (9,000 unique / 1,000 dup) |
| 3 | Chunk lifecycle: only changed chunks reprocessed | Paragraph-based deterministic chunker; `ChunkTracker` diffs chunk hashes across versions → SKIP/PROCESS | `src/lifecycle/chunk.rs` | `examples/chunk_demo` (3 chunks, 1 changed → 66.7% avoided) |
| 4 | Embedding abstraction + deterministic mock provider | `EmbeddingProvider` trait; `MockEmbeddingProvider` (feature-hash + small lexicon, 128-d, normalized); `Embedding` + `EmbeddingStore` | `src/embedding/provider.rs`, `src/embedding/store.rs` | `examples/embedding_demo` |
| 5 | Vector storage + brute-force cosine search | `EmbeddingStore::search` (top-K, cosine); `VectorMatch` | `src/embedding/store.rs` | `examples/vector_search_demo` (query retrieves "move constructor" without shared words) |
| 6 | SQL-visible vector operator | `Expr::Similarity { column, query }`; evaluated as cosine in `eval_expr`; parsed by CLI + C API `SIMILARITY(col, 'query')` | `src/exec/expr.rs`, `examples/cli/main.rs`, `src/capi/mod.rs` | `examples/sql_vector_demo`; CLI query |
| 7 | Hybrid SQL + vector query | Composes via `Expr::And`/`Eq`/`Similarity`; added `LIMIT` (`LogicalPlan::Limit`, `PhysicalLimit`) | `src/exec/logical_plan.rs`, `src/exec/physical_plan.rs` | `examples/hybrid_demo` (topic filter + similarity, LIMIT 5) |
| 8 | Incremental vector update | `IncrementalEngine` ties lifecycle + chunks + store + provider; `update()` re-embeds **only** changed chunks | `src/incremental/mod.rs` | `examples/incremental_demo`; 500,000 chunks / 1,000 changed → **99.8% work avoided**, 5 ms |
| 9 | DELETE / stale cleanup | `IncrementalEngine::delete()` resolves affected chunks directly (no full vector scan), removes vectors + lifecycle state | `src/incremental/mod.rs` | `examples/delete_demo` (10k docs / 30k vectors; delete 100 → 300 vectors, untouched 29,700) |
| 10 | Semantic deduplication (report-only) | `SemanticDedup` pairwise cosine over stored embeddings, deterministic ordering, candidates only | `src/embedding/dedup.rs` | `examples/semantic_dedup_demo` (RAII paraphrase pair 0.80; nothing deleted) |
| 11 | Embedding model versioning + incremental migration | Every vector stamps its `model_id`; `migrate_model()` re-embeds only old-model vectors, preserves `source_version`, idempotent | `src/incremental/mod.rs` | `examples/model_migration_demo` (v1→v2 migrates all, re-run migrates 0) |

## Cross-cutting

| Concern | Implementation |
|---|---|
| Document/source ID | `i64` document id + `u32` chunk id keying `(document_id, chunk_id)` |
| Version | `u64` per document (`SourceState.version`); each chunk embeds carry `source_version` |
| Content hash | FNV-1a 64-bit (`lifecycle::fnv1a`) over row/paragraph bytes |
| Lifecycle state | `Active` / `Stale` (`LifecycleState`) |
| Dependency metadata | `ChunkTracker` maps doc → chunks; enables direct (non-scanning) update/delete/migration |
| Vector search | Brute-force cosine (correctness-first; ANN deferred) |
| SQL integration | `SIMILARITY(column, 'query')` expression + `LIMIT`; parsers in CLI + C API |
| Mock provider | Deterministic, dependency-free; pluggable via `EmbeddingProvider` trait |

## Test inventory (current)

- Unit tests: 69 (lifecycle, dedup, chunking, embedding provider/store, engine ingest/update/delete/migrate/persist, semantic dedup)
- Integration tests: 30 (SQL/filters/aggregation, similarity filter, hybrid + limit, incremental update/delete/migration, semantic dedup)
- Benchmarks: criterion groups `encoding`, `query`, `cache`, `persistence`, `dedup`, `incremental`
- Live demos: 15 (`examples/*_demo`, `examples/showcase`, `examples/benchmark_report`)
- Key benchmark: `examples/benchmark_report` (see `target/benchmark_report.md`)

## Benchmark snapshot

`cargo run --release --example benchmark_report` (mock model-v1, 128 dims, 5 chunks/doc).

| scale | chunks | changed | regenerated | work avoided | speedup vs naive |
|-------|--------|---------|-------------|--------------|------------------|
| 1,000 | 5,000 | 1% | 50 | 99.0% | 73.0x |
| 10,000 | 50,000 | 1% | 500 | 99.0% | 74.1x |
| 100,000 | 500,000 | 1% | 5,000 | **99.0%** | **72.7x** |
| 100,000 | 500,000 | 5% | 25,000 | 95.0% | 18.2x |
| 100,000 | 500,000 | 10% | 50,000 | 90.0% | 14.6x |

Headline case: **500,000 chunks, 1,000 changed → 1,000 processed, 499,000 skipped
→ 99.8% work avoided** (`examples/incremental_demo`).

Deep dive: **[docs/VECTOR_DB.md](VECTOR_DB.md)**.

## Live run

```bash
cargo test                 # correctness
cargo run --release --example showcase          # full live demo
cargo run --release --example benchmark_report  # scaling benchmark table
cargo run --example file_index_demo -- --file data/cpp_guide.txt   # real text -> chunks -> vector DB -> interactive questions
cargo run --example unified_demo -- --csv data/cpp_questions.csv   # real CSV -> .tuck + vector DB, query-driven routing
```

Sample data files live in `data/` (`cpp_guide.txt`, `cpp_questions.csv`); point either demo at your own `.txt`/`.md`/`.pdb`/`.csv`.

## Not yet implemented (deferred by design)

- True SQL `UPDATE` / `DELETE` DML statements (lifecycle is driven via the API today)
- Approximate nearest neighbor (ANN) index
- Real (non-mock) embedding provider / LLM integration
- Automatic application of safe semantic-dedup policies
