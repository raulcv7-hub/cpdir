//! Idempotent `HKCU` Windows 11 Explorer context-menu registration and isolated test targeting.

use std::io;

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

use crate::platform::reg_export::{CONTEXT_MENU_LABEL, ContextMenuSpec};

/// Target registry hive prefix allowing isolated testing outside `Software\Classes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegTarget {
    /// Production per-user Explorer shell keys (`HKCU\Software\Classes`).
    CurrentUser,
    /// Isolated subkey prefix under `HKCU` reserved for automated integration tests.
    CustomPrefix(String),
}

impl RegTarget {
    fn base_subkey(&self) -> &str {
        match self {
            Self::CurrentUser => r"Software\Classes",
            Self::CustomPrefix(prefix) => prefix.as_str(),
        }
    }

    /// Returns the `Directory\shell\cpdir` subkey path relative to `HKEY_CURRENT_USER`.
    #[must_use]
    pub fn directory_verb_path(&self) -> String {
        format!(r"{}\Directory\shell\cpdir", self.base_subkey())
    }

    /// Returns the `Directory\Background\shell\cpdir` subkey path relative to `HKEY_CURRENT_USER`.
    #[must_use]
    pub fn background_verb_path(&self) -> String {
        format!(r"{}\Directory\Background\shell\cpdir", self.base_subkey())
    }
}

/// Outcome returned by [`install_context_menu`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallOutcome {
    /// Both `Directory` and `Directory\Background` verbs were written idempotently under `HKCU`.
    Installed,
}

/// Outcome returned by [`uninstall_context_menu`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallOutcome {
    /// One or more context-menu keys were removed from `HKCU`.
    Removed,
    /// Neither context-menu key was present under `HKCU`.
    AlreadyAbsent,
}

/// Writes or updates `Directory` (`%1`) and `Directory\Background` (`%V`) context menu verbs.
///
/// # Errors
///
/// Returns an `io::Error` if `HKEY_CURRENT_USER` subkeys or values cannot be created or updated.
pub fn install_context_menu(
    target: &RegTarget,
    spec: &ContextMenuSpec,
) -> io::Result<InstallOutcome> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    write_verb(&hkcu, &target.directory_verb_path(), spec, "%1")?;
    write_verb(&hkcu, &target.background_verb_path(), spec, "%V")?;
    Ok(InstallOutcome::Installed)
}

/// Removes `cpdir` context menu keys from `target`, succeeding idempotently if already absent.
///
/// # Errors
///
/// Returns an `io::Error` if registry deletion fails for any reason other than missing keys.
pub fn uninstall_context_menu(target: &RegTarget) -> io::Result<UninstallOutcome> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let removed_dir = delete_subkey_if_present(&hkcu, &target.directory_verb_path())?;
    let removed_bg = delete_subkey_if_present(&hkcu, &target.background_verb_path())?;
    if removed_dir || removed_bg {
        Ok(UninstallOutcome::Removed)
    } else {
        Ok(UninstallOutcome::AlreadyAbsent)
    }
}

/// Reads currently registered command strings for `(Directory, Directory\Background)` under `target`.
///
/// # Errors
///
/// Returns an `io::Error` if registry access fails for reasons other than missing keys/values.
pub fn read_installed_commands(target: &RegTarget) -> io::Result<(Option<String>, Option<String>)> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let dir_cmd = read_command_value(&hkcu, &target.directory_verb_path())?;
    let bg_cmd = read_command_value(&hkcu, &target.background_verb_path())?;
    Ok((dir_cmd, bg_cmd))
}

fn write_verb(
    hkcu: &RegKey,
    verb_path: &str,
    spec: &ContextMenuSpec,
    placeholder: &str,
) -> io::Result<()> {
    let (verb_key, _) = hkcu.create_subkey(verb_path)?;
    verb_key.set_value("", &CONTEXT_MENU_LABEL)?;
    verb_key.set_value("Icon", &spec.icon_value())?;
    let (cmd_key, _) = verb_key.create_subkey("command")?;
    cmd_key.set_value("", &spec.shell_command(placeholder))?;
    Ok(())
}

fn delete_subkey_if_present(hkcu: &RegKey, subkey_path: &str) -> io::Result<bool> {
    match hkcu.delete_subkey_all(subkey_path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn read_command_value(hkcu: &RegKey, verb_path: &str) -> io::Result<Option<String>> {
    let cmd_path = format!(r"{verb_path}\command");
    match hkcu.open_subkey(&cmd_path) {
        Ok(key) => match key.get_value::<String, _>("") {
            Ok(val) => Ok(Some(val)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}
