use std::collections::HashMap;

use super::state::content_hash;

/// Result of inserting content into a dedup registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupOutcome {
    /// First time this exact content is seen.
    NewUnique,
    /// Exact duplicate of previously seen content.
    Duplicate,
}

/// Summary of deduplication over a set of inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct DedupReport {
    pub total_inputs: usize,
    pub unique_contents: usize,
    pub duplicates_detected: usize,
    pub duplicate_ratio: f64,
    pub processing_avoided: usize,
}

/// Exact-content deduplication registry keyed by content hash.
pub struct ContentDedup {
    canonical: HashMap<u64, Vec<String>>,
    counts: HashMap<u64, usize>,
    total: usize,
}

impl ContentDedup {
    pub fn new() -> Self {
        Self {
            canonical: HashMap::new(),
            counts: HashMap::new(),
            total: 0,
        }
    }

    /// Register content. Returns Duplicate when an exact copy was seen before.
    pub fn insert(&mut self, content: &[&str]) -> DedupOutcome {
        let hash = content_hash(content);
        let count = self.counts.entry(hash).or_insert(0);
        let outcome = if *count == 0 {
            DedupOutcome::NewUnique
        } else {
            DedupOutcome::Duplicate
        };
        *count += 1;
        self.canonical
            .entry(hash)
            .or_insert_with(|| content.iter().map(|s| s.to_string()).collect());
        self.total += 1;
        outcome
    }

    /// Canonical (first-seen) content for the given content, if registered.
    pub fn lookup(&self, content: &[&str]) -> Option<&[String]> {
        let hash = content_hash(content);
        self.canonical.get(&hash).map(|v| v.as_slice())
    }

    pub fn total(&self) -> usize {
        self.total
    }

    pub fn unique_count(&self) -> usize {
        self.canonical.len()
    }

    pub fn duplicates_detected(&self) -> usize {
        self.total - self.canonical.len()
    }

    pub fn duplicate_ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.duplicates_detected() as f64 / self.total as f64
        }
    }

    pub fn report(&self) -> DedupReport {
        let duplicates = self.duplicates_detected();
        DedupReport {
            total_inputs: self.total,
            unique_contents: self.canonical.len(),
            duplicates_detected: duplicates,
            duplicate_ratio: self.duplicate_ratio(),
            processing_avoided: duplicates,
        }
    }
}

impl Default for ContentDedup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_insert_is_unique() {
        let mut dedup = ContentDedup::new();
        assert_eq!(dedup.insert(&["What is RAII?"]), DedupOutcome::NewUnique);
    }

    #[test]
    fn exact_duplicate_is_detected_by_hash() {
        let mut dedup = ContentDedup::new();
        dedup.insert(&["What is RAII?"]);
        assert_eq!(dedup.insert(&["What is RAII?"]), DedupOutcome::Duplicate);
        assert_eq!(
            dedup.insert(&["What is a move constructor?"]),
            DedupOutcome::NewUnique
        );
    }

    #[test]
    fn lookup_returns_canonical_content() {
        let mut dedup = ContentDedup::new();
        dedup.insert(&["a", "b", "c"]);
        let canonical = dedup.lookup(&["a", "b", "c"]).unwrap();
        assert_eq!(canonical, &["a", "b", "c"]);
        assert!(dedup.lookup(&["x", "y", "z"]).is_none());
    }

    #[test]
    fn report_counts_are_correct() {
        let mut dedup = ContentDedup::new();
        for i in 0..9000 {
            dedup.insert(&[&format!("unique_content_{}", i)]);
        }
        for i in 0..1000 {
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
    fn empty_registry_reports_zero() {
        let dedup = ContentDedup::new();
        let report = dedup.report();
        assert_eq!(report.total_inputs, 0);
        assert_eq!(report.unique_contents, 0);
        assert_eq!(report.duplicates_detected, 0);
        assert_eq!(report.duplicate_ratio, 0.0);
    }
}
