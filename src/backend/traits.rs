pub trait BlobBackend: Send + Sync {
    fn read(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, String>;
    fn write(&self, path: &str, data: &[u8]) -> Result<(), String>;
    fn exists(&self, path: &str) -> bool;
    fn list(&self, prefix: &str) -> Result<Vec<String>, String>;
}
