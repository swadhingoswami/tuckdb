# TuckDB-AI — End-to-End Demo (real output)

This is a **real, unedited transcript** of the end-to-end flow: upload a real text
file by path → chunk it → store vectors → query semantically → add another file →
delete a file — all incrementally, plus SQL routing and the headline benchmark.

Reproduce it any time with:

```bash
./scripts/e2e_demo.sh          # writes the same transcript to target/e2e_demo.txt
```

---

## PART 1 — Upload a real file → chunk → store → query

We index `data/demo_cpp.txt` (11 real C++ paragraphs) and ask four natural-language
questions. Every question returns the correct chunk.

```text
== PART 1: upload a real file -> chunk -> store -> query ==
> [add] doc 0 <- data/demo_cpp.txt: 11 new chunks embedded (0 -> 11 vectors; existing untouched)
      vector DB saved to /tmp/tuckdb_e2e/vector_db.tkdb
> indexed files (1):
  doc 0: data/demo_cpp.txt (11 chunks)
  total vectors in vector DB: 11
> Query: "How do I manage ownership of dynamically allocated memory?"
  #1 score=0.437 [data/demo_cpp.txt chunk 1]
     "Ownership of dynamically allocated memory is best expressed with smart pointers. std::unique_ptr is an exclusive owner; "
> Query: "What happens when an object goes out of scope?"
  #1 score=0.296 [data/demo_cpp.txt chunk 2]
     "Resource Acquisition Is Initialization, RAII, means the lifetime of a resource is tied to the lifetime of an object. The"
> Query: "Why do move constructors avoid expensive copies?"
  #1 score=0.693 [data/demo_cpp.txt chunk 3]
     "Move semantics avoid expensive deep copies. A move constructor steals the internal buffers and pointers from a temporary"
> Query: "What is runtime polymorphism?"
  #1 score=0.540 [data/demo_cpp.txt chunk 7]
     "Virtual functions provide runtime polymorphism through the vtable. A base class declares a function virtual, derived cla"
```

## PART 2 — New session (restart): nothing re-processed; add another file; delete

Key evidence: the vector DB is **persisted**, so on restart nothing is re-processed.
Adding `README.md` touches only the new file; deleting `doc 0` removes only its 11
vectors.

```text
== PART 2 (NEW session): nothing re-processed; add another file; delete ==
[load] restored 1 docs / 11 vectors from /tmp/tuckdb_e2e/vector_db.tkdb — nothing re-processed

> [add] doc 1 <- README.md: 363 new chunks embedded (11 -> 374 vectors; existing untouched)
      vector DB saved to /tmp/tuckdb_e2e/vector_db.tkdb
> indexed files (2):
  doc 0: data/demo_cpp.txt (11 chunks)
  doc 1: README.md (363 chunks)
  total vectors in vector DB: 374
> Query: "What is inside a tuck file?"
  #1 score=0.774 [README.md chunk 290]
     "### What's inside a .tuck file?"
> [del] doc 0: removed 11 vectors directly (374 -> 363); unrelated untouched
> indexed files (1):
  doc 1: README.md (363 chunks)
  total vectors in vector DB: 363
```

## PART 3 — SQL routing: structured (`.tuck`) vs vector (`SIMILARITY`)

The query itself decides the route. `WHERE topic = 'cpp'` reads the `.tuck` file;
`VECTOR_SEARCH(answer, …)` searches the vector DB.

```text
== PART 3: SQL routing - structured (.tuck) vs vector (SIMILARITY) ==
[data]   loaded 5 rows x 4 columns from data/cpp_questions.csv
[tuck]   persisted cpp_questions.tuck (618 bytes)
[vector] indexed 5 rows -> 20 chunks -> 20 vectors (model "model-v1")
[vector] vector DB persisted to /tmp/tuckdb_unified/cpp_questions.tvec (21212 bytes)
[vector] loaded 20 vectors from vector DB file /tmp/tuckdb_unified/cpp_questions.tvec (no re-embedding needed)

SQL: SELECT id, question FROM cpp_questions WHERE topic = 'cpp'
Route: STRUCTURED ENGINE (no SIMILARITY) - reads from cpp_questions.tuck
    results (2 rows):
      3 | What is a move constructor?
      4 | What is a virtual function?

SQL: SELECT id FROM cpp_questions WHERE VECTOR_SEARCH(answer, 'How does C++ transfer ownership without copying?', 0.3) LIMIT 3
Route: VECTOR ENGINE (identified by clause: VECTOR_SEARCH(...))
    semantic: column="answer" query="How does C++ transfer ownership without copying?" threshold=0.3
    results (2 rows):
      #1 score=0.873 row=1: 2
      #2 score=0.790 row=2: 3
```

## PART 4 — Headline benchmark (500,000 chunks / 1,000 changed)

```text
== PART 4: headline benchmark (500,000 chunks / 1,000 changed) ==
Dataset:                   500000 chunks
Changed:                     1000
Processed:                   1000
Skipped:                   499000

Work avoided:            99.8%
Update phase elapsed:       5 ms

Baseline (naive) would re-embed all 500000 chunks; incremental embedded only 1000.
```

---

## What this demonstrates

1. **Real data, real questions** — a text file on disk is chunked, embedded, stored,
   and searched; no hard-coded rows or questions.
2. **Persistence** — restarting restores the DB with *nothing* re-processed.
3. **Incrementality** — adding a file processes only that file; deleting a file
   removes only its vectors (no full scan, no re-processing of the rest).
4. **Unified SQL + vector** — the same query language routes to the `.tuck` engine
   or the vector engine based on whether `SIMILARITY`/`VECTOR_SEARCH` is present.
5. **The core metric** — 99.8% of embedding work avoided when 0.2% of data changes.

See [VECTOR_DB.md](VECTOR_DB.md) for architecture, flows, and the API, and
[PROGRESS.md](PROGRESS.md) for phase-by-phase evidence.
