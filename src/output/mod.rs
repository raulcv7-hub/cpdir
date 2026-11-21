//! Strict XML serialization and system clipboard output adapters.

pub mod clipboard;
pub mod git_xml;
pub mod xml_writer;

pub use clipboard::{ClipboardError, ClipboardWriter, SystemClipboard, copy_to_clipboard};
pub use git_xml::{GitCommitEntry, GitContextSnapshot, GitHotspotEntry, GitTagEntry};
pub use xml_writer::{XmlWriteError, render_strict_xml};
