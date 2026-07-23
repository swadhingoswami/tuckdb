use std::collections::HashMap;
use std::time::Instant;

use crate::cache::policy::EvictionPolicy;
use crate::storage::chunk::EncodedChunk;

#[derive(Debug, Clone)]
struct CacheEntry {
    chunk: EncodedChunk,
    last_access: Instant,
    hit_count: u64,
}

pub struct DataCache {
    max_size: usize,
    policy: EvictionPolicy,
    entries: HashMap<(String, u32, u32), CacheEntry>,
    current_size: usize,
}

impl DataCache {
    pub fn new(max_size: usize, policy: EvictionPolicy) -> Self {
        Self {
            max_size,
            policy,
            entries: HashMap::new(),
            current_size: 0,
        }
    }

    pub fn get(&mut self, blob_id: &str, col_idx: u32, chunk_idx: u32) -> Option<EncodedChunk> {
        let key = (blob_id.to_string(), col_idx, chunk_idx);
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_access = Instant::now();
            entry.hit_count += 1;
            Some(entry.chunk.clone())
        } else {
            None
        }
    }

    pub fn insert(&mut self, blob_id: &str, col_idx: u32, chunk_idx: u32, chunk: EncodedChunk) {
        let key = (blob_id.to_string(), col_idx, chunk_idx);
        let size = chunk.meta.compressed_size as usize;

        // evict if needed
        while self.current_size + size > self.max_size && !self.entries.is_empty() {
            self.evict_one();
        }

        if !self.entries.contains_key(&key) {
            self.current_size += size;
        }

        self.entries.insert(
            key,
            CacheEntry {
                chunk,
                last_access: Instant::now(),
                hit_count: 1,
            },
        );
    }

    fn evict_one(&mut self) {
        let key = match self.policy {
            EvictionPolicy::LRU => self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_access)
                .map(|(k, _)| k.clone()),
            EvictionPolicy::LFU => self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.hit_count)
                .map(|(k, _)| k.clone()),
        };

        if let Some(k) = key
            && let Some(entry) = self.entries.remove(&k)
        {
            self.current_size -= entry.chunk.meta.compressed_size as usize;
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_size = 0;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
