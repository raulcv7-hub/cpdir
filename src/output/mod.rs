//! Strict XML serialization and system clipboard output adapters.

pub mod xml_writer;

pub use xml_writer::{
    GitCommitEntry, GitContextSnapshot, GitHotspotEntry, XmlWriteError, render_strict_xml,
};
