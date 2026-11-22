//! Strict XML serialization and system clipboard output adapters.

pub mod clipboard;
pub mod git_xml;
mod xml_escape;
pub mod xml_writer;

pub use crate::domain::{
    GitCommitEntry, GitContextSnapshot, GitDiffNumstatEntry, GitDiffStat, GitHotspotEntry,
    GitTagEntry,
};
pub use clipboard::{ClipboardError, ClipboardWriter, SystemClipboard, copy_to_clipboard};
pub use git_xml::render_git_context_xml;
pub use xml_writer::{XmlWriteError, render_strict_xml};
