//! Interactive and non-interactive CLI handlers for `cpdir install` and `cpdir uninstall`.

use std::fs;
use std::io::{self, BufRead, IsTerminal as _, Write};
use std::path::Path;

use crate::platform::reg_export::{ContextMenuSpec, render_context_menu_reg_utf16le};
#[cfg(windows)]
use crate::platform::win_registry::{
    RegTarget, UninstallOutcome, install_context_menu, uninstall_context_menu,
};

/// Injectable terminal streams decoupling confirmation prompts from global `stdin`/`stderr`.
pub struct ConsoleIo<'a> {
    /// Buffered input stream used to read `[y/N]` confirmation lines.
    pub input: &'a mut dyn BufRead,
    /// Output stream receiving prompts, warnings, and completion messages.
    pub output: &'a mut dyn Write,
    /// Whether `input` is attached to an interactive terminal.
    pub is_interactive: bool,
}

/// Outcome emitted by `cpdir install` subcommand execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallSubcommandOutcome {
    /// Live `HKCU` Explorer context-menu keys were written or updated.
    Installed,
    /// A `.reg` template file was exported without touching live Registry keys.
    ExportedReg,
    /// User declined installation at the confirmation prompt.
    Cancelled,
}

/// Outcome emitted by `cpdir uninstall` subcommand execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallSubcommandOutcome {
    /// Existing `HKCU` Explorer context-menu keys were deleted.
    Removed,
    /// No `cpdir` context-menu keys were present under `HKCU`.
    AlreadyAbsent,
    /// User declined uninstallation at the confirmation prompt.
    Cancelled,
}

/// Executes `cpdir install` against system `stdin`/`stderr` and `HKCU\Software\Classes`.
///
/// # Errors
///
/// Returns an `io::Error` if executable resolution, `.reg` export, confirmation I/O, or Registry
/// mutation fails, or if invoked on a non-Windows platform without `--export-reg`.
pub fn run_install_subcommand(
    yes: bool,
    export_reg: Option<&Path>,
) -> io::Result<InstallSubcommandOutcome> {
    let spec = ContextMenuSpec::current_exe()?;
    let stdin = io::stdin();
    let is_interactive = stdin.is_terminal();
    let mut input = stdin.lock();
    let mut output = io::stderr();
    let mut console = ConsoleIo {
        input: &mut input,
        output: &mut output,
        is_interactive,
    };

    if let Some(reg_path) = export_reg {
        return export_reg_template(&spec, reg_path, console.output);
    }

    #[cfg(windows)]
    {
        execute_install_with_io(&RegTarget::CurrentUser, &spec, yes, &mut console)
    }

    #[cfg(not(windows))]
    {
        let _ = (yes, &mut console);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "live HKCU context-menu registration is only supported on Windows; use --export-reg",
        ))
    }
}

/// Executes `cpdir uninstall` against system `stdin`/`stderr` and `HKCU\Software\Classes`.
///
/// # Errors
///
/// Returns an `io::Error` if confirmation I/O or Registry deletion fails, or on non-Windows OSes.
pub fn run_uninstall_subcommand(yes: bool) -> io::Result<UninstallSubcommandOutcome> {
    let stdin = io::stdin();
    let is_interactive = stdin.is_terminal();
    let mut input = stdin.lock();
    let mut output = io::stderr();
    let mut console = ConsoleIo {
        input: &mut input,
        output: &mut output,
        is_interactive,
    };

    #[cfg(windows)]
    {
        execute_uninstall_with_io(&RegTarget::CurrentUser, yes, &mut console)
    }

    #[cfg(not(windows))]
    {
        let _ = (yes, &mut console);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "HKCU context-menu uninstallation is only supported on Windows",
        ))
    }
}

/// Exports the `UTF-16LE` `.reg` context-menu template to `reg_path` without modifying `HKCU`.
///
/// # Errors
///
/// Returns an `io::Error` if writing `reg_path` or status output fails.
pub fn export_reg_template(
    spec: &ContextMenuSpec,
    reg_path: &Path,
    output: &mut dyn Write,
) -> io::Result<InstallSubcommandOutcome> {
    let bytes = render_context_menu_reg_utf16le(spec);
    fs::write(reg_path, bytes)?;
    if spec.looks_ephemeral_target_build() {
        writeln!(
            output,
            "Warning: executable points inside a Cargo target\\ build directory ({}).",
            spec.exe_display()
        )?;
    }
    writeln!(
        output,
        "Exported Windows Registry template to {} (UTF-16LE, HKCU). No live keys modified.",
        reg_path.display()
    )?;
    Ok(InstallSubcommandOutcome::ExportedReg)
}

/// Installs Explorer context-menu keys into `target` using injectable prompt I/O.
///
/// # Errors
///
/// Returns an `io::Error` if stdin is non-interactive without `yes`, or if I/O / Registry fails.
#[cfg(windows)]
pub fn execute_install_with_io(
    target: &RegTarget,
    spec: &ContextMenuSpec,
    yes: bool,
    console: &mut ConsoleIo<'_>,
) -> io::Result<InstallSubcommandOutcome> {
    writeln!(
        console.output,
        "Optional Windows 11 Explorer context menu target binary:\n  {}",
        spec.exe_display()
    )?;
    writeln!(
        console.output,
        "Registry keys (HKCU, no admin required, under 'Show more options'):\n  HKCU\\{}\n  HKCU\\{}",
        target.directory_verb_path(),
        target.background_verb_path()
    )?;
    if spec.looks_ephemeral_target_build() {
        writeln!(
            console.output,
            "Warning: executable lives in a Cargo target\\ dir; prefer an installed PATH binary."
        )?;
    }

    if !yes && !confirm_action(console, "Install Explorer context menu keys? [y/N]: ")? {
        writeln!(
            console.output,
            "Installation cancelled. Registry untouched."
        )?;
        return Ok(InstallSubcommandOutcome::Cancelled);
    }

    install_context_menu(target, spec)?;
    writeln!(
        console.output,
        "Installed Windows Explorer HKCU context menu for {}.",
        spec.exe_display()
    )?;
    Ok(InstallSubcommandOutcome::Installed)
}

/// Removes `cpdir` Explorer context-menu keys from `target` using injectable prompt I/O.
///
/// # Errors
///
/// Returns an `io::Error` if stdin is non-interactive without `yes`, or if I/O / Registry fails.
#[cfg(windows)]
pub fn execute_uninstall_with_io(
    target: &RegTarget,
    yes: bool,
    console: &mut ConsoleIo<'_>,
) -> io::Result<UninstallSubcommandOutcome> {
    writeln!(
        console.output,
        "Keys to remove from HKCU:\n  HKCU\\{}\n  HKCU\\{}",
        target.directory_verb_path(),
        target.background_verb_path()
    )?;

    if !yes && !confirm_action(console, "Remove Explorer context menu keys? [y/N]: ")? {
        writeln!(
            console.output,
            "Uninstallation cancelled. Registry untouched."
        )?;
        return Ok(UninstallSubcommandOutcome::Cancelled);
    }

    let outcome = match uninstall_context_menu(target)? {
        UninstallOutcome::Removed => {
            writeln!(
                console.output,
                "Removed Windows Explorer HKCU context menu."
            )?;
            UninstallSubcommandOutcome::Removed
        }
        UninstallOutcome::AlreadyAbsent => {
            writeln!(
                console.output,
                "No cpdir context menu keys were present under HKCU."
            )?;
            UninstallSubcommandOutcome::AlreadyAbsent
        }
    };
    Ok(outcome)
}

#[cfg(windows)]
fn confirm_action(console: &mut ConsoleIo<'_>, prompt: &str) -> io::Result<bool> {
    if !console.is_interactive {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "non-interactive stdin requires --yes (or --export-reg <PATH> for install)",
        ));
    }
    write!(console.output, "{prompt}")?;
    console.output.flush()?;

    let mut line = String::new();
    let bytes = console.input.read_line(&mut line)?;
    if bytes == 0 {
        return Ok(false);
    }
    let answer = line.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}
