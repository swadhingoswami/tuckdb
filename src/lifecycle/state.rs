use std::collections::HashMap;

/// Lifecycle state of a document's derived representations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    /// Derived representations are up-to-date with source content.
    Active,
    /// Source content changed; derived representations need reprocessing.
    Stale,
}

/// Per-document lifecycle metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceState {
    pub document_id: i64,
    pub version: u64,
    pub content_hash: u64,
    pub state: LifecycleState,
}

impl SourceState {
    pub fn new_active(document_id: i64, content_hash: u64) -> Self {
        Self {
            document_id,
            version: 1,
            content_hash,
            state: LifecycleState::Active,
        }
    }
}

/// FNV-1a 64-bit hash. Deterministic and dependency-free.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Hash a document's content (one or more text parts).
pub fn content_hash(parts: &[&str]) -> u64 {
    let mut buf = Vec::new();
    for part in parts {
        buf.extend_from_slice(part.as_bytes());
        buf.push(0x1f);
    }
    fnv1a(&buf)
}

/// Result of applying an update to a tracked document.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeReport {
    pub document_id: i64,
    pub previous_version: u64,
    pub new_version: u64,
    pub previous_hash: u64,
    pub new_hash: u64,
    pub content_changed: bool,
    pub derived_state: LifecycleState,
}

/// Tracks lifecycle metadata for documents by source ID.
pub struct LifecycleManager {
    documents: HashMap<i64, SourceState>,
}

impl LifecycleManager {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    /// Record a new source document at version 1, state Active.
    /// If the document is already tracked, the existing state is kept.
    pub fn register(&mut self, document_id: i64, content: &[&str]) -> &SourceState {
        let hash = content_hash(content);
        self.documents
            .entry(document_id)
            .or_insert_with(|| SourceState::new_active(document_id, hash))
    }

    /// Apply an update to a tracked document.
    /// Identical content produces no version bump and no downstream work.
    /// Changed content increments the version and marks derived state stale.
    pub fn apply_update(
        &mut self,
        document_id: i64,
        content: &[&str],
    ) -> Result<ChangeReport, String> {
        let new_hash = content_hash(content);
        let state = self.documents.get_mut(&document_id).ok_or_else(|| {
            format!(
                "document {} is not tracked; register it before update",
                document_id
            )
        })?;

        let previous_version = state.version;
        let previous_hash = state.content_hash;

        let (new_version, derived_state, content_changed) = if new_hash == previous_hash {
            (previous_version, state.state, false)
        } else {
            state.version += 1;
            state.content_hash = new_hash;
            state.state = LifecycleState::Stale;
            (state.version, state.state, true)
        };

        Ok(ChangeReport {
            document_id,
            previous_version,
            new_version,
            previous_hash,
            new_hash,
            content_changed,
            derived_state,
        })
    }

    pub fn get(&self, document_id: i64) -> Option<&SourceState> {
        self.documents.get(&document_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (i64, &SourceState)> {
        self.documents.iter().map(|(id, s)| (*id, s))
    }

    pub fn from_entries(entries: Vec<(i64, SourceState)>) -> Self {
        Self {
            documents: entries.into_iter().collect(),
        }
    }

    /// Remove a document and its lifecycle metadata.
    pub fn remove(&mut self, document_id: i64) -> Option<SourceState> {
        self.documents.remove(&document_id)
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        let parts = [
            "What is RAII?",
            "RAII binds resource lifetime to object lifetime.",
            "memory",
        ];
        assert_eq!(content_hash(&parts), content_hash(&parts));
    }

    #[test]
    fn hash_differs_for_different_content() {
        let a = content_hash(&["What is RAII?"]);
        let b = content_hash(&["What is a move constructor?"]);
        assert_ne!(a, b);
    }

    #[test]
    fn register_creates_active_version_one() {
        let mut lm = LifecycleManager::new();
        lm.register(1001, &["hello", "world"]);
        let state = lm.get(1001).unwrap();
        assert_eq!(state.version, 1);
        assert_eq!(state.state, LifecycleState::Active);
        assert_ne!(state.content_hash, 0);
    }

    #[test]
    fn identical_update_does_no_work() {
        let mut lm = LifecycleManager::new();
        lm.register(1, &["a", "b", "c"]);
        let report = lm.apply_update(1, &["a", "b", "c"]).unwrap();
        assert!(!report.content_changed);
        assert_eq!(report.previous_version, 1);
        assert_eq!(report.new_version, 1);
        assert_eq!(report.derived_state, LifecycleState::Active);
    }

    #[test]
    fn changed_update_increments_version_and_marks_stale() {
        let mut lm = LifecycleManager::new();
        lm.register(1, &["a", "b", "c"]);
        let report = lm.apply_update(1, &["a", "CHANGED", "c"]).unwrap();
        assert!(report.content_changed);
        assert_eq!(report.previous_version, 1);
        assert_eq!(report.new_version, 2);
        assert_eq!(report.derived_state, LifecycleState::Stale);
        assert_ne!(report.previous_hash, report.new_hash);
        let state = lm.get(1).unwrap();
        assert_eq!(state.version, 2);
        assert_eq!(state.state, LifecycleState::Stale);
    }

    #[test]
    fn update_untracked_document_errors() {
        let mut lm = LifecycleManager::new();
        assert!(lm.apply_update(999, &["x"]).is_err());
    }
}
