//! Per-directory persistent user selection memory.

pub mod memory_store;
pub mod schema;

pub use memory_store::{MemoryStore, normalize_dir_for_hash};
pub use schema::{STATE_SCHEMA_VERSION, SavedWorkspaceState, StateError};
