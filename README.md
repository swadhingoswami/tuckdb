<img width="1376" height="768" alt="Gemini_Generated_Image_ehre03ehre03ehre" src="https://github.com/user-attachments/assets/44d6b0ec-0164-43b8-820a-edb6f46ec6c7" />
<img width="2048" height="1920" alt="licensed-image_1" src="https://github.com/user-attachments/assets/4cef399a-392a-4baa-9557-b0945d80db07" />

<p align="center">
  <img src="https://img.shields.io/badge/status-beta-yellow" alt="Status">
  <img src="https://img.shields.io/badge/rust-1.85%2B-orange" alt="Rust">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License">
  <img src="https://img.shields.io/badge/platform-linux%20%7C%20macOS%20%7C%20windows-lightgrey" alt="Platform">
  <img src="https://img.shields.io/badge/CI-passing-brightgreen" alt="CI">
</p>

<h1 align="center">📦 TuckDB</h1>

<p align="center">
  <b>One Rust library = Storage + Compression + Query Engine + Cache</b><br>
  <i>Embed analytics anywhere — edge devices, microservices, servers, cloud.</i>
</p>

<p align="center">
  <code>13 GB/s encode throughput</code> •
  <code>32x compression on booleans</code> •
  <code>zero server processes</code> •
  <code>6 column encodings</code> •
  <code>incremental AI engine</code> •
  <code>C API</code> •
  <code>S3 backend</code>
</p>

<p align="center">
  <a href="#-try-it-in-30-seconds"><b>🚀 Try it in 30s</b></a> •
  <a href="#-tuckdb-ai--incremental-ai-data-engine"><b>🤖 TuckDB-AI</b></a> •
  <a href="#-features-at-a-glance">Features</a> •
  <a href="#-architecture">Architecture</a> •
  <a href="#-performance">Performance</a> •
  <a href="#-use-cases">Use Cases</a> •
  <a href="WHITEPAPER.md">📄 Whitepaper</a>
</p>

---

## 🚀 Try it in 30 seconds

```bash
# 1. Load a CSV file
cargo run --example cli -- load data.csv --table mydata --dir /tmp/db

# 2. Query with SQL
cargo run --example cli -- query "SELECT city, AVG(temp) FROM mydata \
  WHERE temp > 20 GROUP BY city" --dir /tmp/db
```

```rust
// Or embed directly in your Rust app
use tuckdb::*;

let mut table = Table::create("metrics", schema, path);
table.insert_batch(batch);
table.flush();                                    // → compressed .tuck file

let results = table.execute(                      // → filtered, aggregated
    LogicalPlan::scan("metrics")
        .filter(col("cpu").gt(lit_float(90.0)))
        .aggregate(vec![(AggOp::Avg, "cpu", "avg_cpu")], vec!["host".into()])
);
```

**No server. No config. One dependency (`zstd`).**

### 👀 See it in action

```bash
$ cargo run --example cli -- scan users --dir /tmp/tuckdb
| id |  name   | score | active |
|----|---------|-------|--------|
| 1  | Alice   | 85.50 | 1      |
| 2  | Bob     | 92.0  | 1      |
| 3  | Charlie | 78.30 | 0      |
| 4  | Diana   | 95.0  | 0      |
| 5  | Eve     | 88.10 | 1      |
(5 rows)

$ cargo run --example cli -- query "SELECT name, score FROM users WHERE active = 1" --dir /tmp/tuckdb
| name  | score |
|-------|-------|
| Alice | 85.50 |
| Bob   | 92.0  |
| Eve   | 88.10 |
(3 rows)

$ cargo run --example cli -- query "SELECT active, COUNT(*) as cnt, AVG(score) as avg_score FROM users GROUP BY active" --dir /tmp/tuckdb
| active | cnt | avg_score |
|--------|-----|-----------|
| 1      | 3.0 | 88.53     |
| 0      | 2.0 | 86.65     |
(2 rows)

$ cargo run --example cli -- info users --dir /tmp/tuckdb
Table: users
Columns:
  id: Int64
  name: Utf8
  score: Float64
  active: Int64
Row count: 5
Data version: 1
File size: 644 bytes
```

---

## 🤖 TuckDB-AI — Incremental AI Data Engine

> **A small change in AI data can trigger expensive reprocessing of large datasets.**
>
> **TuckDB-AI detects exactly what changed and updates only the affected data.**

TuckDB now extends into a **unified SQL + vector engine**: vector representations
(chunks, embeddings, vector search) are **derived from your database data** and
**incrementally maintained** as that data changes.

> *"I am trying to eliminate the boundary between the database and its AI representation."*

### 🏗 How it works

```mermaid
flowchart TD
    App[Application] --> Q[SQL / AI Query]
    Q --> P[Query Parser]
    P --> PL[Query Planner]
    PL --> R{contains SIMILARITY<br/>or VECTOR_SEARCH?}
    R -- yes --> VE[Vector / Semantic Engine]
    R -- no --> SE[Structured SQL Engine]
    SE --> T[(TuckDB .tuck file)]
    VE --> VS[(Vector DB .tkdb file)]
    VE -. vectors derived from the same table rows .-> T
    T --> LM[Lifecycle Manager]
    VS --> LM
    LM --> ADD[INSERT -> index only new data]
    LM --> UPD[UPDATE -> re-embed only changed chunks]
    LM --> DEL[DELETE -> clean only affected vectors]
```

### ✨ New capabilities

| | Feature | What it means for you |
|-|---------|----------------------|
| 🔎 | **Semantic vector search** | Ask questions in natural language; top-K cosine results with scores |
| 🧠 | **`SIMILARITY(...)` / `VECTOR_SEARCH(...)` SQL** | Semantic operators inside `WHERE`; the query itself decides the route |
| 🔁 | **Incremental update** | A chunk change re-embeds **only that chunk** — 500k chunks, 1k changed → **99.8% work avoided** |
| 🗑️ | **Incremental delete** | Removing a document cleans only its vectors — no full vector scan |
| 🧬 | **Exact + semantic dedup** | Byte-identical hashes (FNV-1a) and embedding-based paraphrase candidates |
| 🧭 | **Model versioning** | Every vector knows its model; migrate v1→v2 without touching unchanged data |
| 💾 | **Persistent vector DB** | Whole engine state (lifecycle + chunks + vectors) saved to one `.tkdb` file |
| 🔀 | **Query-driven routing** | `SIMILARITY`/`VECTOR_SEARCH` → vector DB; otherwise → `.tuck` file |

### 🖼️ Demo walkthrough: upload → chunk → store → query

Index any text file (`.txt`, `.md`, `.pdb`, …) into the vector DB, then query it —
**adding or deleting files later never re-processes the existing data**:

```mermaid
sequenceDiagram
    participant U as User
    participant D as file_index_demo
    participant E as IncrementalEngine
    participant S as Vector DB (.tkdb)

    U->>D: add data/demo_cpp.txt
    D->>E: ingest(file)
    E->>E: chunk text (paragraphs)
    E->>E: embed each chunk
    E->>S: store vectors (11)
    D->>S: save_to vector_db.tkdb

    U->>D: q "How do I manage ownership?"
    D->>E: embed query
    E->>S: search (cosine top-K)
    S-->>D: ranked chunks + scores
    D-->>U: #1 "std::unique_ptr is an exclusive owner…"
```

```bash
# 1. Upload a real file and index it
cargo run --example file_index_demo -- --file data/demo_cpp.txt

# 2. NEW session: nothing is re-processed
#    [load] restored 1 docs / 11 vectors — nothing re-processed

# 3. Add another file — only the new file is processed
#    > add README.md
#    [add] doc 1 <- README.md: 330 new chunks (11 -> 341 vectors; existing untouched)

# 4. Search
#    > q What is the tuck file format?
#    #1 score=0.591 [README.md chunk 199] "### Summary: What's in the .tuck file…"

# 5. Delete — only that file's vectors are removed
#    > del 0
#    [del] doc 0: removed 11 vectors directly (341 -> 330); unrelated untouched
```

> 📄 **Full end-to-end transcript (real, unedited output):** see
> **[docs/E2E_DEMO.md](docs/E2E_DEMO.md)** — reproduce it with `./scripts/e2e_demo.sh`.

### ⚡ Headline numbers

```
Dataset:                   500,000 chunks
Changed:                     1,000
Processed (incremental):     1,000
Skipped:                   499,000

Work avoided:                99.8%
```

Reproduce it yourself:

```bash
cargo run --release --example benchmark_report   # scaling table → target/benchmark_report.md
cargo run --release --example showcase           # the full story in one run
```

### 🧭 Full demo list

```bash
cargo run --example lifecycle_demo          # versions, content hashes, stale state
cargo run --example dedup_demo              # exact duplicate detection by content hash
cargo run --example chunk_demo              # only the changed chunk is reprocessed
cargo run --example embedding_demo          # mock embedding provider + vector store
cargo run --example vector_search_demo      # semantic top-K over stored vectors
cargo run --example sql_vector_demo         # SIMILARITY(...) in SQL
cargo run --example hybrid_demo             # topic = 'memory' AND SIMILARITY(...) > 0.3
cargo run --example incremental_demo        # 500k chunks / 1k changed → 99.8% avoided
cargo run --example delete_demo             # delete cleans only affected vectors
cargo run --example semantic_dedup_demo     # paraphrase candidates (report only)
cargo run --example model_migration_demo    # v1 → v2 incremental migration
cargo run --example file_index_demo         # real files → chunks → vector DB → queries
cargo run --example unified_demo            # SQL routing: vector DB vs .tuck
```

See **[docs/VECTOR_DB.md](docs/VECTOR_DB.md)** for the deep dive (flow diagrams,
routing, persistence format, benchmark table) and **[docs/PROGRESS.md](docs/PROGRESS.md)**
for the phase-by-phase evidence.

---

## ✨ Features at a glance

| | Feature | What it means for you |
|-|---------|----------------------|
| 📦 | **All-in-one library** | Storage + compression + query + cache. No stitching together S3 + Parquet + DuckDB + Redis |
| ⚡ | **Compressed-first execution** | Queries skip irrelevant chunks using stats — zero decompression for MIN/MAX/COUNT |
| 🧩 | **6 pluggable encodings** | Delta, XOR/Gorilla, Dict+ZSTD, RLE, Bitmap — pick the best for your data |
| 🔌 | **Embeddable** | No server process. Link as a library. Rust API + C API for Python/C++/Go |
| 💾 | **Self-describing blobs** | `.tuck` files include schema + stats — no external catalog or metastore |
| 🗄️ | **S3 + local backends** | Read/write compressed blobs to local disk or S3-compatible storage |
| 🚀 | **Caching built-in** | LRU data cache for chunks + result cache for repeated queries |
| 🔧 | **CLI + SQL** | Load CSVs, run SQL SELECT with WHERE/GROUP BY/aggregation |
| 🤖 | **AI / vector engine** | Vector representations derived from your data, maintained incrementally |
| 🔎 | **Semantic search** | Natural-language queries via SQL `SIMILARITY(...)` / `VECTOR_SEARCH(...)` |
| 🔁 | **Incremental lifecycle** | Insert/update/delete re-process only the affected chunks — up to 99.8% work avoided |

---

## 🎯 Who is this for?

**You need TuckDB if:**
- ✅ You want to embed analytics in your **app, microservice, or edge device** without running a database server
- ✅ You're tired of **stitching together** Parquet + DuckDB + Redis + S3 SDK for every project
- ✅ Your data has **repeated values, time-series patterns, or low-cardinality strings** — and you want real compression
- ✅ You want a **single-file portable format** that carries its own schema and statistics
- ✅ You work in **Rust** and want a native analytics library (or use C API from other languages)

**You might want something else if:**
- ❌ You need full SQL-92 (JOINs, subqueries, window functions) — DuckDB is better
- ❌ Your workload is OLTP (row updates, transactions, indexes) — SQLite fits
- ❌ You need a distributed query engine across 100+ nodes — use Spark/Trino

---

## 📥 Installation

### Prerequisites

- **Rust** (1.85+): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **C compiler** (zstd dependency): `build-essential` (Linux), `xcode-select --install` (macOS), or MSVC Build Tools (Windows)

### Linux (Ubuntu/Debian)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install build dependencies
sudo apt update && sudo apt install build-essential pkg-config libssl-dev

# Clone and build
git clone https://github.com/swadhingoswami/tuckdb.git
cd tuckdb
cargo build --release

# Run CLI
cargo run --example cli -- help
```

### Linux (Fedora/RHEL)

```bash
sudo dnf install gcc pkg-config openssl-devel
git clone https://github.com/swadhingoswami/tuckdb.git
cd tuckdb && cargo build --release
```

### macOS

```bash
# Install Rust + Xcode Command Line Tools
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/swadhingoswami/tuckdb.git
cd tuckdb
cargo build --release
```

### Windows

```powershell
# 1. Install Rust from https://rustup.rs
# 2. Install Visual Studio Build Tools (or VS 2022) with "Desktop development with C++"
# 3. Clone and build
git clone https://github.com/swadhingoswami/tuckdb.git
cd tuckdb
cargo build --release
```

### Add to your Rust project

```toml
[dependencies]
tuckdb = "0.1"

# Optional features:
# tuckdb = { version = "0.1", features = ["s3"] }    # S3 backend
# tuckdb = { version = "0.1", features = ["capi"] }  # C API
```

### Build with different features

```bash
# Default (minimal): only zstd
cargo build --release

# With S3 backend
cargo build --release --features s3

# With C API (produces shared library)
cargo build --release --features capi

# All features
cargo build --release --features s3,capi

# Cross-compile for ARM (e.g., Raspberry Pi)
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
```

### Run after install

```bash
# CLI tool
cargo run --example cli -- info mytable --dir /tmp/db

# Demo
cargo run --release --example demo_app

# All examples
cargo run --release --example basic_operations
cargo run --release --example time_series
```

---

## 🔌 Use from any language (C API)

TuckDB compiles to a **C shared library** that any language can call — C, C++, Python, Java, C#, Go, Swift, Ruby, Zig, and more.

### Build the shared library

```bash
cargo build --release --features capi
```

Output files:
| Platform | Library | Header |
|----------|---------|--------|
| **Linux** | `target/release/libtuckdb.so` | `capi/tuckdb.h` |
| **macOS** | `target/release/libtuckdb.dylib` | `capi/tuckdb.h` |
| **Windows** | `target/release/tuckdb.dll` + `tuckdb.lib` | `capi/tuckdb.h` |

### C / C++

```c
#include "tuckdb.h"
#include <stdio.h>

int main() {
    tuckdb_table_t* table = tuckdb_create(
        "sensors",
        "[{\"name\":\"temp\",\"type\":\"Float64\"},{\"name\":\"humidity\",\"type\":\"Int64\"}]",
        "/tmp/db"
    );
    if (!table) { printf("Error: %s\n", tuckdb_error_message(0)); return 1; }

    // Insert data (temperature columns)
    double temps[] = {22.5, 23.0, 21.8};
    int64_t hums[] = {65, 70, 60};
    tuckdb_insert(table, 2, NULL, "", temps, "temp", NULL, "");

    // Query
    tuckdb_result_t* res = tuckdb_query(table, "SELECT * FROM sensors WHERE temp > 22");
    for (int r = 0; r < tuckdb_result_num_rows(res); r++) {
        double t = tuckdb_result_value_double(res, r, 0);
        printf("temp = %.1f\n", t);
    }

    tuckdb_result_free(res);
    tuckdb_table_free(table);
    return 0;
}
```

```bash
# Compile and link
gcc -o myapp myapp.c -I./capi -L./target/release -ltuckdb
./myapp
```

### Python

```python
import ctypes
import os

lib = ctypes.cdll.LoadLibrary("./target/release/libtuckdb.dylib")

# Define function signatures
lib.tuckdb_create.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p]
lib.tuckdb_create.restype = ctypes.c_void_p

table = lib.tuckdb_create(
    b"sensors",
    b'[{"name":"temp","type":"Float64"},{"name":"humidity","type":"Int64"}]',
    b"/tmp/db"
)

# Query
lib.tuckdb_query.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
lib.tuckdb_query.restype = ctypes.c_void_p

res = lib.tuckdb_query(table, b"SELECT AVG(temp) FROM sensors")
rows = lib.tuckdb_result_num_rows(res)
cols = lib.tuckdb_result_num_cols(res)
print(f"{rows} rows, {cols} cols")
```

### Java (JNI)

```java
public class TuckDB {
    static { System.loadLibrary("tuckdb"); }

    // Native method declarations matching the C API
    public static native long tuckdb_create(String name, String schema, String path);
    public static native long tuckdb_query(long table, String sql);
    // ... wrap with JNI generator or hand-written native methods
}
```

### C# / .NET (P/Invoke)

```csharp
using System.Runtime.InteropServices;

class TuckDB {
    [DllImport("tuckdb")] static extern IntPtr tuckdb_create(string name, string schema, string path);
    [DllImport("tuckdb")] static extern IntPtr tuckdb_query(IntPtr table, string sql);
    // ...
}
```

### Go (cgo)

```go
/*
#cgo LDFLAGS: -L./target/release -ltuckdb
#include "tuckdb.h"
*/
import "C"

func main() {
    table := C.tuckdb_create(
        C.CString("sensors"),
        C.CString(`[{"name":"temp","type":"Float64"}]`),
        C.CString("/tmp/db"),
    )
    result := C.tuckdb_query(table, C.CString("SELECT * FROM sensors"))
    // ...
}
```

### Full C API reference

All functions are documented in [`capi/tuckdb.h`](capi/tuckdb.h):

```c
// Lifecycle
tuckdb_table_t* tuckdb_create(const char* name, const char* schema_json, const char* path);
tuckdb_table_t* tuckdb_open(const char* name, const char* path);
void            tuckdb_table_free(tuckdb_table_t* table);

// Insert
int tuckdb_insert(tuckdb_table_t* table, int num_columns,
                  const int64_t* int_cols, const char* int_col_names,
                  const double* float_cols, const char* float_col_names,
                  const char* const* str_cols, const char* str_col_names);

// Query
tuckdb_result_t* tuckdb_query(tuckdb_table_t* table, const char* sql);
int             tuckdb_result_num_rows(tuckdb_result_t* result);
int             tuckdb_result_num_cols(tuckdb_result_t* result);
const char*     tuckdb_result_column_name(tuckdb_result_t* result, int col);
double          tuckdb_result_value_double(tuckdb_result_t* result, int row, int col);
const char*     tuckdb_result_value_string(tuckdb_result_t* result, int row, int col);
int64_t         tuckdb_result_value_int(tuckdb_result_t* result, int row, int col);
void            tuckdb_result_free(tuckdb_result_t* result);

// Error handling
const char* tuckdb_error_message(int code);
```

---

## 🤔 Why another analytics library?

> *"Today, getting analytics means: store data in S3 → convert to Parquet → run DuckDB → cache with Redis. That's 4 systems talking to each other. None of them co-designed."*

**TuckDB replaces that entire stack** with one library where compression, storage, queries, and caching are built together — not bolted on.

```mermaid
flowchart LR
    subgraph OLD["❌ Traditional stack (4+ systems)"]
        S3 --> P["Parquet/ORC"] --> Q["DuckDB/Spark"] --> C["Redis"]
    end

    subgraph NEW["✅ TuckDB (one library)"]
        T["📦 tuckdb"]
    end

    APP["Application"] --> OLD
    APP2["Application"] --> NEW

    style OLD fill:#ffe0e0
    style NEW fill:#e0ffe0
```

**Result:**
- **5-20x compression** on strings vs row-based storage
- **Chunk-level filter skipping** — reads only the data it needs
- **Zero server processes** — embedded in your application
- **One file format** — schema, stats, and data travel together

---

## 🏗 Architecture 
```mermaid
flowchart TB
    APP["Application"] --> API
    subgraph TUCKDB["📦 TuckDB"]
        API["API: Table / Query Builder"]
        QENG["Query Engine<br/>LogicalPlan → Optimizer → Physical Ops"]
        STORE["Storage: .tuck blobs + 6 encodings + stats"]
        CACHE["Cache: DataCache + ResultCache"]
        BACKEND["Backend: Local FS / S3"]
        API --> QENG --> STORE --> CACHE --> BACKEND
    end
```

### Query Flow

```
SQL "SELECT AVG(temp) WHERE temp > 100 GROUP BY city"
       │
       ▼
LogicalPlan: Scan → Filter → Project → Aggregate
       │
       ▼
Optimizer pushes filter & projection into Scan
       │
       ▼
FileScan iterates compressed chunks:
  ┌──────────────────────────────────────────────┐
  │  Chunk 1: min=50, max=80 → 🚫 SKIP           │
  │  Chunk 2: min=90, max=150 → 🔍 DECODE         │
  │  Chunk 3: min=120, max=200 → ✅ PASS ALL      │
  └──────────────────────────────────────────────┘
       │
       ▼
Aggregate: hash GROUP BY → SUM/COUNT/AVG → result
```

---

## 💡 Under the Hood — Complete Walkthrough with emp Table

This section walks through the entire lifecycle of TuckDB using a concrete `emp` table example. Every state, every piece of metadata, every step explained.

---

### Step 1: Create table — No .tuck file yet

You call:
```rust
let schema = Schema::new(vec![
    Field::new("empid",   DataType::Int64, false),
    Field::new("empname", DataType::Utf8,  false),
    Field::new("empaddr", DataType::Utf8,  false),
    Field::new("empsal",  DataType::Int64, true),
]);
let mut table = Table::create("emp", schema, "/tmp/db");
```

**Memory state:**
```
Table "emp"
├── schema: [empid:Int64, empname:Utf8, empaddr:Utf8, empsal:Int64]
├── batches: []                  ← empty, no data yet
├── data_version: 0              ← no changes yet
├── result_cache: {}             ← empty
└── .tuck file: DOES NOT EXIST   ← nothing on disk
```

---

### Step 2: Insert batch 1 (3 employees) — All in memory, .tuck untouched

```rust
let batch1 = RecordBatch::new(schema, vec![
    Column::new(f_empid,   ColumnData::Int64(  vec![101, 102, 103])),
    Column::new(f_empname, ColumnData::Utf8(   vec!["Alice", "Bob", "Charlie"])),
    Column::new(f_empaddr, ColumnData::Utf8(   vec!["Mumbai", "Delhi", "Bangalore"])),
    Column::new(f_empsal,  ColumnData::Int64(  vec![25000, 18000, 45000])),
]);
table.insert_batch(batch1);
```

**Memory state after insert:**
```
Table "emp"
├── schema: [empid:Int64, empname:Utf8, empaddr:Utf8, empsal:Int64]
├── batches: [
│     batch 0 (chunk 0): RecordBatch {
│       num_rows: 3,
│       columns: [
│         empid:   Int64(  [101, 102, 103] ),     ← Vec<i64> raw
│         empname: Utf8(   ["Alice","Bob","Charlie"] ), ← Vec<String> raw
│         empaddr: Utf8(   ["Mumbai","Delhi","Bangalore"] ), ← Vec<String> raw
│         empsal:  Int64(  [25000, 18000, 45000] ), ← Vec<i64> raw
│       ]
│     }
│   ]
├── data_version: 1              ← incremented from 0
├── result_cache: {}             ← cleared (was already empty)
└── .tuck file: DOES NOT EXIST   ← still no file on disk
```

**Key points:**
- Data is stored column-wise: all empids together (`Vec<i64>`), all empnames together (`Vec<String>`)
- Each column type has its own Vec type — no row objects, no boxing
- **Zero compression, zero disk I/O** — just appends to an in-memory vector
- `data_version` went from 0 → 1 (tracks data freshness)
- The `.tuck` file on disk doesn't exist yet

---

### Step 3: Insert batch 2 (2 more employees) — Second chunk in memory

```rust
let batch2 = RecordBatch::new(schema, vec![
    Column::new(f_empid,   ColumnData::Int64(  vec![104, 105])),
    Column::new(f_empname, ColumnData::Utf8(   vec!["Diana", "Eve"])),
    Column::new(f_empaddr, ColumnData::Utf8(   vec!["Pune", "Mumbai"])),
    Column::new(f_empsal,  ColumnData::Int64(  vec![55000, 32000])),
]);
table.insert_batch(batch2);
```

**Memory state after second insert:**
```
Table "emp"
├── batches: [
│     batch 0 (chunk 0): { num_rows:3, empid:[101,102,103], empname:["Alice","Bob","Charlie"],
│                           empaddr:["Mumbai","Delhi","Bangalore"], empsal:[25000,18000,45000] },
│     batch 1 (chunk 1): { num_rows:2, empid:[104,105], empname:["Diana","Eve"],
│                           empaddr:["Pune","Mumbai"], empsal:[55000,32000] },
│   ]
├── data_version: 2              ← incremented again
├── result_cache: {}             ← cleared again
└── .tuck file: DOES NOT EXIST
```

**Each batch is a separate chunk. No merging, no sorting, no rebalancing.**

---

### Step 4: Query before flush — Reads from memory

```rust
let plan = LogicalPlan::scan("emp")
    .filter(col("empsal").gt(lit_int(30000)))
    .project(&["empname", "empsal"]);
let result = table.execute(plan);
```

**What happens inside:**

1. **Optimizer** rewrites plan: pushes `empsal > 30000` and `projection [empname, empsal]` into the Scan node.

2. **Chunk skipping on in-memory data**: For each batch, compute stats on the fly by scanning the raw Vec:

   ```
   Batch 0: empsal = [25000, 18000, 45000]
     → min=18000, max=45000  (computed by scanning 3 values)
     → "Can empsal > 30000 match in range [18000, 45000]?"
     → max(45000) > 30000? YES → keep batch, decode (no-op, already raw)

   Batch 1: empsal = [55000, 32000]
     → min=32000, max=55000  (computed by scanning 2 values)
     → "Can empsal > 30000 match in range [32000, 55000]?"
     → max(55000) > 30000? YES → keep batch, decode
   ```

3. **Row-level filter** applied to each batch:
   ```
   Batch 0: mask = [25000>30000? No, 18000>30000? No, 45000>30000? Yes]
            → filtered: empname=["Charlie"], empsal=[45000]

   Batch 1: mask = [55000>30000? Yes, 32000>30000? Yes]
            → filtered: empname=["Diana","Eve"], empsal=[55000,32000]
   ```

4. **Projection**: Keep only empname and empsal columns.

5. **Result**: Charlie(45000), Diana(55000), Eve(32000)

**No .tuck file is read. Everything comes from the in-memory Vecs.**

---

### Step 5: Flush — Compress everything, write .tuck file

```rust
table.flush();
```

This is where the real work happens. Let's trace every internal step.

#### Step 5a: Encode each column of each chunk

For every (batch, column), pick the best encoder and compress:

```
Batch 0, empid:   [101, 102, 103]    → Delta+Varint → [101, +1, +1] → 3 bytes
Batch 0, empname: ["Alice","Bob","Charlie"] → Dict+ZSTD → ~45 bytes
Batch 0, empaddr: ["Mumbai","Delhi","Bangalore"] → Dict+ZSTD → ~50 bytes
Batch 0, empsal:  [25000, 18000, 45000] → Delta+Varint → [25000, -7000, +27000] → 6 bytes

Batch 1, empid:   [104, 105]         → Delta+Varint → [104, +1] → 2 bytes
Batch 1, empname: ["Diana","Eve"]    → Dict+ZSTD → ~35 bytes
Batch 1, empaddr: ["Pune","Mumbai"]  → Dict+ZSTD → ~38 bytes
Batch 1, empsal:  [55000, 32000]     → Delta+Varint → [55000, -23000] → 4 bytes
```

#### Step 5b: Compute column stats for each chunk

For each compressed column, compute the min, max, null_count:

```
Batch 0:
  empid:   min=101,  max=103,  nulls=0
  empname: min=None, max=None, nulls=0   ← Utf8 has NO string stats
  empaddr: min=None, max=None, nulls=0   ← Utf8 has NO string stats
  empsal:  min=18000, max=45000, nulls=0

Batch 1:
  empid:   min=104,  max=105,  nulls=0
  empname: min=None, max=None, nulls=0
  empaddr: min=None, max=None, nulls=0
  empsal:  min=32000, max=55000, nulls=0
```

#### Step 5c: Build ChunkMeta records

For each (batch_idx, col_idx), create one ChunkMeta with encoding, sizes, offset, stats:

```
Total compressed data size = 3 + 45 + 50 + 6 + 2 + 35 + 38 + 4 = 183 bytes
Header size (schema + all ChunkMeta) ≈ 44 + (8 × 38) ≈ 348 bytes
Data starts at offset 348

ChunkMeta[0]: col=0(empid),   chunk=0, enc=1(Delta),   offset=348, comp=3,  uncompr=3, min=101,   max=103
ChunkMeta[1]: col=1(empname), chunk=0, enc=3(DictZstd), offset=351, comp=45, uncompr=3, min=None,  max=None
ChunkMeta[2]: col=2(empaddr), chunk=0, enc=3(DictZstd), offset=396, comp=50, uncompr=3, min=None,  max=None
ChunkMeta[3]: col=3(empsal),  chunk=0, enc=1(Delta),   offset=446, comp=6,  uncompr=3, min=18000, max=45000
ChunkMeta[4]: col=0(empid),   chunk=1, enc=1(Delta),   offset=452, comp=2,  uncompr=2, min=104,   max=105
ChunkMeta[5]: col=1(empname), chunk=1, enc=3(DictZstd), offset=454, comp=35, uncompr=2, min=None,  max=None
ChunkMeta[6]: col=2(empaddr), chunk=1, enc=3(DictZstd), offset=489, comp=38, uncompr=2, min=None,  max=None
ChunkMeta[7]: col=3(empsal),  chunk=1, enc=1(Delta),   offset=527, comp=4,  uncompr=2, min=32000, max=55000
```

**The offset is calculated**: offset = header_size + sum of all previous chunks' compressed sizes. This lets the reader jump directly to any chunk by seeking to `meta.offset`.

#### Step 5d: Write the .tuck file

The complete binary file on disk:

```
emp.tuck  (approx 531 bytes)
┌────────────────────────────────────────────────────────────────────┐
│ 0-3:   Magic "TCKB"                                                │
│ 4-7:   Version = 1                                                 │
│ 8-11:  Schema length = 44                                          │
│ 12-55: Schema bytes → [empid:Int64, empname:Utf8, empaddr:Utf8,   │
│                         empsal:Int64]                               │
│ 56-59: Number of chunk metas = 8                                   │
│ 60-97: ChunkMeta[0] → empid chunk0, offset=348, comp=3,  min=101  │
│ 98-135: ChunkMeta[1] → empname chunk0, offset=351, comp=45        │
│ 136-173: ChunkMeta[2] → empaddr chunk0, offset=396, comp=50       │
│ 174-211: ChunkMeta[3] → empsal chunk0, offset=446, comp=6,  max=45000 │
│ 212-249: ChunkMeta[4] → empid chunk1, offset=452, comp=2,  min=104   │
│ 250-287: ChunkMeta[5] → empname chunk1, offset=454, comp=35       │
│ 288-325: ChunkMeta[6] → empaddr chunk1, offset=489, comp=38       │
│ 326-363: ChunkMeta[7] → empsal chunk1, offset=527, comp=4,  max=55000 │
│ 364-366: Chunk 0 empid compressed [3 bytes]                        │
│ 367-411: Chunk 0 empname compressed [45 bytes]                     │
│ 412-461: Chunk 0 empaddr compressed [50 bytes]                     │
│ 462-467: Chunk 0 empsal compressed [6 bytes]                       │
│ 468-469: Chunk 1 empid compressed [2 bytes]                        │
│ 470-504: Chunk 1 empname compressed [35 bytes]                     │
│ 505-542: Chunk 1 empaddr compressed [38 bytes]                     │
│ 543-546: Chunk 1 empsal compressed [4 bytes]                       │
└────────────────────────────────────────────────────────────────────┘
```

**ChunkMeta stores offset values that point into the data section.** For example, ChunkMeta[0] says: "empid data for chunk 0 starts at byte 348, it's 3 bytes long." At byte 348 you find the compressed Delta+Varint encoding of [101, 102, 103].

#### Step 5e: Update in-memory state after flush

```
Table "emp"
├── batches: [batch0, batch1]     ← still exists unchanged in memory
├── persisted: true               ← new flag set
├── blob_bytes: Some(531 bytes)   ← entire .tuck file kept in memory too
├── data_version: 2               ← unchanged (no new insert)
└── .tuck file: EXISTS at /tmp/db/emp.tuck (531 bytes)
```

**Data is now duplicated** — the raw `Vec`s in memory AND the compressed bytes in `blob_bytes`. This is intentional: queries before the next insert can use either source.

---

### Step 6: Query after flush — Chunk skipping with stored metadata

```rust
let plan = LogicalPlan::scan("emp")
    .filter(col("empsal").gt(lit_int(30000)))
    .project(&["empname", "empsal"]);
let result = table.execute(plan);
```

This time the engine sees `persisted=true` and uses **FileScan** instead of in-memory scan:

1. **Parse header** — Read the first ~363 bytes of `blob_bytes`, extract schema and all 8 ChunkMeta records.

2. **Chunk skipping using stored min/max** (not computed on the fly):

   ```
   ChunkMeta[3]: empsal, chunk=0, min=18000, max=45000
     → "Can empsal > 30000 match in [18000, 45000]?"
     → max(45000) > 30000? YES → read compressed data at offset 446 (6 bytes)
     → Delta+Varint decode → [25000, 18000, 45000]
     → Row filter: 25000>30000? No, 18000>30000? No, 45000>30000? Yes
     → Keep: empname="Charlie", empsal=45000

   ChunkMeta[7]: empsal, chunk=1, min=32000, max=55000
     → "Can empsal > 30000 match in [32000, 55000]?"
     → max(55000) > 30000? YES → read compressed data at offset 527 (4 bytes)
     → Delta+Varint decode → [55000, 32000]
     → Row filter: 55000>30000? Yes, 32000>30000? Yes
     → Keep: empname="Diana", empsal=55000 / empname="Eve", empsal=32000
   ```

3. **Project** — Keep only empname and empsal.

4. **DataCache** — Decoded chunks are inserted into the LRU cache for future queries.

**Same result as before flush, but now decompression happened. The benefit of chunk skipping shows when chunks CAN be skipped.**

---

### Step 7: Query that BENEFITS from chunk skipping

```rust
let plan = LogicalPlan::scan("emp")
    .filter(col("empsal").gt(lit_int(50000)))
    .project(&["empname", "empsal"]);
let result = table.execute(plan);
```

```
ChunkMeta[3]: empsal, chunk=0, min=18000, max=45000
  → "Can empsal > 50000 match in [18000, 45000]?"
  → max(45000) > 50000? NO → SKIP 🚫
  → Zero bytes read from data section. Zero decompression.

ChunkMeta[7]: empsal, chunk=1, min=32000, max=55000
  → "Can empsal > 50000 match in [32000, 55000]?"
  → max(55000) > 50000? YES → read offset 527 (4 bytes), decode → [55000, 32000]
  → Row filter: 55000>50000? Yes, 32000>50000? No
  → Keep: empname="Diana", empsal=55000
```

**Chunk 0 was skipped entirely by reading just 38 bytes of metadata. No decompression of empid, empname, empaddr, or empsal for that chunk.**

---

### Step 8: Insert after flush — New data in memory, .tuck now stale

```rust
let batch3 = RecordBatch::new(schema, vec![
    Column::new(f_empid,   ColumnData::Int64(  vec![106])),
    Column::new(f_empname, ColumnData::Utf8(   vec!["Frank"])),
    Column::new(f_empaddr, ColumnData::Utf8(   vec!["Chennai"])),
    Column::new(f_empsal,  ColumnData::Int64(  vec![15000])),
]);
table.insert_batch(batch3);
```

**Memory state:**
```
Table "emp"
├── batches: [
│     batch 0 (chunk 0): 3 rows (Alice, Bob, Charlie)   ← from before flush
│     batch 1 (chunk 1): 2 rows (Diana, Eve)             ← from before flush
│     batch 2 (chunk 2): 1 row  (Frank)                  ← NEW, only in memory
│   ]
├── data_version: 3              ← incremented
├── persisted: true              ← still true, but .tuck is now STALE
├── blob_bytes: Some(...)        ← old .tuck (only chunks 0 & 1)
└── .tuck file: emp.tuck         ← still has only chunks 0 & 1. Frank is NOT in it.
```

**The .tuck file on disk is now outdated.** It has 5 rows (chunks 0+1), but the table has 6 rows (chunks 0+1+2). Frank only exists in memory.

**Current limitation**: The query engine reads from the .tuck file (chunks 0+1) and ignores the in-memory batch 2 (Frank). Calling `flush()` again rewrites the complete .tuck file with all 3 chunks.

---

### Step 9: Flush again — Rewrite complete .tuck

```rust
table.flush();
```

1. All 3 batches (old chunks 0+1 + new chunk 2) are compressed together
2. New ChunkMeta records are built for all 3 × 4 = 12 column-chunks
3. Complete .tuck file is rewritten — now 6 rows, 3 chunks

```
emp.tuck (rewritten)
├── Schema: [empid, empname, empaddr, empsal]
├── ChunkMeta[0..3]: chunk 0 metadata (unchanged min/max)
├── ChunkMeta[4..7]: chunk 1 metadata (unchanged min/max)
├── ChunkMeta[8..11]: chunk 2 metadata → empsal min=15000, max=15000
└── Compressed data: chunk 0 cols + chunk 1 cols + chunk 2 cols
```

**Now the file has all 6 rows. The in-memory and on-disk state are in sync again.**

---

### Summary: What's in the .tuck file at each stage

| State | .tuck file exists? | Contains | Metadata in .tuck |
|-------|-------------------|----------|-------------------|
| After `create()` | No | — | — |
| After `insert()` | No | — | — |
| After `flush()` | **Yes** | All inserted chunks, compressed | Schema + ChunkMeta[] with min/max/offsets |
| After more `insert()` | Yes (but stale) | Old chunks only | Stale metadata (missing new chunks) |
| After 2nd `flush()` | **Yes** (rewritten) | All chunks old+new | Updated ChunkMeta[] with new chunks |

**Key design takeaways:**
- **No incremental writes.** The entire .tuck file is written from scratch on every flush.
- **No separate metadata file.** Schema, ChunkMeta, and compressed data live in one binary.
- **Metadata grows linearly** with (chunks × columns). Each ChunkMeta is ~38 bytes.
- **Chunk skipping** works on numeric columns (Int64, Float64, Timestamp) using stored min/max. String columns (Utf8) store `min=None, max=None` and cannot skip chunks.
- **Data is duplicated** between memory and .tuck file after flush until next insert.

---

## ⚡ Performance

| Encoding | Type | Throughput (encode) | Ratio |
|----------|------|-------------------|-------|
| Delta + varint | Sequential ints | **13.2 GB/s** | 8.0x |
| XOR / Gorilla | Time-series floats | **8.1 GB/s** | 2.8x |
| Dictionary + ZSTD | Low-card strings | 0.3 GB/s | **12.5x** |
| Run-length (RLE) | Repeated ints | **18.2 GB/s** | 8.0x |
| Bitmap | 0/1 booleans | **32.4 GB/s** | **32.0x** |

| Query (100K rows) | Cold cache | Hot cache |
|-------------------|-----------|-----------|
| Full scan | 2.1 ms | **0.3 ms** |
| Filter (10%) | 0.8 ms | **0.1 ms** |
| GROUP BY aggregation | 3.0 ms | **0.4 ms** |

*Benchmarked on Apple M3. Run `cargo bench` to reproduce.*

---

## 📦 What's inside

```text
src/
├── api/              # Table, ResultSet (user-facing API)
├── schema/           # DataType, Field, Schema
├── exec/             # Query engine (LogicalPlan, Optimizer, Physical ops)
├── storage/          # Blob format (.tuck) + 6 encodings
│   └── encoding/     # Delta, XOR, Dict+ZSTD, RLE, Bitmap, Timestamp
├── lifecycle/        # TuckDB-AI: versions, content hashes, stale state, chunk diff, exact dedup
├── embedding/        # TuckDB-AI: EmbeddingProvider, vector store, cosine search, semantic dedup
├── incremental/      # TuckDB-AI: IncrementalEngine — ingest/update/delete/migrate + persistence
├── cache/            # DataCache (LRU/LFU) + ResultCache
├── backend/          # Local FS, S3, In-Memory
└── capi/             # C FFI bindings

examples/
├── cli/              # CLI tool: load CSV, SQL queries, scan, info
├── demo_app/         # 100K rows end-to-end demo
├── basic_operations/ # 10-step CRUD walkthrough
├── time_series/      # 3-server analytics example
├── crud_workflow/    # Product inventory cycle
├── file_index_demo/  # TuckDB-AI: real files → chunks → vector DB → queries
├── unified_demo/     # TuckDB-AI: SQL routing — vector DB vs .tuck
├── showcase/         # TuckDB-AI: the full story in one run
└── benchmark_report/ # TuckDB-AI: scaling benchmark (work avoided, speedup)

benches/              # Criterion benchmarks
capi/tuckdb.h         # C header
data/                 # TuckDB-AI: sample files (cpp_guide.txt, demo_cpp.txt, cpp_questions.csv)
```

---

## 🛠 Quick start (detailed)

### Add to your project

```toml
[dependencies]
tuckdb = "0.1"
```

### Full Rust example

```rust
use std::path::PathBuf;
use tuckdb::*;

fn main() {
    let dir = PathBuf::from("/tmp/tuckdb_demo");
    std::fs::create_dir_all(&dir).unwrap();

    // 1. Define schema
    let schema = Schema::new(vec![
        Field::new("ts", DataType::Timestamp, false),
        Field::new("sensor", DataType::Utf8, false),
        Field::new("temp", DataType::Float64, true),
        Field::new("humidity", DataType::Int64, true),
    ]);

    let mut table = Table::create("sensors", schema.clone(), dir);

    // 2. Ingest data (10 batches × 1000 rows = 10K rows)
    for batch_id in 0..10 {
        let n = 1000usize;
        let ts: Vec<i64> = (0..n).map(|i|
            1700000000i64 + (batch_id * n + i) as i64).collect();
        let sensors = vec!["temp-sensor-01"; n].into_iter()
            .map(String::from).collect();
        let temps: Vec<f64> = (0..n).map(|i|
            20.0 + (i as f64 * 0.1).sin() * 5.0).collect();
        let hums: Vec<i64> = (0..n).map(|i| (60 + (i % 20)) as i64).collect();

        table.insert_batch(RecordBatch::new(schema.clone(), vec![
            Column::new(schema.fields[0].clone(), ColumnData::Timestamp(ts)),
            Column::new(schema.fields[1].clone(), ColumnData::Utf8(sensors)),
            Column::new(schema.fields[2].clone(), ColumnData::Float64(temps)),
            Column::new(schema.fields[3].clone(), ColumnData::Int64(hums)),
        ]));
    }
    println!("Ingested {} rows", table.num_rows());

    // 3. Query
    let plan = LogicalPlan::scan("sensors")
        .filter(col("humidity").gt(lit_int(65)))
        .project(&["sensor", "temp"])
        .aggregate(
            vec![
                (AggOp::Avg, "temp", "avg_temp"),
                (AggOp::Min, "temp", "min_temp"),
                (AggOp::Max, "temp", "max_temp"),
            ],
            vec!["sensor".to_string()],
        );

    let mut rs = table.execute(plan);
    while let Some(batch) = rs.next_batch() {
        for row in 0..batch.num_rows {
            println!("{}: avg={:.1}°C",
                match &batch.columns[0].data {
                    ColumnData::Utf8(v) => &v[row], _ => ""
                },
                match &batch.columns[1].data {
                    ColumnData::Float64(v) => v[row], _ => 0.0
                }
            );
        }
    }

    // 4. Persist to disk
    table.flush();

    // 5. Reload later
    let reloaded = Table::open("sensors", dir);
    println!("Reloaded {} rows from disk", reloaded.num_rows());
}
```

### CLI tool

```bash
# Load CSV → auto-detect schema
cargo run --example cli -- load weather.csv --table weather --dir /tmp/db

# SQL query
cargo run --example cli -- query "SELECT city, AVG(temp) as avg_temp \
  FROM weather WHERE temp > 20 GROUP BY city" --dir /tmp/db

# Scan all
cargo run --example cli -- scan weather --dir /tmp/db

# Table info
cargo run --example cli -- info weather --dir /tmp/db
```

### Run all examples

```bash
cargo run --example demo_app          # 100K rows end-to-end
cargo run --example basic_operations  # 10-step CRUD walkthrough
cargo run --example time_series       # 3-server analytics
cargo run --example crud_workflow     # Product inventory cycle
cargo run --example showcase          # TuckDB-AI: full live demo
cargo run --example file_index_demo   # TuckDB-AI: real files → chunks → vector DB → queries
cargo run --example unified_demo      # TuckDB-AI: SQL routing (vector DB vs .tuck)
```

---

## 📖 API Reference

### Core types

```rust
DataType::Int64 | Float64 | Utf8 | Timestamp
Field::new(name, data_type, nullable)
Schema::new(vec![fields])  // .index_of(name) → Option<usize>

ColumnData::Int64(Vec<i64>) | Float64(Vec<f64>) | Utf8(Vec<String>) | Timestamp(Vec<i64>)
Column::new(field, data)
RecordBatch::new(schema, columns)
```

### Table

```rust
Table::create(name, schema, path)  // new in-memory table
Table::open(name, path)            // load from disk
table.insert_batch(batch)          // insert rows
table.execute(plan)                // run query → ResultSet
table.scan_all()                   // full scan → ResultSet
table.flush()                      // write .tuck file to disk
table.num_rows()                   // row count
```

### Query builder

```rust
LogicalPlan::scan("table")
    .filter(col("score").gt(lit_float(80.0))
        .and(col("active").eq(lit_int(1))))
    .project(&["name", "score", "active"])
    .aggregate(
        vec![(AggOp::Avg, "score", "avg_score")],
        vec!["active".to_string()],
    );
```

### Expressions

```rust
col("x").eq(lit_int(5))        // equality
col("x").neq(lit_int(5))       // not equal
col("x").gt(lit_float(10.5))   // greater than
col("x").gte(lit_int(0))       // greater than or equal
col("x").lt(lit_int(100))      // less than
col("x").lte(lit_float(50.0))  // less than or equal
a.and(b)                        // logical AND
a.or(b)                         // logical OR
```

### Aggregation

```
AggOp::Sum    AggOp::Count   AggOp::Avg
AggOp::Min    AggOp::Max
```

---

## 🆚 TuckDB vs alternatives

| Feature | **TuckDB** | DuckDB | SQLite | Parquet+Arrow |
|---------|-----------|--------|--------|---------------|
| **Server needed** | None | None | None | ❌ Needs engine |
| **Compression** | 6 co-designed encodings | Columnar (fixed) | Row (none) | Columnar (fixed) |
| **Query on compressed** | ✅ Stats skip chunks | ⚠️ Page-level | ❌ | ❌ |
| **Cache built-in** | ✅ Data + result | ❌ | ❌ | ❌ |
| **S3 native** | ✅ | ⚠️ Extension | ❌ | ❌ |
| **C API** | ✅ | ❌ (C++) | ✅ | ❌ |
| **Dependencies** | 1 (`zstd`) | ~50 crates | 0 | ~30 crates |
| **File format** | Self-describing `.tuck` | Needs catalog | Needs schema | Self-describing |
| **Memory safe** | ✅ Rust | ❌ C++ | ✅ C | Varies |

---

## ❓ Design FAQ

### How is data stored per table?

Each table gets its own `.tuck` file. `employees` → `employees.tuck`, `orders` → `orders.tuck`. No shared files, no catalog. The file is fully self-contained — copy it anywhere and open it directly.

### What's inside a .tuck file?

Everything is in one binary file — no separate metadata, no sidecar files:

```
.tuck file
├── Magic "TCKB" (identifies the format)
├── Version number
├── Schema (column names, types, nullability)
├── ChunkMeta[] — metadata for every column of every chunk:
│   col_idx, chunk_idx, encoding_id, offset, compressed_size,
│   uncompressed_size, min_value, max_value, null_count
└── Compressed data (actual column values)
```

### How does insert_batch() work?

Every `insert_batch()` call:
1. Appends the batch to an **in-memory list** — no disk I/O, no compression
2. Increments an internal version counter (tracks data freshness for cache)
3. Clears the result cache

**The .tuck file is NOT touched during insert.** Compression only happens on `flush()`.

### How are column values stored in memory?

Each column is stored as its own typed vector — not as rows:

- `Int64` → `Vec<i64>` — contiguous 8-byte integers
- `Float64` → `Vec<f64>` — contiguous 8-byte floats
- `Utf8` → `Vec<String>` — contiguous heap-allocated strings
- `Timestamp` → `Vec<i64>` — contiguous 8-byte integers

This is the columnar layout. All empid values are together, all empname values are together. This enables type-specific compression (Delta+Varint on ints, Dict+ZSTD on strings).

### What is a "chunk"?

Each batch you insert becomes **one chunk**. Batch N = Chunk N. No splitting, no merging, no rebalancing. A chunk with 1 row is stored the same way as a chunk with 1M rows — just with different sizes.

### How does flush() work?

1. Every column of every batch is compressed using its type's encoding (Delta+Varint for ints, XOR for floats, Dict+ZSTD for strings)
2. Column stats (min, max, null_count) are computed per chunk
3. ChunkMeta records are built with byte offsets, sizes, encoding IDs, and stats
4. The entire .tuck file is assembled: header → metadata → compressed data
5. The file is written to disk in one shot

**The entire file is rewritten on every flush.** No append, no in-place update. This is by design — you batch your inserts and call flush once.

### Does flush() happen automatically?

**No.** You must explicitly call `table.flush()`. There is no auto-flush, no periodic sync. You control when compression happens.

### How does chunk skipping work?

When querying with a filter like `WHERE salary > 50000`:

1. Read all ChunkMeta for the salary column (38 bytes each, ~300 bytes for 8 chunks)
2. For each chunk, check: "Does the filter match this chunk's [min, max] range?"
   - `col > X` → chunk passes if `max > X`
   - `col < X` → chunk passes if `min < X`
   - `col = X` → chunk passes if `min ≤ X ≤ max`
   - `A AND B` → both must pass
   - `A OR B` → either must pass
3. Chunks that can't match → **skipped, zero decompression**
4. Chunks that could match → read compressed data at recorded offset, decompress, filter rows

For selective filters on **sorted or ordered data**: 50-90% of chunks skipped.

### What types support chunk skipping?

Only **numeric types** (Int64, Float64, Timestamp) store min/max in ChunkMeta. **Utf8 (string) columns store min=None, max=None** — so string filters like `WHERE name = "swadhin"` or `WHERE address > "M"` cannot skip chunks. Every string chunk must be decompressed and scanned.

**Future**: Lexicographic min/max for strings, bloom filters for equality checks.

### How row count is maintained?

Each batch tracks its own row count. The table's total is the sum of all batch row counts. Row count is NOT stored explicitly in the .tuck file — it's recomputed by summing `uncompressed_size` from each chunk's metadata.

### Can I query before flush?

Yes. Before flush, the query engine reads from the in-memory `Vec<RecordBatch>`. Chunk skipping still works — stats are computed on the fly by scanning the raw Vec. After flush, the engine reads from the .tuck file in memory and uses the stored ChunkMeta for skipping.

### What happens if I insert after flush?

New batches are appended to the in-memory list. The .tuck file on disk is now stale — it doesn't include the new data. You must call `flush()` again to rewrite the .tuck file with all data (old + new).

**Limitation**: The current query engine after flush reads only from the .tuck file, ignoring any unflushed in-memory batches. **Future**: Merge operator that unions persisted and in-memory data so queries always see everything.

### How do Min/Max aggregation queries work?

There is no global min/max pre-computed. `SELECT MIN(salary)` scans the ChunkMeta for all salary chunks, takes the smallest min:

```
Chunk 0 salary: min=5000
Chunk 1 salary: min=30000
Chunk 2 salary: min=2000
→ Global min = 2000 (from metadata only, no decompression)
```

`SELECT AVG(salary)` or `COUNT(*) WHERE salary > X` requires decompressing matching chunks and scanning rows.

### Why can't strings skip chunks?

The ChunkMeta min/max fields store `f64` (64-bit float). Strings are not converted to f64 for stats. This is a known limitation. Future versions could store string min/max as byte-ordered binary (lexicographic comparison).

### What's with the offset in ChunkMeta?

The `offset` field stores the exact byte position in the .tuck file where each compressed chunk starts. This enables:
- **Random access**: Jump to any chunk without scanning
- **S3 range requests**: Request only the bytes you need via HTTP Range headers
- **Zero-copy reads**: Read the exact byte range into memory

### How many files does TuckDB create?

One `.tuck` file per table, plus the application itself. No WAL, no temp files, no lock files, no schema registry. The output directory contains only the `.tuck` files.

---

## 🔮 What's NOT There — Limitations & Future Scope

### Storage & File Format

| Missing | Why | Future |
|---------|-----|--------|
| Incremental append to .tuck | Entire file rewritten on every flush | Append-only format — new chunks appended, metadata updated at end |
| Multi-table blobs | One .tuck per table | Multi-table format with table directory in header |
| Checksums per chunk | No corruption detection | CRC32 per compressed chunk |
| Lazy loading from file | Entire file read into memory on open | Read chunks on demand, keep compressed file as primary store |
| Concurrent writers | No write-ahead log, no locking | WAL for crash-safe inserts |
| Delete / Update | Not supported | Row-level delete tombstones + compaction |

### Compression

| Missing | Why | Future |
|---------|-----|--------|
| String min/max stats | Stats store f64 only | Lexicographic string range for chunk skipping on text |
| Adaptive encoding selection | Currently hardcoded per type | Sample data → pick best encoder per chunk (e.g., RLE for runs, Delta for monotonic, Bitmap for 0/1) |
| Float compression beyond XOR | Bit-level only | FPC (FPC algorithm), ZSTD for floats |
| String encoding variety | Dictionary+ZSTD only | Plain unencoded (fast), FSST (string substition), Delta on string prefixes |
| Per-chunk encoding choice | Same encoding for all chunks of a type | Analyze each chunk separately, choose optimal encoding |

### Query Engine

| Missing | Why | Future |
|---------|-----|--------|
| ORDER BY / LIMIT | Not implemented | Sort operator with top-K optimization |
| JOINs | Not implemented | Hash join, nested loop join, bloom filter probe-side skipping |
| Subqueries / CTEs / Window functions | SQL parser doesn't support | Full SQL-92 support |
| Parallel query execution | Single-threaded Volcano model | Operator-level parallelism (rayon), partition chunks across threads |
| Streaming query results | Collects all batches first | Streaming ResultSet for large datasets |
| Merge unflushed + persisted data | Queries either memory or .tuck | Union operator combining both sources |

### Caching & Memory

| Missing | Why | Future |
|---------|-----|--------|
| Result cache persistence | In-memory only, lost on restart | Disk-backed result cache for warm restart |
| Cache admission policy | Insert on every decode | Cost-based admission (skip caching large low-value chunks) |
| Memory-mapped file access | Entire file in Vec<u8> | mmap for lazy page-in from OS |

### Concurrency

| Missing | Why | Future |
|---------|-----|--------|
| Thread-safe table access | Single-threaded (C API has Mutex) | `Arc<RwLock<Table>>` for concurrent readers |
| Async / non-blocking I/O | Synchronous read/write | tokio-based async backend for non-blocking S3/disk |
| Multi-process sharing | No file-level locking | Advisory locks, version-based conflict detection |

### Performance Optimizations (What's next)

| Optimization | Expected Gain | Complexity |
|-------------|---------------|------------|
| SIMD decode for Delta+Varint | 3-5x faster decode | Low |
| Sort on flush for tight chunk ranges | 5-10x more chunks skipped | Medium |
| Bloom filters for equality filters | Skip string equality checks without metadata scan | Medium |
| Adaptive encoding per chunk | Better compression on mixed data | Medium |
| Parallel chunk decode | 4-8x faster scan on multi-core | Low (chunks are independent) |
| Result cache reuse across queries | Sub-millisecond for repeated queries on static data | Already done |
| Memory-mapped .tuck files | Lower memory usage, instant open | Low |

---

## 🛣 Roadmap

```
✅  Phase 1-4: Core, compression, query engine, caching
✅  Phase 5:   CLI tool, SQL parser, 6 encodings, S3 backend, C API, benchmarks
⬜  Upcoming:  ORDER BY / LIMIT / JOIN, adaptive encoding, Python bindings,
               Arrow interop, concurrent readers
```

---

## 👨‍💻 About the author

**Swadhin Goswami** — Staff Software Engineer with **13+ years** building high-performance systems in C++, Rust, Linux, distributed systems, storage, and security.

```
C++ · Rust · Distributed Systems · Storage · Platform Engineering
Backend Infrastructure · Virtualization · Trusted Computing
```

I design and build **foundational infrastructure** — storage engines, backup systems, TPM security runtimes, and now TuckDB. Each project solves a real systems problem: performance, reliability, and clean architecture.

📧 **gsmswadhin@gmail.com** |  💻 **[github.com/swadhingoswami](https://github.com/swadhingoswami) https://www.linkedin.com/in/swadhin-goswami/**

> *Exploring Staff / Principal / Senior Software Engineer roles in systems infrastructure. If TuckDB or any of my projects resonate with your team's challenges, I'd love to chat.*

---

<p align="center">
  ⭐ Star on GitHub • 🐛 File issues • 💬 Start discussions<br>
  <i>Built with Rust. Open source. MIT license.</i>
</p>

<p align="center">
  <code>#Rust</code> <code>#SystemsProgramming</code> <code>#Database</code> <code>#Analytics</code>
  <code>#Compression</code> <code>#EmbeddedSystems</code> <code>#EdgeComputing</code>
  <code>#Performance</code> <code>#OpenSource</code> <code>#Storage</code>
  <code>#OLAP</code> <code>#Columnar</code> <code>#CPlusPlus</code>
  <code>#Linux</code> <code>#DistributedSystems</code>
</p>
