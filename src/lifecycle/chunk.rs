use std::collections::HashMap;

use super::state::content_hash;

/// Default maximum chunk length in bytes for deterministic chunking.
pub const DEFAULT_CHUNK_SIZE: usize = 512;

/// A chunk of a source document at a specific source version.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub document_id: i64,
    pub source_version: u64,
    pub chunk_id: u32,
    pub content_hash: u64,
    pub content: String,
}

/// Whether a chunk requires downstream processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkChange {
    /// Chunk is byte-identical to the previous source version.
    Skip,
    /// Chunk changed (or is new) and needs reprocessing.
    Process,
}

/// Decision for a single chunk in a document diff.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkDiff {
    pub chunk_id: u32,
    pub change: ChunkChange,
    pub previous_hash: u64,
    pub new_hash: u64,
}

/// Summary of a chunk lifecycle diff between two document versions.
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkDiffReport {
    pub document_id: i64,
    pub diffs: Vec<ChunkDiff>,
    pub total_chunks: usize,
    pub changed_chunks: usize,
    pub unchanged_chunks: usize,
    pub reprocessed_chunks: usize,
    pub processing_avoided_pct: f64,
}

/// Deterministic chunking: split text into blank-line separated paragraphs;
/// paragraphs longer than `max_chunk_len` are further split at whitespace.
pub fn chunk_text(text: &str, max_chunk_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    for paragraph in split_paragraphs(text) {
        if paragraph.len() <= max_chunk_len {
            chunks.push(paragraph.to_string());
        } else {
            chunks.extend(split_fixed(paragraph, max_chunk_len));
        }
    }
    chunks
}

/// Split a document into non-empty paragraphs at blank lines.
fn split_paragraphs(text: &str) -> Vec<&str> {
    let mut paragraphs = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut j = i;
            while j < bytes.len() && bytes[j] == b'\n' {
                j += 1;
            }
            if j - i >= 2 {
                let para = text[start..i].trim();
                if !para.is_empty() {
                    paragraphs.push(para);
                }
                i = j;
                start = i;
            } else {
                i = j;
            }
        } else {
            i += 1;
        }
    }
    let tail = text[start..].trim();
    if !tail.is_empty() {
        paragraphs.push(tail);
    }
    paragraphs
}

/// Split text into chunks of at most `max_chunk_len` bytes without
/// splitting words when a whitespace boundary is available.
fn split_fixed(text: &str, max_chunk_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0usize;
    while start < bytes.len() {
        let remaining = bytes.len() - start;
        if remaining <= max_chunk_len {
            chunks.push(text[start..].to_string());
            break;
        }
        let hard_end = text.floor_char_boundary(start + max_chunk_len);
        let mut cut = hard_end;
        while cut > start && !bytes[cut].is_ascii_whitespace() {
            cut -= 1;
        }
        let end = if cut == start { hard_end } else { cut };
        chunks.push(text[start..end].to_string());
        start = end;
        while start < bytes.len() && bytes[start].is_ascii_whitespace() {
            start += 1;
        }
    }
    chunks
}

/// Chunk a document at the given source version.
pub fn chunk_document(document_id: i64, source_version: u64, content: &str) -> Vec<Chunk> {
    chunk_text(content, DEFAULT_CHUNK_SIZE)
        .into_iter()
        .enumerate()
        .map(|(i, text)| Chunk {
            document_id,
            source_version,
            chunk_id: i as u32,
            content_hash: content_hash(&[&text]),
            content: text,
        })
        .collect()
}

/// Tracks the chunked representation of documents across source versions.
pub struct ChunkTracker {
    documents: HashMap<i64, (u64, Vec<Chunk>)>,
}

impl ChunkTracker {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    /// Register a new document version and return its chunks.
    pub fn register(&mut self, document_id: i64, source_version: u64, content: &str) -> Vec<Chunk> {
        let chunks = chunk_document(document_id, source_version, content);
        self.documents
            .insert(document_id, (source_version, chunks.clone()));
        chunks
    }

    /// Re-chunk an updated document and report which chunks changed.
    pub fn apply_update(
        &mut self,
        document_id: i64,
        new_source_version: u64,
        new_content: &str,
    ) -> Result<ChunkDiffReport, String> {
        let (_, old_chunks) = self
            .documents
            .get(&document_id)
            .ok_or_else(|| format!("document {} is not tracked", document_id))?;

        let new_chunks = chunk_document(document_id, new_source_version, new_content);
        let old_by_id: HashMap<u32, &Chunk> = old_chunks.iter().map(|c| (c.chunk_id, c)).collect();

        let mut diffs = Vec::new();
        let mut changed = 0usize;
        for chunk in &new_chunks {
            let change = match old_by_id.get(&chunk.chunk_id) {
                Some(old) if old.content_hash == chunk.content_hash => ChunkChange::Skip,
                _ => {
                    changed += 1;
                    ChunkChange::Process
                }
            };
            let previous_hash = old_by_id
                .get(&chunk.chunk_id)
                .map(|c| c.content_hash)
                .unwrap_or(0);
            diffs.push(ChunkDiff {
                chunk_id: chunk.chunk_id,
                change,
                previous_hash,
                new_hash: chunk.content_hash,
            });
        }

        let total = new_chunks.len();
        let report = ChunkDiffReport {
            document_id,
            diffs,
            total_chunks: total,
            changed_chunks: changed,
            unchanged_chunks: total - changed,
            reprocessed_chunks: changed,
            processing_avoided_pct: if total == 0 {
                0.0
            } else {
                (1.0 - changed as f64 / total as f64) * 100.0
            },
        };

        self.documents
            .insert(document_id, (new_source_version, new_chunks));
        Ok(report)
    }

    pub fn get(&self, document_id: i64) -> Option<&[Chunk]> {
        self.documents
            .get(&document_id)
            .map(|(_, chunks)| chunks.as_slice())
    }

    pub fn iter(&self) -> impl Iterator<Item = (i64, u64, &[Chunk])> {
        self.documents
            .iter()
            .map(|(id, (version, chunks))| (*id, *version, chunks.as_slice()))
    }

    pub fn from_entries(entries: Vec<(i64, u64, Vec<Chunk>)>) -> Self {
        Self {
            documents: entries.into_iter().map(|(id, v, c)| (id, (v, c))).collect(),
        }
    }

    /// Remove a document and its chunk state, returning the chunks.
    pub fn remove(&mut self, document_id: i64) -> Option<Vec<Chunk>> {
        self.documents
            .remove(&document_id)
            .map(|(_, chunks)| chunks)
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

impl Default for ChunkTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers resources from another object.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";

    const V2: &str = "Q1: What is RAII?\nA1: RAII binds resource lifetime to object lifetime.\n\nQ2: What is a move constructor?\nA2: A move constructor transfers ownership by moving resources instead of copying them.\n\nQ3: What is a virtual function?\nA3: A virtual function enables runtime polymorphism.";

    #[test]
    fn chunk_text_splits_paragraphs() {
        let chunks = chunk_text(V1, DEFAULT_CHUNK_SIZE);
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].starts_with("Q1:"));
        assert!(chunks[1].starts_with("Q2:"));
        assert!(chunks[2].starts_with("Q3:"));
    }

    #[test]
    fn chunk_document_has_sequential_ids_and_hashes() {
        let chunks = chunk_document(1, 1, V1);
        assert_eq!(chunks.len(), 3);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_id, i as u32);
            assert_eq!(chunk.source_version, 1);
            assert_eq!(chunk.document_id, 1);
            assert_ne!(chunk.content_hash, 0);
        }
    }

    #[test]
    fn unchanged_chunks_share_hashes() {
        let v1 = chunk_document(1, 1, V1);
        let v2 = chunk_document(1, 2, V1);
        for (a, b) in v1.iter().zip(v2.iter()) {
            assert_eq!(a.content_hash, b.content_hash);
        }
    }

    #[test]
    fn update_with_unchanged_content_skips_all() {
        let mut tracker = ChunkTracker::new();
        tracker.register(1, 1, V1);
        let report = tracker.apply_update(1, 2, V1).unwrap();
        assert_eq!(report.total_chunks, 3);
        assert_eq!(report.changed_chunks, 0);
        assert_eq!(report.reprocessed_chunks, 0);
        assert!(report.processing_avoided_pct > 99.9);
        assert!(report.diffs.iter().all(|d| d.change == ChunkChange::Skip));
    }

    #[test]
    fn update_changing_one_section_reprocesses_only_it() {
        let mut tracker = ChunkTracker::new();
        tracker.register(1, 1, V1);
        let report = tracker.apply_update(1, 2, V2).unwrap();
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
    }

    #[test]
    fn update_untracked_document_errors() {
        let mut tracker = ChunkTracker::new();
        assert!(tracker.apply_update(99, 2, V1).is_err());
    }
}
