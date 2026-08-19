#!/usr/bin/env bash
set -euo pipefail

echo "================================================================"
echo "  TuckDB-AI | Incremental AI Data Engine - Live Showcase"
echo "  'Process what changed. Leave everything else untouched.'"
echo "================================================================"
echo

echo ">> [0/3] Correctness: unit + integration tests"
cargo test --quiet
echo

echo ">> [1/3] Full live demo"
cargo run --release --example showcase -- "${@:-}"
echo

echo ">> [2/3] Scaling benchmark report"
cargo run --release --example benchmark_report
echo

echo ">> [3/3] Done. Evidence:"
echo "      - docs/PROGRESS.md        (proposed vs implemented)"
echo "      - target/benchmark_report.md"
