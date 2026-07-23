use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tuckdb::cache::policy::EvictionPolicy;
use tuckdb::cache::DataCache;
use tuckdb::exec::batch::{Column, ColumnData, RecordBatch};
use tuckdb::exec::expr::{col, lit_float};
use tuckdb::exec::logical_plan::AggOp;
use tuckdb::exec::physical_plan::{
    PhysicalAggregate, PhysicalFilter, PhysicalOperator, PhysicalProject, PhysicalScan,
};
use tuckdb::schema::{DataType, Field, Schema};
use tuckdb::storage::chunk::{ChunkMeta, ColumnStats, EncodedChunk};
use tuckdb::storage::encoding;
use tuckdb::storage::encoding::{bitmap, float64, int64, rle, utf8};
use tuckdb::storage::format::{BlobReader, BlobWriter};

// ---------------------------------------------------------------------------
// Deterministic PRNG (xorshift64*)
// ---------------------------------------------------------------------------
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0 = self.0.wrapping_mul(0x2545F4914F6CDD1D);
        self.0
    }
    fn next_i64(&mut self) -> i64 {
        self.next_u64() as i64
    }
    fn next_f64(&mut self) -> f64 {
        f64::from_bits(0x3FF0000000000000 | (self.next_u64() >> 12))
            .mul_add(1000.0, -500.0)
    }
}

// ---------------------------------------------------------------------------
// Test data generators
// ---------------------------------------------------------------------------
fn seq_ints(n: usize) -> ColumnData {
    ColumnData::Int64((0..n).map(|i| i as i64).collect())
}

fn rand_ints(n: usize, seed: u64) -> ColumnData {
    let mut r = Rng::new(seed);
    ColumnData::Int64((0..n).map(|_| r.next_i64()).collect())
}

fn ts_floats(n: usize) -> ColumnData {
    let mut v = 100.0;
    let mut vals = Vec::with_capacity(n);
    for _ in 0..n {
        vals.push(v);
        v += v * 0.001 * 0.5;
    }
    ColumnData::Float64(vals)
}

fn rand_floats(n: usize, seed: u64) -> ColumnData {
    let mut r = Rng::new(seed);
    ColumnData::Float64((0..n).map(|_| r.next_f64()).collect())
}

fn strings(n: usize, distinct: usize) -> ColumnData {
    let mut r = Rng::new(42);
    let dict: Vec<String> = (0..distinct).map(|i| format!("val_{}", i)).collect();
    ColumnData::Utf8((0..n).map(|_| dict[(r.next_u64() as usize) % dict.len()].clone()).collect())
}

fn bitmap_data(n: usize, seed: u64) -> ColumnData {
    let mut r = Rng::new(seed);
    ColumnData::Int64((0..n).map(|_| (r.next_u64() & 1) as i64).collect())
}

fn rle_data(n: usize, run: usize, seed: u64) -> ColumnData {
    let mut r = Rng::new(seed);
    let mut vals = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        let v = r.next_i64() % 100;
        let lim = run.min(n - i);
        for _ in 0..lim {
            vals.push(v);
        }
        i += lim;
    }
    ColumnData::Int64(vals)
}

// ---------------------------------------------------------------------------
// Helper: run an encoder once and print stats
// ---------------------------------------------------------------------------
fn bench_encode(
    c: &mut Criterion,
    label: &str,
    size: usize,
    data: &ColumnData,
    encoder: &dyn tuckdb::storage::encoding::Encoder,
    raw_bytes: u64,
) {
    let compressed = encoder.encode(data);
    let ratio = raw_bytes as f64 / compressed.len() as f64;
    eprintln!("  {label:<18}  ratio={ratio:5.1}x  raw={raw_bytes:>8}  cmp={:>8}", compressed.len());

    let mut g = c.benchmark_group(format!("encode/{label}"));
    g.throughput(Throughput::Bytes(raw_bytes));
    g.bench_with_input(BenchmarkId::new("size", size), &data, |b, d| {
        b.iter(|| black_box(encoder.encode(d)));
    });
    g.finish();

    // reuse compressed data for decode benchmark
    let mut g = c.benchmark_group(format!("decode/{label}"));
    g.throughput(Throughput::Bytes(raw_bytes));
    g.bench_with_input(BenchmarkId::new("size", size), &(compressed, size), |b, (cdata, sz)| {
        b.iter(|| black_box(tuckdb::storage::encoding::decode_column(encoder.encoding_id(), cdata, *sz)));
    });
    g.finish();
}

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

fn encoding_benchmarks(c: &mut Criterion) {
    let sizes: &[usize] = &[100_000, 1_000_000];

    for &size in sizes {
        // Delta+varint – sequential ints
        let data = seq_ints(size);
        let raw_bytes = (size * 8) as u64;
        bench_encode(c, "delta_varint/seq", size, &data, &int64::DeltaBitpackEncoder, raw_bytes);

        // Delta+varint – random ints
        let data = rand_ints(size, 0xdead);
        bench_encode(c, "delta_varint/rand", size, &data, &int64::DeltaBitpackEncoder, raw_bytes);

        // XOR – time-series floats
        let data = ts_floats(size);
        let raw_bytes = (size * 8) as u64;
        bench_encode(c, "xor/ts", size, &data, &float64::XorEncoder, raw_bytes);

        // XOR – random floats
        let data = rand_floats(size, 0xbeef);
        bench_encode(c, "xor/rand", size, &data, &float64::XorEncoder, raw_bytes);

        // Dict+ZSTD – high-repetition strings
        let data = strings(size, size / 100);          // 1% distinct
        let raw_bytes = data.len() as u64 * 16;        // approx
        bench_encode(c, "dict_zstd/highrep", size, &data, &utf8::DictZstdEncoder, raw_bytes);

        // Dict+ZSTD – low-repetition strings
        let data = strings(size, size / 2);             // 50% distinct
        let raw_bytes = data.len() as u64 * 16;
        bench_encode(c, "dict_zstd/lowrep", size, &data, &utf8::DictZstdEncoder, raw_bytes);

        // RLE
        let data = rle_data(size, 50, 0xcafe);
        let raw_bytes = (size * 8) as u64;
        bench_encode(c, "rle/run50", size, &data, &rle::RleEncoder, raw_bytes);

        // Bitmap
        let data = bitmap_data(size, 0xface);
        let raw_bytes = (size * 8) as u64;
        bench_encode(c, "bitmap", size, &data, &bitmap::BitmapEncoder, raw_bytes);
    }
}

fn query_benchmarks(c: &mut Criterion) {
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("category", DataType::Utf8, false),
        Field::new("value", DataType::Float64, true),
        Field::new("ts", DataType::Timestamp, false),
    ]);

    let sizes: &[usize] = &[100_000]; // 1M/10M can be added, but 100K keeps bench time practical

    for &size in sizes {
        let num_batches = 10;
        let bs = size / num_batches;
        let mut batches = Vec::with_capacity(num_batches);

        for b in 0..num_batches {
            let base = b * bs;
            let mut r = Rng::new(b as u64);
            let ids: Vec<i64> = (base..base + bs).map(|i| i as i64).collect();
            let cats: Vec<String> = (0..bs).map(|_| format!("cat_{}", r.next_u64() % 10)).collect();
            let vals: Vec<f64> = (0..bs).map(|_| r.next_f64()).collect();
            let ts: Vec<i64> = (base as i64..(base + bs) as i64).collect();
            batches.push(RecordBatch::new(
                schema.clone(),
                vec![
                    Column::new(schema.fields[0].clone(), ColumnData::Int64(ids)),
                    Column::new(schema.fields[1].clone(), ColumnData::Utf8(cats)),
                    Column::new(schema.fields[2].clone(), ColumnData::Float64(vals)),
                    Column::new(schema.fields[3].clone(), ColumnData::Timestamp(ts)),
                ],
            ));
        }

        // Full scan
        let mut g = c.benchmark_group(format!("query/{}", size));
        g.throughput(Throughput::Elements(size as u64));
        g.bench_with_input(BenchmarkId::new("full_scan", size), &batches, |b, data| {
            b.iter(|| {
                let mut op = PhysicalScan::new(data.clone(), vec![], None);
                let mut n = 0usize;
                while let Some(batch) = op.next_batch() {
                    n += batch.num_rows;
                }
                black_box(n);
            });
        });

        // Filter 50% (non-selective)
        g.bench_with_input(BenchmarkId::new("filter_50pct", size), &batches, |b, data| {
            b.iter(|| {
                let scan = PhysicalScan::new(data.clone(), vec![], None);
                let mut op = PhysicalFilter::new(Box::new(scan), col("value").gt(lit_float(0.0)));
                let mut n = 0usize;
                while let Some(batch) = op.next_batch() {
                    n += batch.num_rows;
                }
                black_box(n);
            });
        });

        // Filter 10% (selective)
        g.bench_with_input(BenchmarkId::new("filter_10pct", size), &batches, |b, data| {
            b.iter(|| {
                let scan = PhysicalScan::new(data.clone(), vec![], None);
                let mut op =
                    PhysicalFilter::new(Box::new(scan), col("value").gt(lit_float(900.0)));
                let mut n = 0usize;
                while let Some(batch) = op.next_batch() {
                    n += batch.num_rows;
                }
                black_box(n);
            });
        });

        // Aggregation (GROUP BY category, SUM(value), COUNT(*), AVG(value))
        g.bench_with_input(BenchmarkId::new("aggregation", size), &batches, |b, data| {
            b.iter(|| {
                let scan = PhysicalScan::new(data.clone(), vec![], None);
                let mut op = PhysicalAggregate::new(
                    Box::new(scan),
                    vec![
                        (AggOp::Sum, "value".to_string(), "total".to_string()),
                        (AggOp::Count, "id".to_string(), "cnt".to_string()),
                        (AggOp::Avg, "value".to_string(), "avg_val".to_string()),
                    ],
                    vec!["category".to_string()],
                );
                let mut n = 0usize;
                while let Some(batch) = op.next_batch() {
                    n += batch.num_rows;
                }
                black_box(n);
            });
        });

        // Combined: filter + project + aggregate
        g.bench_with_input(BenchmarkId::new("combined", size), &batches, |b, data| {
            b.iter(|| {
                let scan = PhysicalScan::new(data.clone(), vec![], None);
                let flt = PhysicalFilter::new(Box::new(scan), col("value").gt(lit_float(100.0)));
                let proj = PhysicalProject::new(
                    Box::new(flt),
                    vec!["category".to_string(), "value".to_string()],
                );
                let mut op = PhysicalAggregate::new(
                    Box::new(proj),
                    vec![(AggOp::Avg, "value".to_string(), "avg_val".to_string())],
                    vec!["category".to_string()],
                );
                let mut n = 0usize;
                while let Some(batch) = op.next_batch() {
                    n += batch.num_rows;
                }
                black_box(n);
            });
        });

        g.finish();
    }
}

fn cache_benchmarks(c: &mut Criterion) {
    // Build a chunk that fits in cache
    let data = seq_ints(10_000);
    let (enc_id, compressed) = encoding::encode_column(&data);
    let meta = ChunkMeta {
        col_idx: 0,
        chunk_idx: 0,
        encoding: enc_id,
        offset: 0,
        compressed_size: compressed.len() as u64,
        uncompressed_size: 10_000,
        stats: ColumnStats::new(None, None, 0),
    };
    let chunk = EncodedChunk::new(meta, compressed);

    // Cold access (insert + immediate get)
    let mut cache = DataCache::new(1024 * 1024 * 100, EvictionPolicy::LRU);
    let mut g = c.benchmark_group("cache");
    g.bench_function("cold_access", |b| {
        b.iter(|| {
            cache.clear();
            cache.insert("t", 0, 0, chunk.clone());
            black_box(cache.get("t", 0, 0));
        });
    });

    // Hot access (already inserted)
    cache.insert("t", 0, 0, chunk.clone());
    g.bench_function("hot_access", |b| {
        b.iter(|| {
            black_box(cache.get("t", 0, 0));
        });
    });

    // Hit rate: insert 1000 chunks, then access 1000 random keys (~33% miss)
    cache.clear();
    for i in 0..1000 {
        let d = seq_ints(1000);
        let (eid, cmp) = encoding::encode_column(&d);
        let m = ChunkMeta {
            col_idx: 0,
            chunk_idx: i,
            encoding: eid,
            offset: 0,
            compressed_size: cmp.len() as u64,
            uncompressed_size: 1000,
            stats: ColumnStats::new(None, None, 0),
        };
        cache.insert("hit", 0, i, EncodedChunk::new(m, cmp));
    }
    g.bench_function("hit_rate", |b| {
        b.iter(|| {
            let mut r = Rng::new(42);
            let mut hits = 0u64;
            let mut misses = 0u64;
            for _ in 0..1000 {
                let idx = r.next_u64() % 1500;
                if cache.get("hit", 0, idx as u32).is_some() {
                    hits += 1;
                } else {
                    misses += 1;
                }
            }
            black_box((hits, misses));
        });
    });

    g.finish();
}

fn persistence_benchmarks(c: &mut Criterion) {
    // Build a multi-column batch
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Float64, true),
        Field::new("label", DataType::Utf8, false),
    ]);

    let n = 100_000usize;
    let ids: Vec<i64> = (0..n).map(|i| i as i64).collect();
    let vals: Vec<f64> = (0..n).map(|i| i as f64 * 1.5).collect();
    let labs: Vec<String> = (0..n).map(|i| format!("row_{}", i % 1000)).collect();
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(ids)),
            Column::new(schema.fields[1].clone(), ColumnData::Float64(vals)),
            Column::new(schema.fields[2].clone(), ColumnData::Utf8(labs)),
        ],
    );
    let batches = vec![batch];

    // Flush
    let mut g = c.benchmark_group("persistence");
    g.throughput(Throughput::Bytes((n * (8 + 8 + 16)) as u64));
    g.bench_function("flush_to_blob", |b| {
        b.iter(|| {
            black_box(BlobWriter::write(&batches));
        });
    });

    // Read back
    let blob = BlobWriter::write(&batches);
    let blob_size = blob.len();
    eprintln!("  blob size: {blob_size} bytes for {n} rows");
    g.bench_function("read_from_blob", |b| {
        b.iter(|| {
            let out = BlobReader::read_all(&blob);
            black_box(out.len());
        });
    });
    g.finish();
}

criterion_group!(
    benches,
    encoding_benchmarks,
    query_benchmarks,
    cache_benchmarks,
    persistence_benchmarks
);
criterion_main!(benches);
