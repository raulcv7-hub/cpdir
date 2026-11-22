//! CLI flag parsing, Explorer path normalization, and binary stdout/stderr tests.

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
            target_dir: PathBuf::from(".")
        })
    );
    assert_eq!(
        parse_cli_args(["cpdir", "--", "-dash-folder"]),
        Ok(CliCommand::Run {
            target_dir: PathBuf::from("-dash-folder")
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
fn compiled_binary_reports_help_version_and_non_tty_errors_without_touching_stdout() {
    let bin = env!("CARGO_BIN_EXE_cpdir");

    let help_out = Command::new(bin).arg("--help").output().expect("run help");
    assert!(help_out.status.success());
    assert_eq!(String::from_utf8_lossy(&help_out.stdout), "");
    assert!(String::from_utf8_lossy(&help_out.stderr).contains("USAGE:"));

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
