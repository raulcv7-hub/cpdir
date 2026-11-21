//! Per-directory persistent user selection memory.

pub mod memory_store;

pub use memory_store::{
    MemoryStore, STATE_SCHEMA_VERSION, SavedWorkspaceState, StateError, normalize_dir_for_hash,
};
