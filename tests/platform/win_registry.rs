//! Unit and isolated `HKCU` round-trip tests for `.reg` export and context menu management.

#[cfg(windows)]
use std::io::{self, Cursor};
use std::path::PathBuf;
#[cfg(windows)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use cpdir::platform::{
    ConsoleIo, InstallOutcome, RegTarget, UninstallOutcome, UninstallSubcommandOutcome,
    execute_install_with_io, execute_uninstall_with_io, install_context_menu,
    read_installed_commands, uninstall_context_menu,
};
use cpdir::platform::{
    ContextMenuSpec, InstallSubcommandOutcome, export_reg_template, render_context_menu_reg_text,
    render_context_menu_reg_utf16le,
};
use tempfile::tempdir;
#[cfg(windows)]
use winreg::RegKey;
#[cfg(windows)]
use winreg::enums::HKEY_CURRENT_USER;

fn decode_utf16le_bom(bytes: &[u8]) -> String {
    assert!(bytes.len() >= 2 && bytes.len().is_multiple_of(2));
    assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
    let units: Vec<u16> = bytes[2..]
        .as_chunks::<2>()
        .0
        .iter()
        .copied()
        .map(u16::from_le_bytes)
        .collect();
    String::from_utf16(&units).expect("valid UTF-16LE .reg payload")
}

#[test]
fn reg_export_generates_valid_utf16le_bom_crlf_and_escapes_special_paths() {
    let spec = ContextMenuSpec::new(r"\\?\C:\Program Files\Tools & Utils\cpdir.exe");
    assert_eq!(
        spec.exe_display(),
        r"C:\Program Files\Tools & Utils\cpdir.exe"
    );
    assert!(!spec.looks_ephemeral_target_build());
    assert!(ContextMenuSpec::new(r"C:\repo\target\debug\cpdir.exe").looks_ephemeral_target_build());
    assert!(
        ContextMenuSpec::new(r"C:\repo\target\release\cpdir.exe").looks_ephemeral_target_build()
    );

    let text = render_context_menu_reg_text(&spec);
    assert!(text.starts_with("Windows Registry Editor Version 5.00\r\n\r\n"));
    assert!(!text.replace("\r\n", "").contains('\n'));
    assert!(text.contains(r"[HKEY_CURRENT_USER\Software\Classes\Directory\shell\cpdir\command]"));
    assert!(text.contains(
        r"[HKEY_CURRENT_USER\Software\Classes\Directory\Background\shell\cpdir\command]"
    ));
    assert!(text.contains(r#"@="\"C:\\Program Files\\Tools & Utils\\cpdir.exe\" \"%1\"""#));
    assert!(text.contains(r#"@="\"C:\\Program Files\\Tools & Utils\\cpdir.exe\" \"%V\"""#));

    let utf16_bytes = render_context_menu_reg_utf16le(&spec);
    assert_eq!(decode_utf16le_bom(&utf16_bytes), text);
    assert_eq!(
        ContextMenuSpec::new(PathBuf::from(r"\\?\UNC\nas\tools\cpdir.exe")).exe_display(),
        r"\\nas\tools\cpdir.exe"
    );

    let tmp = tempdir().expect("tempdir");
    let out_reg = tmp.path().join("cpdir-menu.reg");
    let mut msg = Vec::new();
    let outcome = export_reg_template(&spec, &out_reg, &mut msg).expect("export reg");
    assert_eq!(outcome, InstallSubcommandOutcome::ExportedReg);
    let disk_bytes = std::fs::read(&out_reg).expect("read .reg");
    assert_eq!(decode_utf16le_bom(&disk_bytes), text);
    assert!(String::from_utf8_lossy(&msg).contains("No live keys modified."));
}

#[cfg(windows)]
struct TestKeyCleanupGuard {
    prefix: String,
}

#[cfg(windows)]
impl Drop for TestKeyCleanupGuard {
    fn drop(&mut self) {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(&self.prefix);
    }
}

#[cfg(windows)]
fn unique_test_target() -> (RegTarget, TestKeyCleanupGuard) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let prefix = format!(r"Software\cpdir_test_{}_{}", std::process::id(), nonce);
    let guard = TestKeyCleanupGuard {
        prefix: prefix.clone(),
    };
    (RegTarget::CustomPrefix(prefix), guard)
}

#[cfg(windows)]
#[test]
fn hkcu_install_and_uninstall_roundtrip_under_isolated_test_key() {
    let (target, _guard) = unique_test_target();

    assert_eq!(
        uninstall_context_menu(&target).expect("absent uninstall"),
        UninstallOutcome::AlreadyAbsent
    );
    assert_eq!(
        read_installed_commands(&target).expect("read empty"),
        (None, None)
    );

    let spec_v1 = ContextMenuSpec::new(r"C:\Tools & Apps\cpdir.exe");
    assert_eq!(
        install_context_menu(&target, &spec_v1).expect("install v1"),
        InstallOutcome::Installed
    );
    assert_eq!(
        read_installed_commands(&target).expect("read v1"),
        (
            Some(r#""C:\Tools & Apps\cpdir.exe" "%1""#.to_owned()),
            Some(r#""C:\Tools & Apps\cpdir.exe" "%V""#.to_owned()),
        )
    );

    let spec_v2 = ContextMenuSpec::new(r"C:\Users\Dev\bin\cpdir.exe");
    assert_eq!(
        install_context_menu(&target, &spec_v2).expect("idempotent reinstall"),
        InstallOutcome::Installed
    );
    assert_eq!(
        read_installed_commands(&target).expect("read v2"),
        (
            Some(r#""C:\Users\Dev\bin\cpdir.exe" "%1""#.to_owned()),
            Some(r#""C:\Users\Dev\bin\cpdir.exe" "%V""#.to_owned()),
        )
    );

    assert_eq!(
        uninstall_context_menu(&target).expect("uninstall"),
        UninstallOutcome::Removed
    );
    assert_eq!(
        read_installed_commands(&target).expect("after uninstall"),
        (None, None)
    );
    assert_eq!(
        uninstall_context_menu(&target).expect("second uninstall"),
        UninstallOutcome::AlreadyAbsent
    );
}

#[cfg(windows)]
#[test]
fn install_cli_prompts_respect_confirmation_and_ephemeral_warnings() {
    let (target, _guard) = unique_test_target();
    let spec = ContextMenuSpec::new(r"C:\repo\target\release\cpdir.exe");

    // Non-interactive stdin without `--yes` fails cleanly without mutating Registry.
    let mut empty_in = Cursor::new(b"");
    let mut out = Vec::new();
    let mut non_tty = ConsoleIo {
        input: &mut empty_in,
        output: &mut out,
        is_interactive: false,
    };
    let err = execute_install_with_io(&target, &spec, false, &mut non_tty)
        .expect_err("non-tty without --yes must fail");
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(
        read_installed_commands(&target).expect("empty"),
        (None, None)
    );

    // Interactive 'n' aborts cleanly.
    let mut no_in = Cursor::new(b"n\n");
    let mut out_no = Vec::new();
    let mut console_no = ConsoleIo {
        input: &mut no_in,
        output: &mut out_no,
        is_interactive: true,
    };
    assert_eq!(
        execute_install_with_io(&target, &spec, false, &mut console_no).expect("cancel"),
        InstallSubcommandOutcome::Cancelled
    );
    assert_eq!(
        read_installed_commands(&target).expect("empty"),
        (None, None)
    );
    assert!(String::from_utf8_lossy(&out_no).contains("Warning: executable lives in"));

    // Interactive 'y' installs cleanly, and uninstall prompt 'yes' removes cleanly.
    let mut yes_in = Cursor::new(b"y\n");
    let mut out_yes = Vec::new();
    let mut console_yes = ConsoleIo {
        input: &mut yes_in,
        output: &mut out_yes,
        is_interactive: true,
    };
    assert_eq!(
        execute_install_with_io(&target, &spec, false, &mut console_yes).expect("install"),
        InstallSubcommandOutcome::Installed
    );
    assert!(
        read_installed_commands(&target)
            .expect("installed")
            .0
            .is_some()
    );

    let mut un_in = Cursor::new(b"yes\n");
    let mut out_un = Vec::new();
    let mut console_un = ConsoleIo {
        input: &mut un_in,
        output: &mut out_un,
        is_interactive: true,
    };
    assert_eq!(
        execute_uninstall_with_io(&target, false, &mut console_un).expect("uninstall"),
        UninstallSubcommandOutcome::Removed
    );
    assert_eq!(
        read_installed_commands(&target).expect("clean"),
        (None, None)
    );
}
