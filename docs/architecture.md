# TuckDB Architecture

## System Layers

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         Application / Client                             │
│                                                                          │
│  Rust API: Table::create() .insert_batch() .execute() .flush()           │
│  Future: C FFI, Python bindings, Node bindings                          │
└─────────────────────────────────┬───────────────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────────────┐
│  1. API Layer                                                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌────────────┐ │
│  │    Table     │  │    Schema    │  │    Query     │  │  ResultSet │ │
│  │              │  │              │  │  Builder     │  │            │ │
│  │ create/open  │  │ fields,      │  │ scan/filter/ │  │ next_batch │ │
│  │ insert_batch │  │ index_of     │  │ project/agg  │  │ collect    │ │
│  │ execute      │  │              │  │              │  │            │ │
│  │ flush        │  │              │  │              │  │            │ │
│  └──────────────┘  └──────────────┘  └──────────────┘  └────────────┘ │
└─────────────────────────────────┬───────────────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────────────┐
│  2. Query Engine                                                        │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                        Logical Plan                                 │ │
│  │                                                                      │ │
│  │  Scan { table, projection, filter }   ←── root of every query       │ │
│  │  Filter { input, predicate }          ←── WHERE clause               │ │
│  │  Project { input, columns }           ←── SELECT clause              │ │
│  │  Aggregate { input, aggs, group_by }  ←── GROUP BY                   │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                  │                                       │
│                                  ▼                                       │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                        Optimizer                                    │ │
│  │                                                                      │ │
│  │  • Predicate push-down: WHERE clause → Scan node                     │ │
│  │  • Projection push-down: SELECT columns → Scan node                  │ │
│  │  • Filter combination: merge adjacent filters                        │ │
│  │                                                                      │ │
│  │  Goal: minimize data read from storage layer                         │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                  │                                       │
│                                  ▼                                       │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    Physical Operators (pull-based)                   │ │
│  │                                                                      │ │
│  │  PhysicalScan      reads batches, applies filter + projection        │ │
│  │  PhysicalFilter    evaluates predicate row-by-row                    │ │
│  │  PhysicalProject   selects columns (column pruning)                  │ │
│  │  PhysicalAggregate hash-based group-by + accumulator                 │ │
│  │                                                                      │ │
│  │  Interface: fn next_batch() -> Option<RecordBatch>                   │ │
│  │  Operators compose: Aggregate(Filter(Scan(...)))                     │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────┬───────────────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────────────┐
│  3. Storage & Compression Layer                                         │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  Blob Format (.tuck file)                                           │ │
│  │                                                                      │ │
│  │  ┌───────┬─────────┬────────────┬────────────┬──────────────────┐  │ │
│  │  │ MAGIC │ VERSION │  Schema    │ ChunkMeta  │ Chunk Data       │  │ │
│  │  │ TCKB  │   u32   │ (ser.)     │ [N entries]│ [compressed]     │  │ │
│  │  └───────┴─────────┴────────────┴────────────┴──────────────────┘  │ │
│  │                                                                      │ │
│  │  ChunkMeta = { col_idx, chunk_idx, encoding_id, offset,             │ │
│  │                compressed_size, uncompressed_size, stats }          │ │
│  │                                                                      │ │
│  │  ColumnStats = { min: Option<f64>, max: Option<f64>,               │ │
│  │                  null_count: usize }                                │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  Encodings (pluggable via Encoder/Decoder traits)                   │ │
│  │                                                                      │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │ │
│  │  │   Int64      │  │   Float64    │  │   Utf8                   │  │ │
│  │  │              │  │              │  │                          │  │ │
│  │  │ Delta +      │  │ XOR (Gorilla)│  │ Dictionary + ZSTD        │  │ │
│  │  │ varint       │  │ 1st: raw f64 │  │ 1st: build dict          │  │ │
│  │  │ (zigzag)     │  │ next: xor    │  │ 2nd: store indices       │  │ │
│  │  │              │  │ with prev,   │  │ 3rd: compress all        │  │ │
│  │  │ id: 1        │  │ store lead/  │  │  with ZSTD               │  │ │
│  │  │              │  │ trailing     │  │                          │  │ │
│  │  │              │  │ zeros        │  │ id: 3                    │  │ │
│  │  │              │  │              │  │                          │  │ │
│  │  │              │  │ id: 2        │  │                          │  │ │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  Key property: stats + encodings enable compressed-first execution      │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  Operation      │  On compressed data? │  How                       │ │
│  ├─────────────────┼──────────────────────┼───────────────────────────│ │
│  │  MIN / MAX      │  ✅ Zero decode      │  Read from ColumnStats     │ │
│  │  COUNT          │  ✅ Zero decode      │  Chunk uncompressed size  │ │
│  │  SUM (delta)    │  ⚡ Partial decode   │  Sum deltas + base value  │ │
│  │  AVG            │  ⚡ Partial decode   │  From SUM / COUNT          │ │
│  │  Filter eval    │  ⚡ Per-chunk skip   │  Stats-based chunk skip   │ │
│  │  Filter (row)   │  ❌ Full decompress  │  Need actual values        │ │
│  └─────────────────┴──────────────────────┴───────────────────────────┘ │
└─────────────────────────────────┬───────────────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────────────┐
│  4. Cache Layer                                                         │
│                                                                          │
│  ┌───────────────────────────────┐  ┌────────────────────────────────┐  │
│  │       DataCache                │  │       ResultCache              │  │
│  │                               │  │                                │  │
│  │  Key: (blob, col, chunk)      │  │  Key: (query_hash, version)    │  │
│  │  Value: EncodedChunk          │  │  Value: Vec<RecordBatch>       │  │
│  │                               │  │                                │  │
│  │  Eviction: LRU or LFU         │  │  Invalidation: on insert       │  │
│  │  Size limit: configurable     │  │  Max entries: configurable     │  │
│  │                               │  │                                │  │
│  │  Reduces disk I/O for hot     │  │  Zero-cost repeat queries      │  │
│  │  chunks across queries        │  │  for unchanged data            │  │
│  └───────────────────────────────┘  └────────────────────────────────┘  │
└─────────────────────────────────┬───────────────────────────────────────┘
                                  │
┌─────────────────────────────────▼───────────────────────────────────────┐
│  5. Blob Backend Layer                                                  │
│                                                                          │
│  ┌────────────────────┐  ┌────────────────────┐  ┌──────────────────┐  │
│  │   LocalFS          │  │   S3-compatible    │  │   InMemory       │  │
│  │                    │  │   (planned)        │  │                  │  │
│  │  std::fs::read     │  │   aws-sdk-rust     │  │   HashMap        │  │
│  │  std::fs::write    │  │   HTTP range reads │  │   (tests)        │  │
│  └────────────────────┘  └────────────────────┘  └──────────────────┘  │
│                                                                          │
│  Trait: BlobBackend { read(path, offset, len) -> bytes; ... }          │
│  All backends return bytes; decoding is handled by storage layer        │
└─────────────────────────────────────────────────────────────────────────┘
```

## Data Flow: Full Query Lifecycle

```
Application                                              TuckDB
    │                                                       │
    │  Table::create("metrics", schema, path)               │
    │──────────────────────────────────────────────────────►│
    │                                                       │  Initialize in-memory state
    │◄──────────────────────────────────────────────────────│  Return Table handle
    │                                                       │
    │  table.insert_batch(batch)  (x10)                     │
    │──────────────────────────────────────────────────────►│
    │                                                       │  Store batch in memory
    │                                                       │  Bump data_version
    │                                                       │  Invalidate ResultCache
    │                                                       │
    │  plan = LogicalPlan::scan("metrics")                  │
    │        .filter(col("tag").eq("web"))                  │
    │        .project(&["ts", "metric"])                    │
    │        .aggregate(sum("metric"), ["hour(ts)"])        │
    │                                                       │
    │  results = table.execute(plan)                        │
    │──────────────────────────────────────────────────────►│
    │                                                       │
    │                                    ┌───────────────┐  │
    │                                    │  Optimizer    │  │
    │                                    │  push-downs   │  │
    │                                    └───────┬───────┘  │
    │                                            │          │
    │                                    ┌───────▼───────┐  │
    │                                    │PhysicalScan   │  │
    │                                    │• Check cache  │  │
    │                                    │• Read batches │  │
    │                                    │• Chunk skip   │  │
    │                                    │• Filter rows  │  │
    │                                    │• Project cols │  │
    │                                    └───────┬───────┘  │
    │                                            │          │
    │                                    ┌───────▼───────┐  │
    │                                    │PhysAggregate  │  │
    │                                    │• Hash group-by│  │
    │                                    │• Accumulate   │  │
    │                                    └───────┬───────┘  │
    │                                            │          │
    │◄──────────────────────────────────────────────────────│  Return ResultSet
    │                                                       │
    │  while let Some(batch) = results.next_batch() {       │
    │      process(batch)                                   │
    │  }                                                    │
    │                                                       │
    │  table.flush()                                        │
    │──────────────────────────────────────────────────────►│
    │                                                       │  BlobWriter::write(batches)
    │                                                       │  • Encode each column chunk
    │                                                       │  • Compute stats
    │                                                       │  • Serialize to .tuck file
    │◄──────────────────────────────────────────────────────│  Written to disk
    │                                                       │
    │  table2 = Table::open("metrics", path)                │
    │──────────────────────────────────────────────────────►│
    │                                                       │  BlobReader::read_all(file)
    │                                                       │  • Parse header + schema
    │                                                       │  • Load chunk metadata
    │                                                       │  • Decode chunks into batches
    │◄──────────────────────────────────────────────────────│  Return Table with data
```

## Module Dependency Graph

```
lib.rs
  │
  ├── api
  │    ├── table.rs      → schema, exec, storage, cache
  │    ├── query.rs      → (placeholder)
  │    └── result.rs     → exec
  │
  ├── schema
  │    └── types.rs      → (no deps)
  │
  ├── exec
  │    ├── batch.rs      → schema
  │    ├── expr.rs       → batch
  │    ├── logical_plan  → expr
  │    ├── optimizer.rs  → logical_plan, schema
  │    └── physical_plan → batch, expr, logical_plan
  │
  ├── storage
  │    ├── format.rs     → schema, exec, encoding, chunk
  │    ├── chunk.rs      → (no deps)
  │    ├── manager.rs    → format
  │    └── encoding/
  │         ├── mod.rs   → batch, chunk, int64, float64, utf8
  │         ├── int64.rs → batch
  │         ├── float64  → batch
  │         └── utf8.rs  → batch, zstd
  │
  ├── cache
  │    ├── data_cache    → storage
  │    ├── result_cache  → exec
  │    └── policy.rs     → (no deps)
  │
  ├── lifecycle           ← TuckDB-AI
  │    ├── state.rs      → (no deps)          # versions, content hash, Active/Stale
  │    ├── dedup.rs      → state              # exact content dedup (FNV-1a)
  │    └── chunk.rs      → state              # paragraph chunker + per-chunk diff
  │
  ├── embedding           ← TuckDB-AI
  │    ├── provider.rs   → lifecycle (fnv1a)  # EmbeddingProvider + mock provider
  │    ├── store.rs      → (no deps)          # vector store, cosine top-K, persistence
  │    └── dedup.rs      → store              # semantic duplicate candidates
  │
  ├── incremental         ← TuckDB-AI
  │    └── mod.rs        → lifecycle, embedding   # IncrementalEngine: ingest/update/delete/migrate + persistence
  │
  └── backend
       ├── traits.rs     → (no deps)
       ├── local_fs.rs   → traits
       └── memory.rs     → traits
```

## Current Implementation Status

```
Phase 1  ✅  Core types + InMemTable (create, insert, scan)
Phase 2  ✅  Compressed blob format + three encodings
Phase 3  ✅  Query engine + optimizer + all operators
Phase 4  ✅  Data cache (LRU) + Result cache
Phase 5  ⬜  Error handling, config, production hardening
Phase 6  ⬜  RLE encoding, workload tracker, benchmarks
```
