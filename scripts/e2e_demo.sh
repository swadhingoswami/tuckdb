#!/usr/bin/env bash
set -euo pipefail

# Pre-build so the transcript has no compile noise.
cargo build --examples --quiet
cargo build --release --example incremental_demo --quiet

OUT="target/e2e_demo.txt"
mkdir -p target
rm -rf /tmp/tuckdb_e2e

{
  echo "============================================================================"
  echo "  TuckDB-AI | End-to-End Demo (real files, real queries, real output)"
  echo "  'Process what changed. Leave everything else untouched.'"
  echo "============================================================================"
  echo

  echo "== PART 1: upload a real file -> chunk -> store -> query =="
  printf 'add data/demo_cpp.txt\nlist\nq How do I manage ownership of dynamically allocated memory?\nq What happens when an object goes out of scope?\nq Why do move constructors avoid expensive copies?\nq What is runtime polymorphism?\nexit\n' \
    | cargo run -q --example file_index_demo -- --dir /tmp/tuckdb_e2e --top 1

  echo
  echo "== PART 2 (NEW session): nothing re-processed; add another file; delete =="
  printf 'add README.md\nlist\nq What is inside a tuck file?\ndel 0\nlist\nexit\n' \
    | cargo run -q --example file_index_demo -- --dir /tmp/tuckdb_e2e --top 1

  echo
  echo "== PART 3: SQL routing - structured (.tuck) vs vector (SIMILARITY) =="
  cargo run -q --example unified_demo -- --csv data/cpp_questions.csv \
    --query "SELECT id, question FROM cpp_questions WHERE topic = 'cpp'" \
    --query "SELECT id FROM cpp_questions WHERE VECTOR_SEARCH(answer, 'How does C++ transfer ownership without copying?', 0.3) LIMIT 3"

  echo
  echo "== PART 4: headline benchmark (500,000 chunks / 1,000 changed) =="
  cargo run --release --example incremental_demo -- --scale 100000 --changed 1000 2>&1 | sed -n '/^Dataset:/,$p'

  echo
  echo "End-to-end demo complete. This transcript is also saved to $OUT"
} 2>&1 | tee "$OUT"
