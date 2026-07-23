use std::collections::HashMap;
use std::path::PathBuf;

use crate::exec::batch::RecordBatch;
use crate::schema::Schema;
use crate::storage::format::{BlobReader, BlobWriter};

pub struct TableManager {
    base_path: PathBuf,
    schema_cache: HashMap<String, Schema>,
    version_counter: HashMap<String, u64>,
}

impl TableManager {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            schema_cache: HashMap::new(),
            version_counter: HashMap::new(),
        }
    }

    pub fn blob_path(&self, name: &str) -> PathBuf {
        let mut path = self.base_path.clone();
        path.push(format!("{}.tuck", name));
        path
    }

    pub fn write_table(&mut self, name: &str, batches: &[RecordBatch]) {
        let blob = BlobWriter::write(batches);
        let path = self.blob_path(name);
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        std::fs::write(&path, blob).expect("Failed to write blob file");
        if !batches.is_empty() {
            self.schema_cache
                .insert(name.to_string(), batches[0].schema.clone());
        }
        let v = self.version_counter.entry(name.to_string()).or_insert(0);
        *v += 1;
    }

    pub fn read_table(&self, name: &str) -> Vec<RecordBatch> {
        let path = self.blob_path(name);
        if !path.exists() {
            return Vec::new();
        }
        let bytes = std::fs::read(&path).expect("Failed to read blob file");
        BlobReader::read_all(&bytes)
    }

    pub fn get_schema(&self, name: &str) -> Option<&Schema> {
        self.schema_cache.get(name)
    }

    pub fn data_version(&self, name: &str) -> u64 {
        *self.version_counter.get(name).unwrap_or(&0)
    }
}
