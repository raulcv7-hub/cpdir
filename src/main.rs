//! Binary entry point for the `cpdir` Windows directory-to-clipboard tool.

use std::process::ExitCode;

use cpdir::app::{RunOutcome, run_interactive};
use cpdir::cli::{CliCommand, parse_cli_args, usage_text};
use cpdir::output::SystemClipboard;
use cpdir::package_version;

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
        CliCommand::Run { target_dir } => match run_interactive(&target_dir, &SystemClipboard) {
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
