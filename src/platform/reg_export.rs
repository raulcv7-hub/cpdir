//! Pure Windows Registry Editor 5.00 (`.reg`) generator with strict quoting and `UTF-16LE` encoding.

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

/// Menu label shown in Windows Explorer context menus.
pub const CONTEXT_MENU_LABEL: &str = "Copy directory context (cpdir)";

/// Target binary specification used to generate Explorer shell commands and `.reg` files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenuSpec {
    exe_path: PathBuf,
}

impl ContextMenuSpec {
    /// Creates a specification targeting `exe_path`.
    #[must_use]
    pub fn new(exe_path: impl Into<PathBuf>) -> Self {
        Self {
            exe_path: exe_path.into(),
        }
    }

    /// Resolves the currently running executable path via `std::env::current_exe`.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the operating system cannot determine the executable path.
    pub fn current_exe() -> io::Result<Self> {
        Ok(Self::new(std::env::current_exe()?))
    }

    /// Returns the raw configured executable path.
    #[must_use]
    pub fn exe_path(&self) -> &Path {
        &self.exe_path
    }

    /// Returns the clean executable display string with any Windows `\\?\` verbatim prefix stripped.
    #[must_use]
    pub fn exe_display(&self) -> String {
        strip_windows_verbatim(&self.exe_path.to_string_lossy())
    }

    /// Builds a quoted Explorer shell invocation targeting `placeholder` (`%1` or `%V`).
    #[must_use]
    pub fn shell_command(&self, placeholder: &str) -> String {
        format!("\"{}\" \"{placeholder}\"", self.exe_display())
    }

    /// Builds the `Icon` registry value pointing to resource index `0` of the executable.
    #[must_use]
    pub fn icon_value(&self) -> String {
        format!("{},0", self.exe_display())
    }

    /// Returns `true` when the executable resides inside a transient Cargo `target\` build folder.
    #[must_use]
    pub fn looks_ephemeral_target_build(&self) -> bool {
        let lower = self.exe_display().to_ascii_lowercase();
        lower.contains(r"\target\debug\") || lower.contains(r"\target\release\")
    }
}

/// Renders a Windows Registry Editor Version 5.00 `.reg` payload with `CRLF` line endings.
#[must_use]
pub fn render_context_menu_reg_text(spec: &ContextMenuSpec) -> String {
    let mut out = String::from("Windows Registry Editor Version 5.00\r\n\r\n");
    push_verb_section(
        &mut out,
        r"HKEY_CURRENT_USER\Software\Classes\Directory\shell\cpdir",
        spec,
        "%1",
    );
    out.push_str("\r\n");
    push_verb_section(
        &mut out,
        r"HKEY_CURRENT_USER\Software\Classes\Directory\Background\shell\cpdir",
        spec,
        "%V",
    );
    out
}

/// Encodes [`render_context_menu_reg_text`] as `UTF-16LE` prefixed with a byte-order mark (`BOM`).
#[must_use]
pub fn render_context_menu_reg_utf16le(spec: &ContextMenuSpec) -> Vec<u8> {
    let text = render_context_menu_reg_text(spec);
    let mut bytes = Vec::with_capacity(2 + text.len() * 2);
    bytes.extend_from_slice(&[0xFF, 0xFE]);
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn push_verb_section(out: &mut String, key_path: &str, spec: &ContextMenuSpec, placeholder: &str) {
    let label = escape_reg_string(CONTEXT_MENU_LABEL);
    let icon = escape_reg_string(&spec.icon_value());
    let cmd = escape_reg_string(&spec.shell_command(placeholder));

    let _ = write!(
        out,
        "[{key_path}]\r\n@=\"{label}\"\r\n\"Icon\"=\"{icon}\"\r\n\r\n[{key_path}\\command]\r\n@=\"{cmd}\"\r\n"
    );
}

fn escape_reg_string(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len() + 8);
    for ch in raw.chars() {
        match ch {
            '\\' => escaped.push_str(r"\\"),
            '"' => escaped.push_str(r#"\""#),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn strip_windows_verbatim(raw: &str) -> String {
    raw.strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| raw.strip_prefix(r"\\?\").map(str::to_owned))
        .unwrap_or_else(|| raw.to_owned())
}
