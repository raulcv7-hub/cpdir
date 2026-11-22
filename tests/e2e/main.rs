//! End-to-end CLI, clipboard, memory, session pipeline, real-project, and stress tests.

#[path = "../common/mod.rs"]
mod common;

mod cli;
mod clipboard;
mod harness_smoke;
mod pipeline_session;
mod real_projects;
mod selection_memory;
mod stress_750k;
