use tuckdb::embedding::{Embedding, EmbeddingProvider, EmbeddingStore, MockEmbeddingProvider};
use tuckdb::lifecycle::ChunkTracker;

fn main() {
    println!("=== TuckDB-AI: Phase 5 — Vector Storage & Similarity Search ===");
    println!("Brute-force cosine search over stored embeddings (no ANN yet).");
    println!();

    let provider = MockEmbeddingProvider::default();
    let mut tracker = ChunkTracker::new();
    let mut store = EmbeddingStore::new();

    let doc = "Q1: What is RAII?\nA1: Resource Acquisition Is Initialization binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers resources from another object.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.\n\nQ4: What is a mutex?\nA4: A mutex provides mutual exclusion for concurrent access.";

    let chunks = tracker.register(1, 1, doc);
    println!(
        "Indexing {} chunks (model=\"{}\", {} dims)...",
        chunks.len(),
        provider.model_id(),
        provider.dimension()
    );
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
    println!("Vectors indexed: {}\n", store.len());

    let query = "How does C++ transfer ownership without copying?";
    println!("Query: \"{}\"\n", query);
    let qv = provider.embed(query);
    let results = store.search(&qv, 4);
    for (i, m) in results.iter().enumerate() {
        let content = &tracker.get(1).unwrap()[m.chunk_id as usize].content;
        println!(
            "#{} score={:.4} chunk {} source_version={} model=\"{}\"",
            i + 1,
            m.score,
            m.chunk_id,
            m.source_version,
            m.model_id
        );
        println!("   {}", content.replace('\n', " | "));
    }

    let top = &tracker.get(1).unwrap()[results[0].chunk_id as usize].content;
    assert!(
        top.contains("move constructor"),
        "expected the move constructor chunk first"
    );
    println!();
    println!("=> top result is 'What is a move constructor?' even though the query words differ.");
}
