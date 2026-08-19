use std::collections::HashMap;
use std::path::Path;

/// Magic header for the serialized vector DB file.
pub const VECTOR_DB_MAGIC: &[u8; 4] = b"TKVB";
/// Format version of the serialized vector DB file.
pub const VECTOR_DB_VERSION: u32 = 1;

/// An embedding vector for a specific chunk at a specific source version,
/// produced by a specific embedding model.
#[derive(Debug, Clone, PartialEq)]
pub struct Embedding {
    pub document_id: i64,
    pub chunk_id: u32,
    pub source_version: u64,
    pub model_id: String,
    pub vector: Vec<f64>,
}

/// Stores embeddings keyed by (document_id, chunk_id).
pub struct EmbeddingStore {
    embeddings: HashMap<(i64, u32), Embedding>,
}

impl EmbeddingStore {
    pub fn new() -> Self {
        Self {
            embeddings: HashMap::new(),
        }
    }

    pub fn insert(&mut self, embedding: Embedding) {
        let key = (embedding.document_id, embedding.chunk_id);
        self.embeddings.insert(key, embedding);
    }

    pub fn get(&self, document_id: i64, chunk_id: u32) -> Option<&Embedding> {
        self.embeddings.get(&(document_id, chunk_id))
    }

    pub fn remove(&mut self, document_id: i64, chunk_id: u32) -> Option<Embedding> {
        self.embeddings.remove(&(document_id, chunk_id))
    }

    pub fn len(&self) -> usize {
        self.embeddings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.embeddings.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Embedding> {
        self.embeddings.values()
    }

    /// Serialize the vector DB to bytes (little-endian binary).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(VECTOR_DB_MAGIC);
        buf.extend_from_slice(&VECTOR_DB_VERSION.to_le_bytes());
        buf.extend_from_slice(&(self.embeddings.len() as u32).to_le_bytes());
        for e in self.embeddings.values() {
            buf.extend_from_slice(&e.document_id.to_le_bytes());
            buf.extend_from_slice(&e.chunk_id.to_le_bytes());
            buf.extend_from_slice(&e.source_version.to_le_bytes());
            buf.extend_from_slice(&(e.model_id.len() as u32).to_le_bytes());
            buf.extend_from_slice(e.model_id.as_bytes());
            buf.extend_from_slice(&(e.vector.len() as u32).to_le_bytes());
            for v in &e.vector {
                buf.extend_from_slice(&v.to_le_bytes());
            }
        }
        buf
    }

    /// Deserialize a vector DB from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 || &bytes[..4] != VECTOR_DB_MAGIC {
            return None;
        }
        let mut pos = 4;
        let _version = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
        pos += 4;
        let count = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
        pos += 4;
        let mut store = Self::new();
        for _ in 0..count {
            let document_id = i64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            let chunk_id = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
            pos += 4;
            let source_version = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            let name_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
            pos += 4;
            let model_id = String::from_utf8(bytes[pos..pos + name_len].to_vec()).ok()?;
            pos += name_len;
            let vec_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
            pos += 4;
            let mut vector = Vec::with_capacity(vec_len);
            for _ in 0..vec_len {
                vector.push(f64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?));
                pos += 8;
            }
            store.insert(Embedding {
                document_id,
                chunk_id,
                source_version,
                model_id,
                vector,
            });
        }
        Some(store)
    }

    /// Persist the vector DB to a file.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_bytes())
    }

    /// Load a vector DB from a file.
    pub fn load_from(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid vector db file")
        })
    }

    /// Brute-force top-K search by cosine similarity. Correctness first; no
    /// approximate index yet.
    pub fn search(&self, query: &[f64], k: usize) -> Vec<VectorMatch> {
        let mut scored: Vec<VectorMatch> = self
            .embeddings
            .values()
            .filter(|e| e.vector.len() == query.len())
            .map(|e| VectorMatch {
                document_id: e.document_id,
                chunk_id: e.chunk_id,
                source_version: e.source_version,
                model_id: e.model_id.clone(),
                score: cosine(query, &e.vector),
            })
            .collect();
        scored.sort_by(|a, b| b.score.total_cmp(&a.score));
        scored.truncate(k);
        scored
    }
}

/// Similarity match from a brute-force search.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorMatch {
    pub document_id: i64,
    pub chunk_id: u32,
    pub source_version: u64,
    pub model_id: String,
    pub score: f64,
}

/// Cosine similarity between two vectors, in [-1, 1].
pub fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

impl Default for EmbeddingStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(document_id: i64, chunk_id: u32, version: u64) -> Embedding {
        Embedding {
            document_id,
            chunk_id,
            source_version: version,
            model_id: "model-v1".to_string(),
            vector: vec![1.0, 2.0, 3.0],
        }
    }

    #[test]
    fn insert_get_remove() {
        let mut store = EmbeddingStore::new();
        store.insert(sample(1, 0, 1));
        assert_eq!(store.len(), 1);
        assert!(store.get(1, 0).is_some());
        assert!(store.get(1, 1).is_none());
        assert!(store.remove(1, 0).is_some());
        assert!(store.get(1, 0).is_none());
        assert!(store.is_empty());
    }

    #[test]
    fn overwrite_updates_embedding() {
        let mut store = EmbeddingStore::new();
        store.insert(sample(1, 1, 1));
        store.insert(sample(1, 1, 2));
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(1, 1).unwrap().source_version, 2);
    }

    #[test]
    fn iter_yields_all_embeddings() {
        let mut store = EmbeddingStore::new();
        store.insert(sample(1, 0, 1));
        store.insert(sample(1, 1, 1));
        store.insert(sample(2, 0, 1));
        assert_eq!(store.iter().count(), 3);
    }

    #[test]
    fn cosine_identical_is_one() {
        let v = vec![0.5, 0.5, 0.5, 0.5];
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine(&a, &b) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_opposite_is_minus_one() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        assert!((cosine(&a, &b) - -1.0).abs() < 1e-9);
    }

    #[test]
    fn search_returns_top_k_sorted() {
        let mut store = EmbeddingStore::new();
        store.insert(Embedding {
            document_id: 1,
            chunk_id: 0,
            source_version: 1,
            model_id: "m".to_string(),
            vector: vec![1.0, 0.0],
        });
        store.insert(Embedding {
            document_id: 1,
            chunk_id: 1,
            source_version: 1,
            model_id: "m".to_string(),
            vector: vec![0.0, 1.0],
        });
        store.insert(Embedding {
            document_id: 1,
            chunk_id: 2,
            source_version: 1,
            model_id: "m".to_string(),
            vector: vec![0.8, 0.6],
        });

        let query = vec![1.0, 0.0];
        let results = store.search(&query, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].chunk_id, 0);
        assert_eq!(results[1].chunk_id, 2);
        assert!(results[0].score >= results[1].score);
    }

    #[test]
    fn search_zero_k_is_empty() {
        let mut store = EmbeddingStore::new();
        store.insert(sample(1, 0, 1));
        assert!(store.search(&[1.0, 0.0, 0.0], 0).is_empty());
    }

    #[test]
    fn search_skips_dimension_mismatch() {
        let mut store = EmbeddingStore::new();
        store.insert(sample(1, 0, 1));
        assert!(store.search(&[1.0, 0.0], 5).is_empty());
    }

    #[test]
    fn roundtrip_serialization() {
        let mut store = EmbeddingStore::new();
        store.insert(sample(1, 0, 3));
        store.insert(sample(2, 5, 1));
        let bytes = store.to_bytes();
        let loaded = EmbeddingStore::from_bytes(&bytes).unwrap();
        assert_eq!(loaded.len(), 2);
        let e = loaded.get(1, 0).unwrap();
        assert_eq!(e.source_version, 3);
        assert_eq!(e.model_id, "model-v1");
        assert_eq!(e.vector, vec![1.0, 2.0, 3.0]);
        assert_eq!(loaded.get(2, 5).unwrap().document_id, 2);
    }

    #[test]
    fn from_bytes_rejects_bad_magic() {
        assert!(EmbeddingStore::from_bytes(b"XXXX").is_none());
    }

    #[test]
    fn empty_store_roundtrip() {
        let store = EmbeddingStore::new();
        let loaded = EmbeddingStore::from_bytes(&store.to_bytes()).unwrap();
        assert!(loaded.is_empty());
    }
}
