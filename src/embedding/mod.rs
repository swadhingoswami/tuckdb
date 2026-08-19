pub mod dedup;
pub mod provider;
pub mod store;

pub use dedup::{SemanticDedup, SemanticDedupReport, SemanticDupMatch};
pub use provider::{DEFAULT_EMBEDDING_DIM, EmbeddingProvider, MockEmbeddingProvider};
pub use store::{Embedding, EmbeddingStore, VectorMatch, cosine};
