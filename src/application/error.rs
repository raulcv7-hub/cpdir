//! Outcome and error types emitted by the interactive application pipeline.

use std::fmt;

use crate::output::{ClipboardError, XmlWriteError};
use crate::scanner::{ContentHydrationError, ScanError};
use crate::tokens::TokenError;

/// Successful interactive run result returned to `main`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// Strict XML was copied to the OS clipboard.
    Copied {
        /// Number of selected files included in `<files>`.
        files: usize,
        /// Total selected BPE tokens across included files and active git context.
        tokens: usize,
        /// Final UTF-8 XML payload size in bytes.
        xml_bytes: usize,
    },
    /// User aborted the interactive session without modifying the clipboard.
    Cancelled,
}

/// Errors emitted across the interactive `cpdir` execution pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// Standard input or standard error is not attached to an interactive terminal.
    NotATerminal,
    /// Directory scan failure.
    Scan(ScanError),
    /// Offline tokenizer initialization failure.
    Tokens(TokenError),
    /// Selected file could not be reloaded from disk prior to XML emission.
    ContentHydration(ContentHydrationError),
    /// Terminal I/O or raw-mode failure.
    TerminalIo(String),
    /// Strict XML serialization error.
    Xml(XmlWriteError),
    /// OS clipboard error.
    Clipboard(ClipboardError),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotATerminal => write!(f, "cpdir requires an interactive terminal"),
            Self::Scan(err) => write!(f, "{err}"),
            Self::Tokens(err) => write!(f, "{err}"),
            Self::ContentHydration(err) => write!(f, "{err}"),
            Self::TerminalIo(reason) => write!(f, "terminal error: {reason}"),
            Self::Xml(err) => write!(f, "{err}"),
            Self::Clipboard(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AppError {}
