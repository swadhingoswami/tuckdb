use std::collections::HashMap;

use crate::exec::batch::RecordBatch;

#[derive(Hash, Eq, PartialEq, Clone)]
pub struct QueryKey {
    pub query_hash: u64,
    pub data_version: u64,
}

pub struct ResultCache {
    max_entries: usize,
    entries: HashMap<QueryKey, Vec<RecordBatch>>,
}

impl ResultCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            entries: HashMap::new(),
        }
    }

    pub fn get(&self, key: &QueryKey) -> Option<&Vec<RecordBatch>> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: QueryKey, results: Vec<RecordBatch>) {
        if self.entries.len() >= self.max_entries && !self.entries.contains_key(&key) {
            // evict oldest entry
            if let Some(oldest) = self.entries.keys().next().cloned() {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key, results);
    }

    pub fn invalidate(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
