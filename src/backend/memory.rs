use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::backend::traits::BlobBackend;

pub struct InMemoryBackend {
    data: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl BlobBackend for InMemoryBackend {
    fn read(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, String> {
        let map = self.data.lock().map_err(|e| e.to_string())?;
        let bytes = map
            .get(path)
            .ok_or_else(|| format!("not found: {}", path))?;
        let start = offset as usize;
        let end = start + len as usize;
        if end > bytes.len() {
            return Err(format!(
                "read out of bounds: offset={} len={} file_size={}",
                offset,
                len,
                bytes.len()
            ));
        }
        Ok(bytes[start..end].to_vec())
    }

    fn write(&self, path: &str, data: &[u8]) -> Result<(), String> {
        let mut map = self.data.lock().map_err(|e| e.to_string())?;
        map.insert(path.to_string(), data.to_vec());
        Ok(())
    }

    fn exists(&self, path: &str) -> bool {
        self.data
            .lock()
            .map(|map| map.contains_key(path))
            .unwrap_or(false)
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, String> {
        let map = self.data.lock().map_err(|e| e.to_string())?;
        let mut result: Vec<String> = map
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        result.sort();
        Ok(result)
    }
}
