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
        /// Whether to include the normalized `root_path` attribute inside `<context_summary>`.
        include_abs_path: bool,
    },
    /// Opt-in Windows 11 Explorer `HKCU` context-menu install or `.reg` template export.
    Install {
        /// Skip interactive `[y/N]` confirmation prompt.
        yes: bool,
        /// Write a `.reg` template to this path instead of modifying the live Registry.
        export_reg: Option<PathBuf>,
    },
    /// Remove optional Windows 11 Explorer `HKCU` context-menu entries.
    Uninstall {
        /// Skip interactive `[y/N]` confirmation prompt.
        yes: bool,
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
    /// A flag requiring a value (`--export-reg`) lacked its argument.
    MissingFlagValue(String),
    /// A valid flag was passed to a command that does not accept it.
    UnsupportedFlag {
        /// Offending option string.
        flag: String,
        /// Active command context rejecting the option.
        command: &'static str,
    },
    /// More than one positional target directory or unexpected positional token was passed.
    UnexpectedArgument(String),
}

impl fmt::Display for CliParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath => write!(f, "target directory path cannot be empty"),
            Self::UnknownFlag(flag) => write!(f, "unknown option: {flag}"),
            Self::MissingFlagValue(flag) => write!(f, "option {flag} requires a path value"),
            Self::UnsupportedFlag { flag, command } => {
                write!(f, "option {flag} is not valid for '{command}'")
            }
            Self::UnexpectedArgument(arg) => write!(f, "unexpected extra argument: {arg}"),
        }
    }
}

impl std::error::Error for CliParseError {}

/// Parses raw process arguments (skipping `argv[0]`) into a validated [`CliCommand`].
///
/// # Errors
///
/// Returns [`CliParseError`] if an unknown option, empty path, missing flag value, unsupported
/// option combination, or unexpected positional argument is provided.
pub fn parse_cli_args<I, S>(args: I) -> Result<CliCommand, CliParseError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut iter = args.into_iter();
    let _program = iter.next();
    let mut target_dir: Option<PathBuf> = None;
    let mut include_abs_path = false;
    let mut end_of_flags = false;

    while let Some(raw) = iter.next() {
        let os = raw.into();
        if !end_of_flags && let Some(s) = os.to_str() {
            match s {
                "-h" | "--help" => return Ok(CliCommand::Help),
                "-V" | "--version" => return Ok(CliCommand::Version),
                "--abs-path" => {
                    include_abs_path = true;
                    continue;
                }
                "-y" | "--yes" | "--export-reg" => {
                    return Err(CliParseError::UnsupportedFlag {
                        flag: s.to_owned(),
                        command: "run",
                    });
                }
                _ if s.starts_with("--export-reg=") => {
                    return Err(CliParseError::UnsupportedFlag {
                        flag: "--export-reg".to_owned(),
                        command: "run",
                    });
                }
                "--" => {
                    end_of_flags = true;
                    continue;
                }
                _ if s.starts_with('-') => return Err(CliParseError::UnknownFlag(s.to_owned())),
                "install" if target_dir.is_none() && !include_abs_path => {
                    return parse_install_args(iter);
                }
                "uninstall" if target_dir.is_none() && !include_abs_path => {
                    return parse_uninstall_args(iter);
                }
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
        include_abs_path,
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
        "  cpdir [OPTIONS] [DIRECTORY]\n",
        "  cpdir install [--yes] [--export-reg <PATH>]\n",
        "  cpdir uninstall [--yes]\n",
        "  cpdir [-h | --help]\n",
        "  cpdir [-V | --version]\n\n",
        "OPTIONS:\n",
        "      --abs-path           Include normalized root_path in <context_summary>\n",
        "  -y, --yes                Skip confirmation prompt for install/uninstall\n",
        "      --export-reg <PATH>  Export UTF-16LE .reg file without writing Registry\n",
    )
}

fn parse_install_args<I, S>(mut iter: I) -> Result<CliCommand, CliParseError>
where
    I: Iterator<Item = S>,
    S: Into<OsString>,
{
    let mut yes = false;
    let mut export_reg: Option<PathBuf> = None;

    while let Some(raw) = iter.next() {
        let os = raw.into();
        let Some(s) = os.to_str() else {
            return Err(CliParseError::UnexpectedArgument(
                os.to_string_lossy().into_owned(),
            ));
        };
        match s {
            "-h" | "--help" => return Ok(CliCommand::Help),
            "-V" | "--version" => return Ok(CliCommand::Version),
            "-y" | "--yes" => yes = true,
            "--export-reg" => {
                let Some(val_os) = iter.next() else {
                    return Err(CliParseError::MissingFlagValue(s.to_owned()));
                };
                export_reg = Some(parse_reg_path(val_os.into())?);
            }
            _ if s.starts_with("--export-reg=") => {
                let raw_path = &s["--export-reg=".len()..];
                export_reg = Some(parse_reg_path(OsString::from(raw_path))?);
            }
            "--abs-path" => {
                return Err(CliParseError::UnsupportedFlag {
                    flag: s.to_owned(),
                    command: "install",
                });
            }
            _ if s.starts_with('-') => return Err(CliParseError::UnknownFlag(s.to_owned())),
            _ => return Err(CliParseError::UnexpectedArgument(s.to_owned())),
        }
    }

    Ok(CliCommand::Install { yes, export_reg })
}

fn parse_uninstall_args<I, S>(iter: I) -> Result<CliCommand, CliParseError>
where
    I: Iterator<Item = S>,
    S: Into<OsString>,
{
    let mut yes = false;

    for raw in iter {
        let os = raw.into();
        let Some(s) = os.to_str() else {
            return Err(CliParseError::UnexpectedArgument(
                os.to_string_lossy().into_owned(),
            ));
        };
        match s {
            "-h" | "--help" => return Ok(CliCommand::Help),
            "-V" | "--version" => return Ok(CliCommand::Version),
            "-y" | "--yes" => yes = true,
            "--abs-path" | "--export-reg" => {
                return Err(CliParseError::UnsupportedFlag {
                    flag: s.to_owned(),
                    command: "uninstall",
                });
            }
            _ if s.starts_with("--export-reg=") => {
                return Err(CliParseError::UnsupportedFlag {
                    flag: "--export-reg".to_owned(),
                    command: "uninstall",
                });
            }
            _ if s.starts_with('-') => return Err(CliParseError::UnknownFlag(s.to_owned())),
            _ => return Err(CliParseError::UnexpectedArgument(s.to_owned())),
        }
    }

    Ok(CliCommand::Uninstall { yes })
}

fn parse_reg_path(os: OsString) -> Result<PathBuf, CliParseError> {
    if let Some(s) = os.to_str() {
        if s.is_empty() {
            return Err(CliParseError::EmptyPath);
        }
        if s.starts_with('-') {
            return Err(CliParseError::MissingFlagValue("--export-reg".to_owned()));
        }
    }
    Ok(PathBuf::from(os))
}

const fn is_bare_windows_drive(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}
