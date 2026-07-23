use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::backend::traits::BlobBackend;

pub struct LocalFileSystem;

impl BlobBackend for LocalFileSystem {
    fn read(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, String> {
        let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    }

    fn write(&self, path: &str, data: &[u8]) -> Result<(), String> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(path, data).map_err(|e| e.to_string())
    }

    fn exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, String> {
        let p = Path::new(prefix);
        let dir = if prefix.is_empty() {
            Path::new(".")
        } else if p.is_dir() {
            p
        } else {
            p.parent().unwrap_or(Path::new("."))
        };
        let mut result = Vec::new();
        if dir.exists() {
            for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if path.is_file()
                    && let Some(s) = path.to_str()
                    && s.starts_with(prefix)
                {
                    result.push(s.to_string());
                }
            }
        }
        result.sort();
        Ok(result)
    }
}
