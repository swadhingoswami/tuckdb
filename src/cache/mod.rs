pub mod data_cache;
pub mod result_cache;
pub mod policy;

pub use data_cache::DataCache;
pub use result_cache::{QueryKey, ResultCache};
pub use policy::EvictionPolicy;
