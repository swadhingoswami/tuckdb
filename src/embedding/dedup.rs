use crate::embedding::{Embedding, EmbeddingStore, cosine};

/// A pair of chunks that are potentially semantic duplicates.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticDupMatch {
    pub document_id: i64,
    pub chunk_id: u32,
    pub other_document_id: i64,
    pub other_chunk_id: u32,
    pub similarity: f64,
}

/// Report of detected semantic duplicate candidates.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticDedupReport {
    pub matches: Vec<SemanticDupMatch>,
    pub pairs_compared: usize,
}

/// Detects potential semantic duplicates using embeddings.
///
/// Candidates are only reported; nothing is deleted automatically. Exact
/// byte-identical duplicates are covered separately by exact deduplication.
pub struct SemanticDedup {
    threshold: f64,
}

impl SemanticDedup {
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }

    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// Brute-force candidate detection over all stored embeddings.
    /// Pairs are emitted in deterministic order (sorted by key), each pair
    /// once, sorted by descending similarity.
    pub fn detect(&self, store: &EmbeddingStore) -> SemanticDedupReport {
        let mut all: Vec<&Embedding> = store.iter().collect();
        all.sort_by_key(|e| (e.document_id, e.chunk_id));

        let mut matches = Vec::new();
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                let a = all[i];
                let b = all[j];
                let similarity = cosine(&a.vector, &b.vector);
                if similarity >= self.threshold {
                    matches.push(SemanticDupMatch {
                        document_id: a.document_id,
                        chunk_id: a.chunk_id,
                        other_document_id: b.document_id,
                        other_chunk_id: b.chunk_id,
                        similarity,
                    });
                }
            }
        }
        matches.sort_by(|x, y| {
            y.similarity
                .total_cmp(&x.similarity)
                .then_with(|| x.document_id.cmp(&y.document_id))
                .then_with(|| x.chunk_id.cmp(&y.chunk_id))
        });

        SemanticDedupReport {
            matches,
            pairs_compared: all.len().saturating_mul(all.len().saturating_sub(1)) / 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::MockEmbeddingProvider;
    use crate::embedding::provider::EmbeddingProvider;

    fn embed(
        document_id: i64,
        chunk_id: u32,
        content: &str,
        provider: &MockEmbeddingProvider,
    ) -> Embedding {
        Embedding {
            document_id,
            chunk_id,
            source_version: 1,
            model_id: provider.model_id().to_string(),
            vector: provider.embed(content),
        }
    }

    fn sample_store() -> EmbeddingStore {
        let provider = MockEmbeddingProvider::default();
        let mut store = EmbeddingStore::new();
        store.insert(embed(1, 0, "What is RAII?", &provider));
        store.insert(embed(
            2,
            0,
            "Explain Resource Acquisition Is Initialization.",
            &provider,
        ));
        store.insert(embed(
            3,
            0,
            "RAII binds resource lifetime to object lifetime.",
            &provider,
        ));
        store.insert(embed(
            4,
            0,
            "Resource Acquisition Is Initialization binds lifetime to object lifetime.",
            &provider,
        ));
        store.insert(embed(5, 0, "What is a virtual function?", &provider));
        store
    }

    #[test]
    fn detects_near_duplicate_pair() {
        let store = sample_store();
        let detector = SemanticDedup::new(0.65);
        let report = detector.detect(&store);

        let pairs: Vec<(i64, i64)> = report
            .matches
            .iter()
            .map(|m| (m.document_id, m.other_document_id))
            .collect();
        assert!(
            pairs.contains(&(2, 4)),
            "RAII paraphrase family should match, got {:?}",
            pairs
        );
        assert!(pairs.contains(&(3, 4)));
        assert!(
            pairs.contains(&(1, 2)),
            "the short RAII question is a paraphrase of the full expansion"
        );
        assert!(!pairs.contains(&(1, 5)));
        assert!(!pairs.contains(&(2, 5)));
    }

    #[test]
    fn identical_content_similarity_is_one() {
        let provider = MockEmbeddingProvider::default();
        let mut store = EmbeddingStore::new();
        store.insert(embed(1, 0, "What is RAII?", &provider));
        store.insert(embed(2, 0, "What is RAII?", &provider));

        let detector = SemanticDedup::new(1.0);
        let report = detector.detect(&store);
        assert_eq!(report.matches.len(), 1);
        assert!(report.matches[0].similarity > 0.9999);
        assert_eq!(report.matches[0].document_id, 1);
        assert_eq!(report.matches[0].other_document_id, 2);
    }

    #[test]
    fn detection_is_deterministic() {
        let store = sample_store();
        let detector = SemanticDedup::new(0.3);
        let a = detector.detect(&store);
        let b = detector.detect(&store);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_store_reports_no_matches() {
        let store = EmbeddingStore::new();
        let detector = SemanticDedup::new(0.0);
        let report = detector.detect(&store);
        assert!(report.matches.is_empty());
        assert_eq!(report.pairs_compared, 0);
    }
}
