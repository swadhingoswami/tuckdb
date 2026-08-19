use tuckdb::embedding::{Embedding, EmbeddingProvider, EmbeddingStore, MockEmbeddingProvider};
use tuckdb::lifecycle::{ChunkChange, ChunkTracker};

fn main() {
    let v1 = "Q1: What is RAII?
A1: RAII binds resource lifetime to object lifetime.

Q2: What is a move constructor?
A2: A move constructor transfers resources from another object.

Q3: What is a virtual function?
A3: A virtual function enables runtime polymorphism.";

    let v2 = "Q1: What is RAII?
A1: RAII binds resource lifetime to object lifetime.

Q2: What is a move constructor?
A2: A move constructor transfers ownership by moving resources instead of copying them.

Q3: What is a virtual function?
A3: A virtual function enables runtime polymorphism.";

    println!("=== TuckDB-AI: Phase 4 — Basic Vector Representation ===");
    println!("A deterministic mock provider drives lifecycle mechanics without a real model.");
    println!();

    let provider = MockEmbeddingProvider::default();
    println!(
        "provider model_id = \"{}\", dimension = {}",
        provider.model_id(),
        provider.dimension()
    );
    let a = provider.embed("What is RAII?");
    let b = provider.embed("What is RAII?");
    println!(
        "deterministic: embed(\"What is RAII?\") == re-embed -> {}",
        a == b
    );
    println!();

    let mut tracker = ChunkTracker::new();
    let mut store = EmbeddingStore::new();

    println!("--- Embed all chunks of document V1 ---");
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
    for chunk in &chunks {
        let e = store.get(1, chunk.chunk_id).unwrap();
        println!(
            "chunk {} -> source_version={}, model=\"{}\", vector=[{:.4}, ... {} dims]",
            e.chunk_id,
            e.source_version,
            e.model_id,
            e.vector[0],
            e.vector.len()
        );
    }
    println!();

    println!("--- Update document V2: only Q2/A2 changed ---");
    let report = tracker.apply_update(1, 2, v2).unwrap();
    let current = tracker.get(1).unwrap().to_vec();
    let mut embedded = 0;
    for diff in &report.diffs {
        if diff.change == ChunkChange::Process {
            let vector = provider.embed(&current[diff.chunk_id as usize].content);
            store.insert(Embedding {
                document_id: 1,
                chunk_id: diff.chunk_id,
                source_version: 2,
                model_id: provider.model_id().to_string(),
                vector,
            });
            embedded += 1;
        }
    }
    println!(
        "embedding calls: {} ({} unchanged chunks skipped)",
        embedded, report.unchanged_chunks
    );
    println!();

    for diff in &report.diffs {
        let e = store.get(1, diff.chunk_id).unwrap();
        println!(
            "chunk {} -> source_version={}, model=\"{}\"",
            e.chunk_id, e.source_version, e.model_id
        );
    }
    println!("=> only the changed chunk was re-embedded (source_version = 2)");
}
