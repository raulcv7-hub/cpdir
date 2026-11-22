//! Binary entry point for the `cpdir` Windows directory-to-clipboard tool.

use std::process::ExitCode;

use cpdir::application::{RunOutcome, run_interactive};
use cpdir::cli::{CliCommand, parse_cli_args, usage_text};
use cpdir::output::SystemClipboard;
use cpdir::package_version;
use cpdir::platform::{run_install_subcommand, run_uninstall_subcommand};

/// Executes the `cpdir` entry point and maps operational outcomes to exit codes.
fn main() -> ExitCode {
    let command = match parse_cli_args(std::env::args_os()) {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("cpdir error: {err}\n\n{}", usage_text());
            return ExitCode::FAILURE;
        }
    };

    match command {
        CliCommand::Help => {
            eprintln!("{}", usage_text());
            ExitCode::SUCCESS
        }
        CliCommand::Version => {
            eprintln!("cpdir {}", package_version());
            ExitCode::SUCCESS
        }
        CliCommand::Install { yes, export_reg } => {
            match run_install_subcommand(yes, export_reg.as_deref()) {
                Ok(_) => ExitCode::SUCCESS,
                Err(err) => {
                    eprintln!("cpdir install error: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        CliCommand::Uninstall { yes } => match run_uninstall_subcommand(yes) {
            Ok(_) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("cpdir uninstall error: {err}");
                ExitCode::FAILURE
            }
        },
        CliCommand::Run {
            target_dir,
            include_abs_path,
        } => match run_interactive(&target_dir, include_abs_path, &SystemClipboard) {
            Ok(RunOutcome::Copied {
                files,
                tokens,
                xml_bytes,
            }) => {
                eprintln!(
                    "Copied {files} files ({tokens} tokens, {xml_bytes} bytes XML) to clipboard."
                );
                ExitCode::SUCCESS
            }
            Ok(RunOutcome::Cancelled) => {
                eprintln!("Cancelled.");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("cpdir error: {err}");
                ExitCode::FAILURE
            }
        },
    }
}
