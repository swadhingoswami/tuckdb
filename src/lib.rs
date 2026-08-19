pub mod api;
pub mod backend;
pub mod cache;
pub mod embedding;
pub mod exec;
pub mod incremental;
pub mod lifecycle;
pub mod schema;
pub mod storage;

#[cfg(feature = "capi")]
pub mod capi;

pub use api::{Query, ResultSet, Table};
pub use cache::{DataCache, EvictionPolicy, QueryKey, ResultCache};
pub use embedding::{
    Embedding, EmbeddingProvider, EmbeddingStore, MockEmbeddingProvider, SemanticDedup,
    SemanticDedupReport, SemanticDupMatch, VectorMatch,
};
pub use exec::PhysicalOperator;
pub use exec::batch::{Column, ColumnData, RecordBatch};
pub use exec::expr::{Expr, col, lit_float, lit_int, lit_str, similarity};
pub use exec::logical_plan::{AggOp, LogicalPlan};
pub use incremental::{
    DeleteReport, IncrementalEngine, IncrementalReport, IngestReport, MigrateReport,
};
pub use lifecycle::{
    ChangeReport, Chunk, ChunkChange, ChunkDiff, ChunkDiffReport, ChunkTracker, ContentDedup,
    DedupOutcome, DedupReport, LifecycleManager, LifecycleState, SourceState,
};
pub use schema::{DataType, Field, Schema};
pub use storage::encoding;
pub use storage::format::{BlobReader, BlobWriter};
