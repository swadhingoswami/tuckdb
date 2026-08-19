use std::sync::Arc;

use crate::embedding::{
    DEFAULT_EMBEDDING_DIM, Embedding, EmbeddingProvider, EmbeddingStore, MockEmbeddingProvider,
};
use crate::lifecycle::{
    Chunk, ChunkChange, ChunkTracker, LifecycleManager, LifecycleState, SourceState,
};

/// Magic header for the serialized engine database file.
pub const ENGINE_DB_MAGIC: &[u8; 4] = b"TKIE";
/// Format version of the serialized engine database file.
pub const ENGINE_DB_VERSION: u32 = 1;

/// Result of ingesting a new document into the incremental engine.
#[derive(Debug, Clone, PartialEq)]
pub struct IngestReport {
    pub document_id: i64,
    pub chunks: usize,
}

/// Result of an incremental document update.
#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalReport {
    pub document_id: i64,
    pub total_chunks: usize,
    pub changed_chunks: usize,
    pub embeddings_generated: usize,
    pub embeddings_skipped: usize,
    pub embeddings_removed: usize,
    pub work_avoided_pct: f64,
}

/// Result of deleting a document and its derived data.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteReport {
    pub document_id: i64,
    pub affected_chunks: usize,
    pub affected_vectors: usize,
    pub vectors_remaining: usize,
    pub cleanup_work: usize,
}

/// Result of migrating embeddings to a new embedding model.
#[derive(Debug, Clone, PartialEq)]
pub struct MigrateReport {
    pub old_model: String,
    pub new_model: String,
    pub total_vectors: usize,
    pub candidates: usize,
    pub migrated: usize,
    pub skipped: usize,
}

/// Connects the lifecycle engine to the vector engine.
///
/// On update it detects the content change, diffs the document's chunks, and
/// regenerates embeddings only for the changed chunks. Unchanged chunks and
/// their vectors are left untouched.
pub struct IncrementalEngine {
    lifecycle: LifecycleManager,
    chunks: ChunkTracker,
    store: EmbeddingStore,
    provider: Arc<dyn EmbeddingProvider>,
}

impl IncrementalEngine {
    pub fn new(provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            lifecycle: LifecycleManager::new(),
            chunks: ChunkTracker::new(),
            store: EmbeddingStore::new(),
            provider,
        }
    }

    /// Register a document and embed all of its chunks into the vector store.
    pub fn ingest(&mut self, document_id: i64, content: &str) -> IngestReport {
        self.lifecycle.register(document_id, &[content]);
        let version = self
            .lifecycle
            .get(document_id)
            .map(|s| s.version)
            .unwrap_or(1);
        let chunks = self.chunks.register(document_id, version, content);
        for chunk in &chunks {
            let vector = self.provider.embed(&chunk.content);
            self.store.insert(Embedding {
                document_id,
                chunk_id: chunk.chunk_id,
                source_version: chunk.source_version,
                model_id: self.provider.model_id().to_string(),
                vector,
            });
        }
        IngestReport {
            document_id,
            chunks: chunks.len(),
        }
    }

    /// Update a document's content, re-embedding only the changed chunks.
    pub fn update(
        &mut self,
        document_id: i64,
        new_content: &str,
    ) -> Result<IncrementalReport, String> {
        let change = self.lifecycle.apply_update(document_id, &[new_content])?;
        let old_ids: Vec<u32> = self
            .chunks
            .get(document_id)
            .map(|c| c.iter().map(|c| c.chunk_id).collect())
            .unwrap_or_default();

        if !change.content_changed {
            let total = old_ids.len();
            return Ok(IncrementalReport {
                document_id,
                total_chunks: total,
                changed_chunks: 0,
                embeddings_generated: 0,
                embeddings_skipped: total,
                embeddings_removed: 0,
                work_avoided_pct: if total == 0 { 0.0 } else { 100.0 },
            });
        }

        let diff = self
            .chunks
            .apply_update(document_id, change.new_version, new_content)?;
        let new_chunks = self.chunks.get(document_id).unwrap();

        let mut generated = 0;
        for d in &diff.diffs {
            if d.change == ChunkChange::Process {
                let chunk = &new_chunks[d.chunk_id as usize];
                let vector = self.provider.embed(&chunk.content);
                self.store.insert(Embedding {
                    document_id,
                    chunk_id: chunk.chunk_id,
                    source_version: chunk.source_version,
                    model_id: self.provider.model_id().to_string(),
                    vector,
                });
                generated += 1;
            }
        }

        let new_ids: Vec<u32> = new_chunks.iter().map(|c| c.chunk_id).collect();
        let mut removed = 0;
        for id in old_ids {
            if !new_ids.contains(&id) && self.store.remove(document_id, id).is_some() {
                removed += 1;
            }
        }

        let total = new_chunks.len();
        let skipped = total.saturating_sub(generated);
        let work_avoided = if total == 0 {
            0.0
        } else {
            (1.0 - generated as f64 / total as f64) * 100.0
        };
        Ok(IncrementalReport {
            document_id,
            total_chunks: total,
            changed_chunks: diff.changed_chunks,
            embeddings_generated: generated,
            embeddings_skipped: skipped,
            embeddings_removed: removed,
            work_avoided_pct: work_avoided,
        })
    }

    /// Delete a document, cleaning only its derived data.
    ///
    /// The affected chunk ids are resolved directly from the chunk tracker, so
    /// cleanup is O(chunks in the document), never a scan of the vector store.
    pub fn delete(&mut self, document_id: i64) -> Result<DeleteReport, String> {
        let chunks = self
            .chunks
            .remove(document_id)
            .ok_or_else(|| format!("document {} is not tracked", document_id))?;
        let affected_chunks = chunks.len();

        let mut affected_vectors = 0;
        for chunk in &chunks {
            if self.store.remove(document_id, chunk.chunk_id).is_some() {
                affected_vectors += 1;
            }
        }
        self.lifecycle.remove(document_id);

        let vectors_remaining = self.store.len();
        Ok(DeleteReport {
            document_id,
            affected_chunks,
            affected_vectors,
            vectors_remaining,
            cleanup_work: affected_vectors,
        })
    }

    /// Migrate embeddings to a new embedding model.
    ///
    /// Every vector records which model produced it. When the model changes,
    /// only vectors generated by the old model are re-embedded (their
    /// `source_version` is preserved); vectors already on the new model are
    /// skipped. The engine's provider is switched to the new model.
    pub fn migrate_model(&mut self, new_provider: Arc<dyn EmbeddingProvider>) -> MigrateReport {
        let new_model = new_provider.model_id().to_string();
        let old_model = self.provider.model_id().to_string();

        let keys: Vec<(i64, u32)> = self
            .store
            .iter()
            .map(|e| (e.document_id, e.chunk_id))
            .collect();

        let mut migrated = 0;
        let mut skipped = 0;
        let chunks = &self.chunks;
        for (document_id, chunk_id) in keys {
            let embedding = match self.store.get(document_id, chunk_id) {
                Some(e) => e,
                None => continue,
            };
            if embedding.model_id == new_model {
                skipped += 1;
                continue;
            }
            let source_version = embedding.source_version;
            let chunk = match chunks.get(document_id) {
                Some(cc) if (chunk_id as usize) < cc.len() => &cc[chunk_id as usize],
                _ => {
                    skipped += 1;
                    continue;
                }
            };
            let vector = new_provider.embed(&chunk.content);
            self.store.insert(Embedding {
                document_id,
                chunk_id,
                source_version,
                model_id: new_model.clone(),
                vector,
            });
            migrated += 1;
        }

        self.provider = new_provider;
        MigrateReport {
            old_model,
            new_model,
            total_vectors: self.store.len(),
            candidates: migrated,
            migrated,
            skipped,
        }
    }

    pub fn store(&self) -> &EmbeddingStore {
        &self.store
    }

    /// Serialize the whole engine state (lifecycle + chunks + vectors).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(ENGINE_DB_MAGIC);
        buf.extend_from_slice(&ENGINE_DB_VERSION.to_le_bytes());

        let model_id = self.provider.model_id();
        buf.extend_from_slice(&(model_id.len() as u32).to_le_bytes());
        buf.extend_from_slice(model_id.as_bytes());

        let lifecycle: Vec<(i64, SourceState)> = self
            .lifecycle
            .iter()
            .map(|(id, s)| (id, s.clone()))
            .collect();
        buf.extend_from_slice(&(lifecycle.len() as u32).to_le_bytes());
        for (id, s) in lifecycle {
            buf.extend_from_slice(&id.to_le_bytes());
            buf.extend_from_slice(&s.version.to_le_bytes());
            buf.extend_from_slice(&s.content_hash.to_le_bytes());
            let state = match s.state {
                LifecycleState::Active => 0u8,
                LifecycleState::Stale => 1u8,
            };
            buf.push(state);
        }

        let docs: Vec<(i64, u64, Vec<Chunk>)> = self
            .chunks
            .iter()
            .map(|(id, version, chunks)| (id, version, chunks.to_vec()))
            .collect();
        buf.extend_from_slice(&(docs.len() as u32).to_le_bytes());
        for (id, version, chunks) in docs {
            buf.extend_from_slice(&id.to_le_bytes());
            buf.extend_from_slice(&version.to_le_bytes());
            buf.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
            for c in &chunks {
                buf.extend_from_slice(&c.chunk_id.to_le_bytes());
                buf.extend_from_slice(&c.source_version.to_le_bytes());
                buf.extend_from_slice(&c.content_hash.to_le_bytes());
                buf.extend_from_slice(&(c.content.len() as u32).to_le_bytes());
                buf.extend_from_slice(c.content.as_bytes());
            }
        }

        let store_bytes = self.store.to_bytes();
        buf.extend_from_slice(&(store_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&store_bytes);
        buf
    }

    /// Deserialize an engine database from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 || &bytes[..4] != ENGINE_DB_MAGIC {
            return None;
        }
        let mut pos = 4;
        let _version = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
        pos += 4;

        let model_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        pos += 4;
        let model_id = String::from_utf8(bytes[pos..pos + model_len].to_vec()).ok()?;
        pos += model_len;

        let lc = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
        pos += 4;
        let mut lifecycle = Vec::new();
        for _ in 0..lc {
            let id = i64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            let version = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            let content_hash = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            let state = match bytes[pos] {
                0 => LifecycleState::Active,
                _ => LifecycleState::Stale,
            };
            pos += 1;
            lifecycle.push((
                id,
                SourceState {
                    document_id: id,
                    version,
                    content_hash,
                    state,
                },
            ));
        }

        let dc = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
        pos += 4;
        let mut docs = Vec::new();
        for _ in 0..dc {
            let id = i64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            let version = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            let n = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
            pos += 4;
            let mut chunks = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let chunk_id = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?);
                pos += 4;
                let source_version = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
                pos += 8;
                let content_hash = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
                pos += 8;
                let clen = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
                pos += 4;
                let content = String::from_utf8(bytes[pos..pos + clen].to_vec()).ok()?;
                pos += clen;
                chunks.push(Chunk {
                    document_id: id,
                    source_version,
                    chunk_id,
                    content_hash,
                    content,
                });
            }
            docs.push((id, version, chunks));
        }

        let store_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        pos += 4;
        let store = EmbeddingStore::from_bytes(&bytes[pos..pos + store_len])?;

        Some(Self {
            lifecycle: LifecycleManager::from_entries(lifecycle),
            chunks: ChunkTracker::from_entries(docs),
            store,
            provider: Arc::new(MockEmbeddingProvider::new(&model_id, DEFAULT_EMBEDDING_DIM)),
        })
    }

    /// Persist the whole engine database to a file.
    pub fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_bytes())
    }

    /// Load a whole engine database from a file.
    pub fn load_from(path: &std::path::Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid engine database file",
            )
        })
    }

    pub fn store_mut(&mut self) -> &mut EmbeddingStore {
        &mut self.store
    }

    pub fn chunks(&self) -> &ChunkTracker {
        &self.chunks
    }

    pub fn lifecycle(&self) -> &LifecycleManager {
        &self.lifecycle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::MockEmbeddingProvider;
    use crate::lifecycle::LifecycleState;

    fn content(id: usize) -> String {
        format!(
            "Section 1 of document {id}.\n\nSection 2 of document {id}.\n\nSection 3 of document {id}.\n\nSection 4 of document {id}.\n\nSection 5 of document {id}."
        )
    }

    fn updated(id: usize) -> String {
        format!(
            "Section 1 of document {id}.\n\nSection 2 of document {id} REVISED.\n\nSection 3 of document {id}.\n\nSection 4 of document {id}.\n\nSection 5 of document {id}."
        )
    }

    fn engine() -> IncrementalEngine {
        IncrementalEngine::new(Arc::new(MockEmbeddingProvider::default()))
    }

    #[test]
    fn ingest_indexes_all_chunks() {
        let mut e = engine();
        let report = e.ingest(1, &content(1));
        assert_eq!(report.chunks, 5);
        assert_eq!(e.store().len(), 5);
        assert_eq!(e.chunks().get(1).unwrap().len(), 5);
        let state = e.lifecycle().get(1).unwrap();
        assert_eq!(state.version, 1);
        assert_eq!(state.state, LifecycleState::Active);
    }

    #[test]
    fn update_unchanged_content_does_no_work() {
        let mut e = engine();
        e.ingest(1, &content(1));
        let report = e.update(1, &content(1)).unwrap();
        assert_eq!(report.changed_chunks, 0);
        assert_eq!(report.embeddings_generated, 0);
        assert_eq!(report.embeddings_skipped, 5);
        assert!(report.work_avoided_pct > 99.9);
    }

    #[test]
    fn update_changed_content_regenerates_only_changed_chunk() {
        let mut e = engine();
        e.ingest(1, &content(1));
        let report = e.update(1, &updated(1)).unwrap();
        assert_eq!(report.total_chunks, 5);
        assert_eq!(report.changed_chunks, 1);
        assert_eq!(report.embeddings_generated, 1);
        assert_eq!(report.embeddings_skipped, 4);
        assert!((report.work_avoided_pct - 80.0).abs() < 1e-9);
        assert_eq!(e.store().get(1, 0).unwrap().source_version, 1);
        assert_eq!(e.store().get(1, 1).unwrap().source_version, 2);
        assert_eq!(e.store().get(1, 2).unwrap().source_version, 1);
    }

    #[test]
    fn update_removes_embeddings_for_removed_chunks() {
        let mut e = engine();
        e.ingest(1, &content(1));
        let shrunk = format!("Section 1 of document 1.\n\nSection 2 of document 1.");
        let report = e.update(1, &shrunk).unwrap();
        assert_eq!(report.embeddings_generated, 0);
        assert_eq!(report.embeddings_removed, 3);
        assert_eq!(e.store().len(), 2);
    }

    #[test]
    fn update_untracked_document_errors() {
        let mut e = engine();
        assert!(e.update(99, "hello").is_err());
    }

    #[test]
    fn multi_document_update_only_reprocesses_changed_chunks() {
        let mut e = engine();
        let docs = 1000usize;
        let changed = 100usize;
        for i in 0..docs {
            e.ingest(i as i64, &content(i));
        }
        assert_eq!(e.store().len(), docs * 5);

        let mut generated = 0usize;
        for i in 0..changed {
            let report = e.update(i as i64, &updated(i)).unwrap();
            assert_eq!(report.embeddings_generated, 1);
            generated += report.embeddings_generated;
        }
        assert_eq!(generated, changed);
        assert_eq!(e.store().len(), docs * 5);

        let avoided = (1.0 - generated as f64 / (docs * 5) as f64) * 100.0;
        assert!((avoided - 98.0).abs() < 1e-9);
    }

    #[test]
    fn delete_removes_all_derived_data() {
        let mut e = engine();
        e.ingest(1, &content(1));
        assert_eq!(e.store().len(), 5);

        let report = e.delete(1).unwrap();
        assert_eq!(report.affected_chunks, 5);
        assert_eq!(report.affected_vectors, 5);
        assert_eq!(report.vectors_remaining, 0);
        assert_eq!(report.cleanup_work, 5);
        assert_eq!(e.store().len(), 0);
        assert!(e.lifecycle().get(1).is_none());
        assert!(e.chunks().get(1).is_none());
    }

    #[test]
    fn delete_only_affects_target_document() {
        let mut e = engine();
        for i in 0..3 {
            e.ingest(i as i64, &content(i));
        }
        assert_eq!(e.store().len(), 15);

        let report = e.delete(1).unwrap();
        assert_eq!(report.affected_vectors, 5);
        assert_eq!(report.vectors_remaining, 10);
        assert_eq!(e.store().len(), 10);
        assert!(e.store().get(1, 0).is_none());
        assert!(e.store().get(0, 0).is_some());
        assert!(e.store().get(2, 0).is_some());
        assert!(e.chunks().get(1).is_none());
        assert!(e.lifecycle().get(1).is_none());
    }

    #[test]
    fn delete_untracked_document_errors() {
        let mut e = engine();
        assert!(e.delete(99).is_err());
    }

    #[test]
    fn delete_then_reingest_is_clean() {
        let mut e = engine();
        e.ingest(1, &content(1));
        e.delete(1).unwrap();
        assert_eq!(e.store().len(), 0);

        let report = e.ingest(1, &content(1));
        assert_eq!(report.chunks, 5);
        assert_eq!(e.store().len(), 5);
        assert_eq!(e.lifecycle().get(1).unwrap().version, 1);
        assert_eq!(e.lifecycle().get(1).unwrap().state, LifecycleState::Active);
    }

    #[test]
    fn migrate_all_embeddings_to_new_model() {
        let mut e = engine();
        e.ingest(1, &content(1));

        let v2 = MockEmbeddingProvider::new("model-v2", 128);
        let report = e.migrate_model(Arc::new(v2));

        assert_eq!(report.old_model, "model-v1");
        assert_eq!(report.new_model, "model-v2");
        assert_eq!(report.total_vectors, 5);
        assert_eq!(report.candidates, 5);
        assert_eq!(report.migrated, 5);
        assert_eq!(report.skipped, 0);

        for chunk_id in 0..5 {
            let embedding = e.store().get(1, chunk_id).unwrap();
            assert_eq!(embedding.model_id, "model-v2");
            assert_eq!(embedding.source_version, 1);
        }
    }

    #[test]
    fn migrate_skips_embeddings_already_on_target_model() {
        let mut e = engine();
        e.ingest(1, &content(1));

        let v2 = MockEmbeddingProvider::new("model-v2", 128);
        let chunk0 = e.chunks().get(1).unwrap()[0].clone();
        e.store_mut().insert(Embedding {
            document_id: 1,
            chunk_id: 0,
            source_version: chunk0.source_version,
            model_id: "model-v2".to_string(),
            vector: v2.embed(&chunk0.content),
        });

        let report = e.migrate_model(Arc::new(v2));
        assert_eq!(report.candidates, 4);
        assert_eq!(report.migrated, 4);
        assert_eq!(report.skipped, 1);
        assert_eq!(e.store().get(1, 0).unwrap().model_id, "model-v2");
        assert_eq!(e.store().get(1, 1).unwrap().model_id, "model-v2");
    }

    #[test]
    fn migrate_again_is_noop() {
        let mut e = engine();
        e.ingest(1, &content(1));
        e.migrate_model(Arc::new(MockEmbeddingProvider::new("model-v2", 128)));

        let report = e.migrate_model(Arc::new(MockEmbeddingProvider::new("model-v2", 128)));
        assert_eq!(report.candidates, 0);
        assert_eq!(report.migrated, 0);
        assert_eq!(report.skipped, 5);
    }

    #[test]
    fn migration_preserves_source_version_after_update() {
        let mut e = engine();
        e.ingest(1, &content(1));
        e.update(1, &updated(1)).unwrap();

        let v2 = MockEmbeddingProvider::new("model-v2", 128);
        e.migrate_model(Arc::new(v2));

        let embedding = e.store().get(1, 1).unwrap();
        assert_eq!(embedding.model_id, "model-v2");
        assert_eq!(
            embedding.source_version, 2,
            "source version must survive model migration"
        );
    }

    #[test]
    fn engine_roundtrip_persistence() {
        let mut e = engine();
        e.ingest(1, &content(1));
        e.update(1, &updated(1)).unwrap();
        e.ingest(2, &content(2));

        let bytes = e.to_bytes();
        let loaded = IncrementalEngine::from_bytes(&bytes).unwrap();
        assert_eq!(loaded.store().len(), e.store().len());
        assert_eq!(loaded.lifecycle().get(1).unwrap().version, 2);
        assert_eq!(loaded.chunks().get(1).unwrap().len(), 5);
        assert_eq!(loaded.chunks().get(2).unwrap().len(), 5);
    }

    #[test]
    fn add_after_load_is_incremental() {
        let mut e = engine();
        e.ingest(1, &content(1));
        let mut loaded = IncrementalEngine::from_bytes(&e.to_bytes()).unwrap();

        let before = loaded.store().len();
        let report = loaded.ingest(2, &content(2));
        assert_eq!(report.chunks, 5);
        assert_eq!(
            loaded.store().len(),
            before + 5,
            "only the new document is processed"
        );
    }

    #[test]
    fn delete_after_load_is_incremental() {
        let mut e = engine();
        e.ingest(1, &content(1));
        e.ingest(2, &content(2));
        let mut loaded = IncrementalEngine::from_bytes(&e.to_bytes()).unwrap();

        let before = loaded.store().len();
        let report = loaded.delete(1).unwrap();
        assert_eq!(report.affected_vectors, 5);
        assert_eq!(loaded.store().len(), before - 5);
        assert!(
            loaded.store().get(2, 0).is_some(),
            "unrelated document untouched"
        );
    }

    #[test]
    fn from_bytes_rejects_bad_magic() {
        assert!(IncrementalEngine::from_bytes(b"XXXX").is_none());
    }
}
