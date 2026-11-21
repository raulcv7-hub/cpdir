//! Win32 system clipboard adapter with transient lock-retry backoff.

use std::fmt;
use std::thread;
use std::time::Duration;

use arboard::Clipboard;

const CLIPBOARD_MAX_ATTEMPTS: usize = 5;
const CLIPBOARD_RETRY_DELAY: Duration = Duration::from_millis(15);

/// Errors emitted while initializing or writing to the operating system clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    /// Failed to acquire the system clipboard handle.
    InitFailed(String),
    /// Failed to store text payload into the system clipboard.
    WriteFailed(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitFailed(reason) => write!(f, "failed to open system clipboard: {reason}"),
            Self::WriteFailed(reason) => write!(f, "failed to write text to clipboard: {reason}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// Port decoupling clipboard writes from the concrete Win32/OS backend.
pub trait ClipboardWriter {
    /// Copies `text` into the clipboard synchronously so it persists after process exit.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] if the clipboard cannot be opened or updated.
    fn copy_text(&self, text: &str) -> Result<(), ClipboardError>;
}

/// Native OS clipboard adapter backed by `arboard` with transient lock retries.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClipboard;

impl ClipboardWriter for SystemClipboard {
    fn copy_text(&self, text: &str) -> Result<(), ClipboardError> {
        for attempt in 0..CLIPBOARD_MAX_ATTEMPTS {
            match try_copy_once(text) {
                Ok(()) => return Ok(()),
                Err(err) if attempt + 1 < CLIPBOARD_MAX_ATTEMPTS => {
                    // Windows Clipboard History / other apps may briefly lock OpenClipboard.
                    thread::sleep(CLIPBOARD_RETRY_DELAY);
                    let _ = err;
                }
                Err(err) => return Err(err),
            }
        }
        unreachable!("retry loop always returns on the final attempt")
    }
}

/// Copies `text` to the default OS clipboard using [`SystemClipboard`].
///
/// # Errors
///
/// Returns [`ClipboardError`] if all retry attempts fail.
pub fn copy_to_clipboard(text: &str) -> Result<(), ClipboardError> {
    SystemClipboard.copy_text(text)
}

fn try_copy_once(text: &str) -> Result<(), ClipboardError> {
    let mut clipboard =
        Clipboard::new().map_err(|err| ClipboardError::InitFailed(err.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|err| ClipboardError::WriteFailed(err.to_string()))
}
