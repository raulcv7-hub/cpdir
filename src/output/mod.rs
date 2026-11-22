//! Strict XML serialization and system clipboard output adapters.

pub mod clipboard;
pub mod git_xml;
pub mod xml_writer;

pub use clipboard::{ClipboardError, ClipboardWriter, SystemClipboard, copy_to_clipboard};
pub use git_xml::{
    GitCommitEntry, GitContextSnapshot, GitDiffNumstatEntry, GitDiffStat, GitHotspotEntry,
    GitTagEntry, render_git_context_xml,
};
pub use xml_writer::{XmlWriteError, render_strict_xml};
