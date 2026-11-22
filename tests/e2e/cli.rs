//! CLI flag parsing, Explorer path normalization, and binary stdout/stderr tests.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use cpdir::cli::{CliCommand, CliParseError, normalize_explorer_path, parse_cli_args};
use cpdir::package_version;

use crate::common::workspace::WorkspaceBuilder;

#[test]
fn cli_normalizes_explorer_drive_root_quotes_unc_and_rejects_invalid_args() {
    assert_eq!(normalize_explorer_path(r#"C:""#), Ok(PathBuf::from(r"C:\")));
    assert_eq!(normalize_explorer_path(r#"d:""#), Ok(PathBuf::from(r"d:\")));
    assert_eq!(
        normalize_explorer_path(r#"C:\Users\Dev\My (Repo) [v1]""#),
        Ok(PathBuf::from(r"C:\Users\Dev\My (Repo) [v1]"))
    );
    assert_eq!(
        normalize_explorer_path(r#""C:\Quoted Folder\proj""#),
        Ok(PathBuf::from(r"C:\Quoted Folder\proj"))
    );
    assert_eq!(
        normalize_explorer_path(r#"\\nas\share\repo""#),
        Ok(PathBuf::from(r"\\nas\share\repo"))
    );
    assert_eq!(normalize_explorer_path("C:"), Ok(PathBuf::from("C:")));
    assert_eq!(normalize_explorer_path(""), Err(CliParseError::EmptyPath));

    assert_eq!(
        parse_cli_args(["cpdir"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from("."),
            include_abs_path: false,
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "--abs-path"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from("."),
            include_abs_path: true,
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "--abs-path", r"C:\Work\proj"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from(r"C:\Work\proj"),
            include_abs_path: true,
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", r"C:\Work\proj", "--abs-path"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from(r"C:\Work\proj"),
            include_abs_path: true,
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "--", "--abs-path"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from("--abs-path"),
            include_abs_path: false,
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "--", "-dash-folder"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from("-dash-folder"),
            include_abs_path: false,
        })
    );
    assert_eq!(parse_cli_args(["cpdir", "--help"]), Ok(CliCommand::Help));
    assert_eq!(parse_cli_args(["cpdir", "-V"]), Ok(CliCommand::Version));
    assert_eq!(
        parse_cli_args(["cpdir", "--unknown"]),
        Err(CliParseError::UnknownFlag("--unknown".to_owned()))
    );
    assert_eq!(
        parse_cli_args(["cpdir", "one", "two"]),
        Err(CliParseError::UnexpectedArgument("two".to_owned()))
    );
}

#[test]
fn cli_parses_install_and_uninstall_subcommands_with_escape_hatch() {
    assert_eq!(
        parse_cli_args(["cpdir", "install"]),
        Ok(CliCommand::Install {
            yes: false,
            export_reg: None,
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "install", "--yes", "--export-reg", "menu.reg"]),
        Ok(CliCommand::Install {
            yes: true,
            export_reg: Some(PathBuf::from("menu.reg")),
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "install", "--export-reg=out/cpdir.reg", "-y"]),
        Ok(CliCommand::Install {
            yes: true,
            export_reg: Some(PathBuf::from("out/cpdir.reg")),
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "uninstall"]),
        Ok(CliCommand::Uninstall { yes: false })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "uninstall", "-y"]),
        Ok(CliCommand::Uninstall { yes: true })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "install", "--help"]),
        Ok(CliCommand::Help)
    );
    assert_eq!(
        parse_cli_args(["cpdir", "--", "install"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from("install"),
            include_abs_path: false,
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "./install"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from("./install"),
            include_abs_path: false,
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "--abs-path", "install"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from("install"),
            include_abs_path: true,
        })
    );

    assert_eq!(
        parse_cli_args(["cpdir", "install", "--export-reg"]),
        Err(CliParseError::MissingFlagValue("--export-reg".to_owned()))
    );
    assert_eq!(
        parse_cli_args(["cpdir", "install", "--export-reg", "--yes"]),
        Err(CliParseError::MissingFlagValue("--export-reg".to_owned()))
    );
    assert_eq!(
        parse_cli_args(["cpdir", "install", "--abs-path"]),
        Err(CliParseError::UnsupportedFlag {
            flag: "--abs-path".to_owned(),
            command: "install",
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "uninstall", "--export-reg", "a.reg"]),
        Err(CliParseError::UnsupportedFlag {
            flag: "--export-reg".to_owned(),
            command: "uninstall",
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "--yes"]),
        Err(CliParseError::UnsupportedFlag {
            flag: "--yes".to_owned(),
            command: "run",
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "install", "extra"]),
        Err(CliParseError::UnexpectedArgument("extra".to_owned()))
    );
}

#[test]
fn compiled_binary_reports_help_version_and_non_tty_errors_without_touching_stdout() {
    let bin = env!("CARGO_BIN_EXE_cpdir");

    let help_out = Command::new(bin).arg("--help").output().expect("run help");
    assert!(help_out.status.success());
    assert_eq!(String::from_utf8_lossy(&help_out.stdout), "");
    let help_stderr = String::from_utf8_lossy(&help_out.stderr);
    assert!(help_stderr.contains("USAGE:"));
    assert!(help_stderr.contains("--abs-path"));
    assert!(help_stderr.contains("--export-reg"));

    let ver_out = Command::new(bin).arg("-V").output().expect("run version");
    assert!(ver_out.status.success());
    assert_eq!(String::from_utf8_lossy(&ver_out.stdout), "");
    let expected_version = format!("cpdir {}", package_version());
    assert!(String::from_utf8_lossy(&ver_out.stderr).contains(&expected_version));

    let bad_flag = Command::new(bin)
        .arg("--invalid-opt")
        .output()
        .expect("run bad flag");
    assert!(!bad_flag.status.success());
    assert_eq!(String::from_utf8_lossy(&bad_flag.stdout), "");

    let ws = WorkspaceBuilder::new();
    let plain_file = ws.write_text("not_a_dir.txt", "hello");
    let file_err = Command::new(bin)
        .arg(&plain_file)
        .output()
        .expect("run file target");
    assert!(!file_err.status.success());
    assert_eq!(String::from_utf8_lossy(&file_err.stdout), "");

    let non_tty = Command::new(bin)
        .arg(ws.root())
        .output()
        .expect("run non-tty dir target");
    assert!(!non_tty.status.success());
    assert_eq!(String::from_utf8_lossy(&non_tty.stdout), "");
    assert!(String::from_utf8_lossy(&non_tty.stderr).contains("interactive terminal"));
}

#[test]
fn compiled_binary_install_export_reg_and_non_tty_safety() {
    let bin = env!("CARGO_BIN_EXE_cpdir");
    let ws = WorkspaceBuilder::new();
    let reg_file = ws.root().join("cpdir-menu.reg");

    let export_out = Command::new(bin)
        .args(["install", "--export-reg"])
        .arg(&reg_file)
        .output()
        .expect("run install --export-reg");
    assert!(export_out.status.success());
    assert_eq!(String::from_utf8_lossy(&export_out.stdout), "");
    assert!(String::from_utf8_lossy(&export_out.stderr).contains("Exported Windows Registry"));
    let bytes = fs::read(&reg_file).expect("read exported .reg");
    assert_eq!(&bytes[..2], &[0xFF, 0xFE]);

    let non_tty_install = Command::new(bin)
        .arg("install")
        .output()
        .expect("run non-tty install");
    assert!(!non_tty_install.status.success());
    assert_eq!(String::from_utf8_lossy(&non_tty_install.stdout), "");
    assert!(String::from_utf8_lossy(&non_tty_install.stderr).contains("--yes"));

    let non_tty_uninstall = Command::new(bin)
        .arg("uninstall")
        .output()
        .expect("run non-tty uninstall");
    assert!(!non_tty_uninstall.status.success());
    assert_eq!(String::from_utf8_lossy(&non_tty_uninstall.stdout), "");
    assert!(String::from_utf8_lossy(&non_tty_uninstall.stderr).contains("--yes"));
}
