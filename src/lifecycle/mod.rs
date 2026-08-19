pub mod chunk;
pub mod dedup;
pub mod state;

pub use chunk::{
    Chunk, ChunkChange, ChunkDiff, ChunkDiffReport, ChunkTracker, chunk_document, chunk_text,
};
pub use dedup::{ContentDedup, DedupOutcome, DedupReport};
pub use state::{ChangeReport, LifecycleManager, LifecycleState, SourceState, content_hash, fnv1a};
