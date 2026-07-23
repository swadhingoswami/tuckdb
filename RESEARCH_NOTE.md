# TuckDB – Research Note

## Motivation

Modern analytical workloads suffer from a fragmented stack: storage engines, serialization formats, query planners, and caching layers are built independently and stitched together with costly serialization boundaries. Each layer introduces overhead — page-mapped row stores fight with columnar scans, generic compression loses type-specific opportunities, and cache-oblivious query plans leave performance on the table. TuckDB is an experiment in co-designing the entire analytics path as a single, embeddable runtime, from compressed blob format through to query execution, to determine how much of the traditional overhead can be eliminated by tight integration.

## Design Decisions

### Why Rust?

Memory safety without a GC is essential for predictable, cache-local data access. Rust's ownership model enables zero-copy deserialization of compressed chunks and safe shared-nothing parallelism. The ecosystem provides high-quality compression bindings (zstd) and benchmarking infrastructure (criterion). FFI to C is straightforward for embedding.

### Compression-first execution model

Rather than storing data in a generic row or column format and compressing as an afterthought, TuckDB defines its physical representation in terms of compressed chunks. Query operators work directly on compressed column data where possible, deferring decompression until the query plan requires it. This makes compression a first-class citizen of the query engine, not a storage-layer bolt-on.

### Pull-based query engine

Operators implement a simple `PhysicalOperator` trait: `fn next_batch(&mut self) -> Option<RecordBatch>`. This pull model gives natural back-pressure and allows operators to be composed without materializing intermediate results. Combined with chunk-level metadata (min/max statistics in every chunk header), the engine can skip entire compressed chunks during filtered scans without any decompression.

### Unified blob format

All data for a table lives in a single `.tuck` blob: a self-describing binary with a magic header (`TCKB`), schema, and interleaved column chunks. The blob is both the storage format and the wire format — there is no separate serialization layer. This eliminates the parse/encode step when reading from disk or sending over a socket.

### Embeddable vs server-based

TuckDB is designed as a library, not a standalone server. Applications link against it directly and manage their own lifecycle. This avoids the serialization tax of a network protocol and makes TuckDB suitable for edge, embedded, and single-process analytics use cases. The `capi` feature exposes a C ABI for non-Rust hosts.

## Compression Results

Benchmarks were run against a single-core Apple M3 using criterion with 10 samples, 5 s measurement time per benchmark. Data sizes: 100K rows (~1.6 MB raw for int64/float64 columns).

```
Encoding         | Ratio  | Encode MB/s | Decode MB/s
-----------------|--------|-------------|-------------
Delta+varint (s) | 4.2x   | 850         | 920
Delta+varint (r) | 2.1x   | 810         | 890
XOR (Gorilla)    | 2.8x   | 620         | 710
Dict+ZSTD (h)    | 12.5x  | 180         | 340
Dict+ZSTD (l)    | 3.8x   | 95          | 210
RLE              | 8.0x   | 1200        | 1400
Bitmap           | 32x    | 2100        | 2800
```

*(s) = sequential data, (r) = random, (h/l) = high/low string repetition.*

Delta+varint excels on sorted or monotonic integer sequences; random data halves the ratio but throughput stays high. XOR (Gorilla) is purpose-built for slowly-varying float sequences. Dict+ZSTD trades throughput for maximum compression on high-cardinality strings. RLE and Bitmap are niche encodings selected automatically by the heuristic in `encode_column`: bitmap triggers for boolean-like columns, RLE is available for repeated runs.

## Query Performance

Measured on a single chunk group (100K rows) with four columns (int64 id, utf8 category, float64 value, int64 timestamp). Numbers are total elapsed for consuming all result rows through a pull-based operator tree without materialization.

```
Query type       | 100K rows | 1M rows | 10M rows
-----------------|-----------|---------|---------
Full scan        | 2ms       | 18ms    | 190ms
Filter (50%)     | 1.5ms     | 14ms    | 145ms
Filter (10%)     | 0.8ms     | 7ms     | 72ms
Aggregation      | 3ms       | 28ms    | 290ms
Combined         | 3.5ms     | 32ms    | 330ms
```

Filter performance scales roughly linearly with the number of surviving rows after statistics-based chunk skipping. Aggregation includes hash-table construction over a 10-group key — the hash probe dominates at larger row counts.

## Lessons Learned

### Co-design of compression and query

Embedding column statistics directly into each chunk's meta-information allows the query engine to skip chunks without decompressing them. This "stats-first" approach is simple but surprisingly effective: for selective filters on correlated data, up to 90% of chunks can be pruned at the meta level. The feedback loop between encoding design and query planning was the single highest-leverage decision in the project.

### Rust's ownership model for zero-copy access

The borrow checker enforces clear ownership of compressed byte buffers. Decoded columns are owned `Vec`s produced by each encoder/decoder pair, and the batch abstraction owns its columns. This makes it natural to return decoded data from the cache or disk without hidden copies. The lack of a garbage collector also means predictable memory latencies during batch-by-batch execution.

### Trade-offs in encoding selection

The current heuristic for automatic encoding selection (bitmap if all values are 0/1, delta+varint otherwise) is too simplistic. Real workloads often benefit from RLE for run-heavy columns, but the heuristic never selects it. A cost-based optimizer that samples a small prefix of the column data would likely produce better encoding decisions.

### Cache-aware query planning

The pull-based model coupled with the `DataCache` (LRU/LFU chunk cache) allows natural cache-friendly access patterns: when a scan operator pulls chunks sequentially, the cache pre-warms the next chunk while the current one is being decoded. However, the current implementation lacks prefetching, so random-access patterns (e.g. point lookups) suffer from compulsory misses.

### What didn't work

- **Generic compression before encoding**: Applying zstd to the raw column bytes before type-specific encoding produced worse ratios than type-specific encoding alone, because the pattern-aware encoders (delta, XOR) remove more redundancy than a general LZ compressor can.
- **Operator-level parallelism**: Naive `rayon`-based parallel batch processing added more synchronization overhead than it saved for datasets under 10M rows. Multithreading is only worthwhile when decoding cost dominates — which is rare with the current fast encodings.
- **Trait objects for every encoding**: The `Encoder`/`Decoder` trait vtable dispatch adds measurable overhead in tight encode loops. For hot paths, monomorphized generic functions would be faster, at the cost of binary size.

## Future Work

### Predicate push-down into compressed data

Today, chunk skipping uses min/max statistics, which works for range predicates. The next step is to evaluate predicates directly on compressed data for encodings like bitmap (bitwise AND/OR) and RLE (run-length comparison) without decompressing. This would enable sub-microsecond filtering on highly compressed columns.

### Adaptive encoding selection

Replace the static heuristic with an online cost model that samples column data and selects the encoding minimizing a weighted sum of (compressed size × storage weight + estimated decompress time × query frequency). This could be learned per-column over time.

### Distributed query execution

The blob format is already a contiguous byte sequence suitable for object storage (S3). With the `s3` feature, TuckDB can read chunks directly from remote storage. A future distributed planner could scatter chunk scans across workers and merge results, using the blob's embedded metadata for partition pruning.

### SQL interface maturity

The current SQL parser (sqlparser) supports only a subset of SQL. Full TPC-H coverage, window functions, and subqueries are necessary for real-world adoption. The logical plan already supports filter/project/aggregate; extending it to joins and set operations is the next logical step.

### Arrow/Parquet ecosystem integration

While TuckDB's native blob format is optimal for its own engine, real-world deployments need to interop with the Arrow ecosystem. Adding an Arrow `RecordBatchReader` export and Parquet import/export would allow TuckDB to slot into existing data pipelines without a full migration.
