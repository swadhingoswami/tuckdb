pub mod traits;
pub mod local_fs;
pub mod memory;
#[cfg(feature = "s3")]
pub mod s3;

pub use traits::BlobBackend;
pub use local_fs::LocalFileSystem;
pub use memory::InMemoryBackend;
#[cfg(feature = "s3")]
pub use s3::S3Backend;
