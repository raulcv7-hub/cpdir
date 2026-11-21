//! Strict XML serialization and system clipboard output adapters.

pub mod clipboard;
pub mod xml_writer;

pub use clipboard::{ClipboardError, ClipboardWriter, SystemClipboard, copy_to_clipboard};
pub use xml_writer::{
    GitCommitEntry, GitContextSnapshot, GitHotspotEntry, XmlWriteError, render_strict_xml,
};
