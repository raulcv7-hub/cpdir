//! Filesystem reader, pruning, ignore-rule precedence, and token counting test suite.

#[path = "../common/mod.rs"]
mod common;

mod file_reader;
mod file_reader_props;
mod ignore_deep;
mod ignore_rules;
mod token_batch_and_cache;
mod token_counting;
