use std::path::PathBuf;
use std::sync::Arc;
use tuckdb::{
    ChunkChange, ChunkTracker, Column, ColumnData, ContentDedup, DataType, Embedding,
    EmbeddingProvider, EmbeddingStore, Field, IncrementalEngine, LifecycleManager, LifecycleState,
    LogicalPlan, MockEmbeddingProvider, RecordBatch, Schema, SemanticDedup, Table, VectorMatch,
    col, lit_float, lit_int, lit_str, similarity,
};

fn test_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/tuckdb_tests/{}", name));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(name: &str) {
    let dir = PathBuf::from(format!("/tmp/tuckdb_tests/{}", name));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_create_table_and_insert() {
    let dir = test_dir("create_insert");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Float64, true),
    ]);

    let mut table = Table::create("test_insert", schema.clone(), dir.clone());
    assert_eq!(table.name(), "test_insert");
    assert_eq!(table.num_rows(), 0);

    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1, 2, 3])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![10.0, 20.0, 30.0]),
            ),
        ],
    );
    table.insert_batch(batch);
    assert_eq!(table.num_rows(), 3);
    cleanup("create_insert");
}

#[test]
fn test_scan_all() {
    let dir = test_dir("scan_all");
    let schema = Schema::new(vec![Field::new("x", DataType::Int64, false)]);

    let mut table = Table::create("scan_test", schema.clone(), dir);
    let batch = RecordBatch::new(
        schema.clone(),
        vec![Column::new(
            schema.fields[0].clone(),
            ColumnData::Int64(vec![1, 2, 3, 4, 5]),
        )],
    );
    table.insert_batch(batch);

    let mut result_set = table.scan_all();
    let mut total = 0usize;
    while let Some(batch) = result_set.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 5);
    cleanup("scan_all");
}

#[test]
fn test_multiple_batches() {
    let dir = test_dir("multi_batch");
    let schema = Schema::new(vec![Field::new("val", DataType::Int64, false)]);
    let mut table = Table::create("multi", schema.clone(), dir);

    for i in 0..3 {
        let data: Vec<i64> = (i * 10..(i + 1) * 10).collect();
        let batch = RecordBatch::new(
            schema.clone(),
            vec![Column::new(
                schema.fields[0].clone(),
                ColumnData::Int64(data),
            )],
        );
        table.insert_batch(batch);
    }

    assert_eq!(table.num_rows(), 30);
    let mut total = 0usize;
    let mut result_set = table.scan_all();
    while let Some(batch) = result_set.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 30);
    cleanup("multi_batch");
}

#[test]
fn test_flush_and_open() {
    let dir = test_dir("flush_open");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ]);

    let mut table = Table::create("persist_test", schema.clone(), dir.clone());
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1, 2, 3])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec!["a".into(), "b".into(), "c".into()]),
            ),
        ],
    );
    table.insert_batch(batch);
    table.flush();

    let table2 = Table::open("persist_test", dir);
    assert_eq!(table2.num_rows(), 3);

    let mut rs = table2.scan_all();
    let mut total = 0;
    while let Some(batch) = rs.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 3);
    cleanup("flush_open");
}

#[test]
fn test_query_with_filter() {
    let dir = test_dir("filter");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("score", DataType::Float64, true),
    ]);

    let mut table = Table::create("filter_test", schema.clone(), dir);
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Int64(vec![1, 2, 3, 4, 5]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![10.0, 20.0, 30.0, 40.0, 50.0]),
            ),
        ],
    );
    table.insert_batch(batch);

    let plan = LogicalPlan::scan("filter_test").filter(col("id").gt(lit_int(2)));
    let mut rs = table.execute(plan);
    let mut total = 0;
    while let Some(batch) = rs.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 3);
    cleanup("filter");
}

#[test]
fn test_query_with_projection() {
    let dir = test_dir("proj");
    let schema = Schema::new(vec![
        Field::new("a", DataType::Int64, false),
        Field::new("b", DataType::Float64, true),
        Field::new("c", DataType::Utf8, false),
    ]);

    let mut table = Table::create("proj_test", schema.clone(), dir);
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1, 2, 3])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![1.0, 2.0, 3.0]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Utf8(vec!["x".into(), "y".into(), "z".into()]),
            ),
        ],
    );
    table.insert_batch(batch);

    let plan = LogicalPlan::scan("proj_test").project(&["a", "c"]);
    let mut rs = table.execute(plan);
    let batch = rs.next_batch().unwrap();
    assert_eq!(batch.columns.len(), 2);
    assert_eq!(batch.schema.fields[0].name, "a");
    assert_eq!(batch.schema.fields[1].name, "c");
    cleanup("proj");
}

#[test]
fn test_aggregate() {
    let dir = test_dir("agg");
    let schema = Schema::new(vec![
        Field::new("cat", DataType::Utf8, false),
        Field::new("val", DataType::Float64, true),
    ]);

    let mut table = Table::create("agg_test", schema.clone(), dir);

    let batch1 = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Utf8(vec!["a".into(), "b".into(), "a".into()]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![1.0, 2.0, 3.0]),
            ),
        ],
    );
    let batch2 = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Utf8(vec!["b".into(), "a".into()]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Float64(vec![4.0, 5.0]),
            ),
        ],
    );
    table.insert_batch(batch1);
    table.insert_batch(batch2);

    let plan = LogicalPlan::scan("agg_test").aggregate(
        vec![(tuckdb::AggOp::Sum, "val", "total")],
        vec!["cat".to_string()],
    );

    let mut rs = table.execute(plan);
    let batch = rs.next_batch().unwrap();
    assert_eq!(batch.num_rows, 2);

    // Verify sums: a = 1.0+3.0+5.0 = 9.0, b = 2.0+4.0 = 6.0
    let cat_col = &batch.columns[0];
    let sum_col = &batch.columns[1];
    match (&cat_col.data, &sum_col.data) {
        (tuckdb::ColumnData::Utf8(cats), tuckdb::ColumnData::Float64(sums)) => {
            let mut results: std::collections::HashMap<&str, f64> =
                std::collections::HashMap::new();
            for (c, s) in cats.iter().zip(sums.iter()) {
                results.insert(c.as_str(), *s);
            }
            assert!((results.get("a").copied().unwrap_or(0.0) - 9.0).abs() < 0.001);
            assert!((results.get("b").copied().unwrap_or(0.0) - 6.0).abs() < 0.001);
        }
        _ => panic!("unexpected column types"),
    }
    cleanup("agg");
}

#[test]
fn test_lifecycle_insert_new_document() {
    let mut lm = LifecycleManager::new();
    let content = [
        "What is RAII?",
        "RAII binds resource lifetime to object lifetime.",
        "memory",
    ];
    lm.register(1001, &content);

    let state = lm.get(1001).unwrap();
    assert_eq!(state.document_id, 1001);
    assert_eq!(state.version, 1);
    assert_eq!(state.state, LifecycleState::Active);
    assert_ne!(state.content_hash, 0);
    assert_eq!(lm.len(), 1);
    assert!(lm.get(999).is_none());
}

#[test]
fn test_lifecycle_update_identical_content() {
    let mut lm = LifecycleManager::new();
    let content = [
        "What is RAII?",
        "RAII binds resource lifetime to object lifetime.",
        "memory",
    ];
    lm.register(1, &content);

    let report = lm.apply_update(1, &content).unwrap();
    assert!(!report.content_changed);
    assert_eq!(report.previous_version, 1);
    assert_eq!(report.new_version, 1);
    assert_eq!(report.derived_state, LifecycleState::Active);
    assert_eq!(report.previous_hash, report.new_hash);

    let state = lm.get(1).unwrap();
    assert_eq!(state.version, 1);
    assert_eq!(state.state, LifecycleState::Active);
}

#[test]
fn test_lifecycle_update_changed_content() {
    let mut lm = LifecycleManager::new();
    lm.register(
        1,
        &[
            "What is RAII?",
            "RAII binds resource lifetime to object lifetime.",
            "memory",
        ],
    );

    let report = lm
        .apply_update(
            1,
            &[
                "What is RAII?",
                "RAII acquires resources in constructors and releases them in destructors.",
                "memory",
            ],
        )
        .unwrap();
    assert!(report.content_changed);
    assert_eq!(report.previous_version, 1);
    assert_eq!(report.new_version, 2);
    assert_eq!(report.derived_state, LifecycleState::Stale);
    assert_ne!(report.previous_hash, report.new_hash);

    let state = lm.get(1).unwrap();
    assert_eq!(state.version, 2);
    assert_eq!(state.state, LifecycleState::Stale);
    assert_eq!(state.content_hash, report.new_hash);
}

#[test]
fn test_exact_dedup_large_scale() {
    let mut dedup = ContentDedup::new();
    let total = 10_000usize;
    let duplicate_extra = 1_000usize;

    for i in 0..total - duplicate_extra {
        dedup.insert(&[&format!("unique_content_{}", i)]);
    }
    for i in 0..duplicate_extra {
        dedup.insert(&[&format!("unique_content_{}", i)]);
    }

    let report = dedup.report();
    assert_eq!(report.total_inputs, 10_000);
    assert_eq!(report.unique_contents, 9_000);
    assert_eq!(report.duplicates_detected, 1_000);
    assert_eq!(report.processing_avoided, 1_000);
    assert!((report.duplicate_ratio - 0.1).abs() < 1e-9);
}

#[test]
fn test_exact_dedup_shared_content() {
    let mut dedup = ContentDedup::new();
    dedup.insert(&["What is RAII?"]);

    assert!(dedup.lookup(&["What is RAII?"]).is_some());
    assert!(dedup.lookup(&["What is a virtual function?"]).is_none());

    let second = dedup.insert(&["What is RAII?"]);
    assert_eq!(second, tuckdb::DedupOutcome::Duplicate);
    assert_eq!(dedup.unique_count(), 1);
    assert_eq!(dedup.duplicates_detected(), 1);
}

#[test]
fn test_chunk_lifecycle_only_changed_section_reprocessed() {
    let mut tracker = ChunkTracker::new();
    let v1 = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers resources from another object.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";
    let v2 = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers ownership by moving resources instead of copying them.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";

    tracker.register(1, 1, v1);
    let report = tracker.apply_update(1, 2, v2).unwrap();

    assert_eq!(report.total_chunks, 3);
    assert_eq!(report.changed_chunks, 1);
    assert_eq!(report.unchanged_chunks, 2);
    assert_eq!(report.reprocessed_chunks, 1);
    assert!((report.processing_avoided_pct - 66.6666).abs() < 0.1);

    let changes: Vec<(u32, ChunkChange)> = report
        .diffs
        .iter()
        .map(|d| (d.chunk_id, d.change))
        .collect();
    assert_eq!(
        changes,
        vec![
            (0, ChunkChange::Skip),
            (1, ChunkChange::Process),
            (2, ChunkChange::Skip),
        ]
    );

    let stored = tracker.get(1).unwrap();
    assert_eq!(stored.len(), 3);
    assert_eq!(stored[1].source_version, 2);
    assert_eq!(stored[1].content_hash, report.diffs[1].new_hash);
}

#[test]
fn test_chunk_lifecycle_unchanged_content_skips_everything() {
    let mut tracker = ChunkTracker::new();
    let v1 = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers resources from another object.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";

    tracker.register(1, 1, v1);
    let report = tracker.apply_update(1, 2, v1).unwrap();
    assert_eq!(report.changed_chunks, 0);
    assert_eq!(report.reprocessed_chunks, 0);
    assert!(report.diffs.iter().all(|d| d.change == ChunkChange::Skip));
    assert!(report.processing_avoided_pct > 99.9);
}

#[test]
fn test_embed_and_store_all_chunks() {
    let provider = MockEmbeddingProvider::default();
    let mut tracker = ChunkTracker::new();
    let mut store = EmbeddingStore::new();

    let v1 = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers resources from another object.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";

    let chunks = tracker.register(1, 1, v1);
    for chunk in &chunks {
        let vector = provider.embed(&chunk.content);
        store.insert(Embedding {
            document_id: 1,
            chunk_id: chunk.chunk_id,
            source_version: chunk.source_version,
            model_id: provider.model_id().to_string(),
            vector,
        });
    }

    assert_eq!(store.len(), 3);
    for chunk in &chunks {
        let embedding = store.get(1, chunk.chunk_id).unwrap();
        assert_eq!(embedding.source_version, 1);
        assert_eq!(embedding.model_id, "model-v1");
        assert_eq!(embedding.vector.len(), provider.dimension());
    }
}

#[test]
fn test_incremental_embedding_only_changed_chunks() {
    let provider = MockEmbeddingProvider::default();
    let mut tracker = ChunkTracker::new();
    let mut store = EmbeddingStore::new();

    let v1 = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers resources from another object.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";
    let v2 = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers ownership by moving resources instead of copying them.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";

    let chunks = tracker.register(1, 1, v1);
    for chunk in &chunks {
        let vector = provider.embed(&chunk.content);
        store.insert(Embedding {
            document_id: 1,
            chunk_id: chunk.chunk_id,
            source_version: chunk.source_version,
            model_id: provider.model_id().to_string(),
            vector,
        });
    }

    let report = tracker.apply_update(1, 2, v2).unwrap();
    let current = tracker.get(1).unwrap().to_vec();
    let mut embed_calls = 0;
    for diff in &report.diffs {
        if diff.change == ChunkChange::Process {
            let content = &current[diff.chunk_id as usize].content;
            let vector = provider.embed(content);
            store.insert(Embedding {
                document_id: 1,
                chunk_id: diff.chunk_id,
                source_version: 2,
                model_id: provider.model_id().to_string(),
                vector,
            });
            embed_calls += 1;
        }
    }

    assert_eq!(embed_calls, 1);
    assert_eq!(report.changed_chunks, 1);
    assert_eq!(store.get(1, 0).unwrap().source_version, 1);
    assert_eq!(store.get(1, 1).unwrap().source_version, 2);
    assert_eq!(store.get(1, 2).unwrap().source_version, 1);
    assert_eq!(store.get(1, 1).unwrap().vector.len(), provider.dimension());
}

#[test]
fn test_vector_search_retrieves_semantic_match() {
    let provider = MockEmbeddingProvider::default();
    let mut tracker = ChunkTracker::new();
    let mut store = EmbeddingStore::new();

    let doc = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers resources from another object.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";

    let chunks = tracker.register(1, 1, doc);
    for chunk in &chunks {
        let vector = provider.embed(&chunk.content);
        store.insert(Embedding {
            document_id: 1,
            chunk_id: chunk.chunk_id,
            source_version: 1,
            model_id: provider.model_id().to_string(),
            vector,
        });
    }

    let query = provider.embed("How does C++ transfer ownership without copying?");
    let results: Vec<VectorMatch> = store.search(&query, 3);
    assert_eq!(results.len(), 3);
    assert!(results[0].score >= results[1].score);
    assert!(results[1].score >= results[2].score);

    let top = &tracker.get(1).unwrap()[results[0].chunk_id as usize].content;
    assert!(
        top.contains("move constructor"),
        "expected the move constructor chunk first, got: {}",
        top
    );
}

#[test]
fn test_vector_search_exact_query_ranks_top() {
    let provider = MockEmbeddingProvider::default();
    let mut tracker = ChunkTracker::new();
    let mut store = EmbeddingStore::new();

    let doc = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers resources from another object.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";

    let chunks = tracker.register(1, 1, doc);
    for chunk in &chunks {
        let vector = provider.embed(&chunk.content);
        store.insert(Embedding {
            document_id: 1,
            chunk_id: chunk.chunk_id,
            source_version: 1,
            model_id: provider.model_id().to_string(),
            vector,
        });
    }

    let exact = &chunks[2].content;
    let query = provider.embed(exact);
    let results = store.search(&query, 3);
    assert_eq!(results[0].chunk_id, 2);
    assert!(results[0].score > 0.99);
}

#[test]
fn test_similarity_filter_returns_semantic_match() {
    let dir = test_dir("similarity");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("question", DataType::Utf8, false),
        Field::new("answer", DataType::Utf8, false),
        Field::new("topic", DataType::Utf8, false),
    ]);
    let mut table = Table::create("cpp_questions", schema.clone(), dir);
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Int64(vec![1, 2, 3, 4]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec![
                    "What is RAII?".into(),
                    "What is a move constructor?".into(),
                    "What is a virtual function?".into(),
                    "What is a mutex?".into(),
                ]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Utf8(vec![
                    "RAII binds resource lifetime to object lifetime.".into(),
                    "A move constructor transfers resources from another object.".into(),
                    "A virtual function enables runtime polymorphism.".into(),
                    "A mutex provides mutual exclusion for concurrent access.".into(),
                ]),
            ),
            Column::new(
                schema.fields[3].clone(),
                ColumnData::Utf8(vec![
                    "memory".into(),
                    "cpp".into(),
                    "cpp".into(),
                    "concurrency".into(),
                ]),
            ),
        ],
    );
    table.insert_batch(batch);

    let plan = LogicalPlan::scan("cpp_questions")
        .filter(
            similarity("answer", "How does C++ transfer ownership without copying?")
                .gt(lit_float(0.4)),
        )
        .project(&["id", "question"]);
    let mut rs = table.execute(plan);
    let mut ids = Vec::new();
    while let Some(batch) = rs.next_batch() {
        if let ColumnData::Int64(v) = &batch.columns[0].data {
            ids.extend_from_slice(v);
        }
    }
    assert_eq!(ids, vec![2]);
    cleanup("similarity");
}

#[test]
fn test_similarity_exact_match_near_one() {
    let dir = test_dir("similarity_exact");
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("answer", DataType::Utf8, false),
    ]);
    let mut table = Table::create("exact", schema.clone(), dir);
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1, 2])),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec![
                    "A move constructor transfers resources from another object.".into(),
                    "Some unrelated text about baking bread.".into(),
                ]),
            ),
        ],
    );
    table.insert_batch(batch);

    let plan = LogicalPlan::scan("exact")
        .filter(
            similarity(
                "answer",
                "A move constructor transfers resources from another object.",
            )
            .gt(lit_float(0.99)),
        )
        .project(&["id"]);
    let mut rs = table.execute(plan);
    let mut ids = Vec::new();
    while let Some(batch) = rs.next_batch() {
        if let ColumnData::Int64(v) = &batch.columns[0].data {
            ids.extend_from_slice(v);
        }
    }
    assert_eq!(ids, vec![1]);
    cleanup("similarity_exact");
}

fn hybrid_table(dir: &PathBuf) -> Table {
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("question", DataType::Utf8, false),
        Field::new("answer", DataType::Utf8, false),
        Field::new("topic", DataType::Utf8, false),
    ]);
    let mut table = Table::create("cpp_questions", schema.clone(), dir.clone());
    let batch = RecordBatch::new(
        schema.clone(),
        vec![
            Column::new(
                schema.fields[0].clone(),
                ColumnData::Int64(vec![1, 2, 3, 4, 5]),
            ),
            Column::new(
                schema.fields[1].clone(),
                ColumnData::Utf8(vec![
                    "What is RAII?".into(),
                    "How is ownership moved without copying?".into(),
                    "What is a move constructor?".into(),
                    "What is a virtual function?".into(),
                    "What is a mutex?".into(),
                ]),
            ),
            Column::new(
                schema.fields[2].clone(),
                ColumnData::Utf8(vec![
                    "RAII binds resource lifetime to object lifetime.".into(),
                    "Move semantics transfer ownership of resources without copying.".into(),
                    "A move constructor transfers resources from another object.".into(),
                    "A virtual function enables runtime polymorphism.".into(),
                    "A mutex provides mutual exclusion for concurrent access.".into(),
                ]),
            ),
            Column::new(
                schema.fields[3].clone(),
                ColumnData::Utf8(vec![
                    "memory".into(),
                    "memory".into(),
                    "cpp".into(),
                    "cpp".into(),
                    "concurrency".into(),
                ]),
            ),
        ],
    );
    table.insert_batch(batch);
    table
}

#[test]
fn test_hybrid_structured_and_semantic_filter() {
    let dir = test_dir("hybrid");
    let mut table = hybrid_table(&dir);

    let plan = LogicalPlan::scan("cpp_questions")
        .filter(
            col("topic")
                .eq(lit_str("memory"))
                .and(similarity("answer", "How does C++ transfer ownership?").gt(lit_float(0.3))),
        )
        .project(&["id"]);
    let mut rs = table.execute(plan);
    let mut ids = Vec::new();
    while let Some(batch) = rs.next_batch() {
        if let ColumnData::Int64(v) = &batch.columns[0].data {
            ids.extend_from_slice(v);
        }
    }
    // id=3 has high similarity but topic='cpp' (excluded by structured filter);
    // id=1 topic='memory' but low similarity (excluded by vector filter);
    // only id=2 satisfies both.
    assert_eq!(ids, vec![2]);
    cleanup("hybrid");
}

#[test]
fn test_hybrid_query_with_limit() {
    let dir = test_dir("hybrid_limit");
    let mut table = hybrid_table(&dir);

    let plan = LogicalPlan::scan("cpp_questions")
        .filter(
            col("topic")
                .eq(lit_str("memory"))
                .and(similarity("answer", "How does C++ transfer ownership?").gt(lit_float(0.3))),
        )
        .limit(5);
    let mut rs = table.execute(plan);
    let mut total = 0;
    while let Some(batch) = rs.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 1);
    cleanup("hybrid_limit");
}

#[test]
fn test_limit_operator() {
    let dir = test_dir("limit");
    let schema = Schema::new(vec![Field::new("id", DataType::Int64, false)]);
    let mut table = Table::create("nums", schema.clone(), dir);
    let batch = RecordBatch::new(
        schema.clone(),
        vec![Column::new(
            schema.fields[0].clone(),
            ColumnData::Int64(vec![1, 2, 3, 4, 5]),
        )],
    );
    table.insert_batch(batch);

    let plan = LogicalPlan::scan("nums").limit(2);
    let mut rs = table.execute(plan);
    let mut total = 0;
    while let Some(batch) = rs.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 2);
    cleanup("limit");
}

#[test]
fn test_limit_across_batches() {
    let dir = test_dir("limit_batches");
    let schema = Schema::new(vec![Field::new("id", DataType::Int64, false)]);
    let mut table = Table::create("nums", schema.clone(), dir);
    for b in 0..3 {
        let data: Vec<i64> = (b * 10..b * 10 + 10).collect();
        let batch = RecordBatch::new(
            schema.clone(),
            vec![Column::new(
                schema.fields[0].clone(),
                ColumnData::Int64(data),
            )],
        );
        table.insert_batch(batch);
    }

    let plan = LogicalPlan::scan("nums").limit(15);
    let mut rs = table.execute(plan);
    let mut total = 0;
    while let Some(batch) = rs.next_batch() {
        total += batch.num_rows;
    }
    assert_eq!(total, 15);
    cleanup("limit_batches");
}

fn incr_content(id: usize) -> String {
    format!("S1 of {id}.\n\nS2 of {id}.\n\nS3 of {id}.\n\nS4 of {id}.\n\nS5 of {id}.")
}

fn incr_updated(id: usize) -> String {
    format!("S1 of {id}.\n\nS2 of {id} REVISED.\n\nS3 of {id}.\n\nS4 of {id}.\n\nS5 of {id}.")
}

#[test]
fn test_incremental_update_processing_reduction() {
    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    let docs = 1000usize;
    let changed = 100usize;

    for i in 0..docs {
        engine.ingest(i as i64, &incr_content(i));
    }
    assert_eq!(engine.store().len(), docs * 5);

    let mut generated = 0usize;
    for i in 0..changed {
        let report = engine.update(i as i64, &incr_updated(i)).unwrap();
        assert_eq!(report.embeddings_generated, 1);
        generated += report.embeddings_generated;
    }

    assert_eq!(generated, 100);
    assert_eq!(engine.store().len(), 5000);
    let avoided = (1.0 - generated as f64 / 5000.0) * 100.0;
    assert!((avoided - 98.0).abs() < 1e-9);
}

#[test]
fn test_incremental_update_index_stays_searchable() {
    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    engine.ingest(
        1,
        "Q1: What is RAII?\n\nA1: RAII binds resources to lifetime.",
    );
    engine.ingest(
        2,
        "Q2: What is a mutex?\n\nA2: A mutex excludes concurrent access.",
    );
    engine.ingest(
        3,
        "Q3: What is a virtual function?\n\nA3: Virtual functions dispatch at runtime.",
    );

    let new_answer =
        "Q2: What is a mutex?\n\nA2: A mutex provides mutual exclusion for concurrent threads.";
    let report = engine.update(2, new_answer).unwrap();
    assert_eq!(report.embeddings_generated, 1);
    assert_eq!(engine.store().get(2, 1).unwrap().source_version, 2);

    let provider = MockEmbeddingProvider::default();
    let qv = provider.embed("mutex mutual exclusion concurrent threads");
    let results = engine.store().search(&qv, 1);
    assert_eq!(results[0].document_id, 2);
    assert_eq!(results[0].chunk_id, 1);
}

#[test]
fn test_delete_cleans_only_affected_document() {
    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    let docs = 1000usize;
    for i in 0..docs {
        engine.ingest(i as i64, &incr_content(i));
    }
    assert_eq!(engine.store().len(), docs * 5);

    let report = engine.delete(100).unwrap();
    assert_eq!(report.affected_chunks, 5);
    assert_eq!(report.affected_vectors, 5);
    assert_eq!(report.vectors_remaining, 4995);
    assert_eq!(report.cleanup_work, 5);
    assert_eq!(engine.store().len(), 4995);
    assert!(engine.store().get(100, 0).is_none());
    assert!(engine.store().get(99, 0).is_some());
    assert!(engine.store().get(101, 0).is_some());
    assert!(engine.lifecycle().get(100).is_none());
    assert!(engine.chunks().get(100).is_none());
}

#[test]
fn test_deleted_document_not_searchable() {
    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    engine.ingest(
        1,
        "Q1: What is RAII?\n\nA1: RAII binds resources to lifetime.",
    );
    engine.ingest(
        2,
        "Q2: What is a mutex?\n\nA2: A mutex excludes concurrent access.",
    );

    engine.delete(2).unwrap();

    let provider = MockEmbeddingProvider::default();
    let qv = provider.embed("mutex concurrent access");
    let results = engine.store().search(&qv, 5);
    assert!(
        results.iter().all(|m| m.document_id != 2),
        "deleted document must never appear in results"
    );
}

#[test]
fn test_semantic_dedup_flags_near_duplicates_only() {
    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    engine.ingest(1, "What is RAII?");
    engine.ingest(2, "Explain Resource Acquisition Is Initialization.");
    engine.ingest(3, "RAII binds resource lifetime to object lifetime.");
    engine.ingest(
        4,
        "Resource Acquisition Is Initialization binds lifetime to object lifetime.",
    );
    engine.ingest(5, "What is a virtual function?");

    let detector = SemanticDedup::new(0.65);
    let report = detector.detect(engine.store());

    let pairs: Vec<(i64, i64)> = report
        .matches
        .iter()
        .map(|m| (m.document_id, m.other_document_id))
        .collect();
    assert!(pairs.contains(&(2, 4)));
    assert!(pairs.contains(&(3, 4)));
    assert!(pairs.contains(&(1, 2)));
    assert!(!pairs.contains(&(1, 5)));
    assert!(!pairs.contains(&(2, 5)));
    assert!(report.matches.iter().all(|m| m.similarity >= 0.65));
}

#[test]
fn test_model_migration_is_incremental_and_idempotent() {
    let mut engine = IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()));
    let docs = 100usize;
    for i in 0..docs {
        engine.ingest(i as i64, &incr_content(i));
    }
    assert_eq!(engine.store().len(), docs * 5);
    assert!(engine.store().iter().all(|e| e.model_id == "model-v1"));

    let v2 = Arc::new(MockEmbeddingProvider::new("model-v2", 128));
    let report = engine.migrate_model(v2.clone());
    assert_eq!(report.total_vectors, 500);
    assert_eq!(report.candidates, 500);
    assert_eq!(report.migrated, 500);
    assert_eq!(report.skipped, 0);
    assert!(engine.store().iter().all(|e| e.model_id == "model-v2"));

    let report = engine.migrate_model(v2);
    assert_eq!(report.migrated, 0);
    assert_eq!(report.skipped, 500);

    let qv = v2_embed_for_search();
    let results = engine.store().search(&qv, 1);
    assert_eq!(results.len(), 1);
}

fn v2_embed_for_search() -> Vec<f64> {
    let provider = MockEmbeddingProvider::new("model-v2", 128);
    use tuckdb::EmbeddingProvider;
    provider.embed("S2 of 5 REVISED")
}
