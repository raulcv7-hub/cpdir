//! Pure command-line parsing and Windows Explorer path quirk normalization.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

/// Validated command parsed from CLI arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    /// Launch `cpdir` targeting `target_dir`.
    Run {
        /// Target workspace directory to inspect and copy.
        target_dir: PathBuf,
    },
    /// Display concise usage instructions.
    Help,
    /// Display package version.
    Version,
}

/// Errors returned while parsing CLI arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseError {
    /// An empty path string was passed explicitly.
    EmptyPath,
    /// An unrecognized flag was supplied.
    UnknownFlag(String),
    /// More than one positional target directory was passed.
    UnexpectedArgument(String),
}

impl fmt::Display for CliParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath => write!(f, "target directory path cannot be empty"),
            Self::UnknownFlag(flag) => write!(f, "unknown option: {flag}"),
            Self::UnexpectedArgument(arg) => write!(f, "unexpected extra argument: {arg}"),
        }
    }
}

impl std::error::Error for CliParseError {}

/// Parses raw process arguments (skipping `argv[0]`) into a validated [`CliCommand`].
///
/// # Errors
///
/// Returns [`CliParseError`] if an unknown option, empty path, or extra positional argument is
/// provided.
pub fn parse_cli_args<I, S>(args: I) -> Result<CliCommand, CliParseError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut iter = args.into_iter();
    let _program = iter.next();
    let mut target_dir: Option<PathBuf> = None;
    let mut end_of_flags = false;

    for raw in iter {
        let os = raw.into();
        if !end_of_flags && let Some(s) = os.to_str() {
            match s {
                "-h" | "--help" => return Ok(CliCommand::Help),
                "-V" | "--version" => return Ok(CliCommand::Version),
                "--" => {
                    end_of_flags = true;
                    continue;
                }
                _ if s.starts_with('-') => return Err(CliParseError::UnknownFlag(s.to_owned())),
                _ => {}
            }
        }

        if target_dir.is_some() {
            return Err(CliParseError::UnexpectedArgument(
                os.to_string_lossy().into_owned(),
            ));
        }

        let normalized = match os.to_str() {
            Some(s) => normalize_explorer_path(s)?,
            None => PathBuf::from(os),
        };
        target_dir = Some(normalized);
    }

    Ok(CliCommand::Run {
        target_dir: target_dir.unwrap_or_else(|| PathBuf::from(".")),
    })
}

/// Normalizes directory paths passed from Windows Explorer context-menu verbs (`"%1"` / `"%V"`).
///
/// When Windows Explorer expands `"%V"` on a drive root (`C:\`), trailing `\"` turns into a
/// literal `"` (`C:"`). This strips the stray quote and restores the root backslash (`C:\`).
///
/// # Errors
///
/// Returns [`CliParseError::EmptyPath`] if the normalized path is empty.
pub fn normalize_explorer_path(raw: &str) -> Result<PathBuf, CliParseError> {
    let mut had_stray_trailing_quote = false;
    let mut cleaned = raw;

    if cleaned.len() >= 2 && cleaned.starts_with('"') && cleaned.ends_with('"') {
        cleaned = &cleaned[1..cleaned.len() - 1];
    } else if let Some(stripped) = cleaned.strip_suffix('"') {
        cleaned = stripped;
        had_stray_trailing_quote = true;
    }

    if cleaned.is_empty() {
        return Err(CliParseError::EmptyPath);
    }

    if had_stray_trailing_quote && is_bare_windows_drive(cleaned) {
        return Ok(PathBuf::from(format!("{cleaned}\\")));
    }

    Ok(PathBuf::from(cleaned))
}

/// Returns concise CLI help text.
#[must_use]
pub const fn usage_text() -> &'static str {
    concat!(
        "cpdir - copy directory XML context to the Windows clipboard\n\n",
        "USAGE:\n",
        "  cpdir [DIRECTORY]\n",
        "  cpdir [-h | --help]\n",
        "  cpdir [-V | --version]\n",
    )
}

const fn is_bare_windows_drive(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
