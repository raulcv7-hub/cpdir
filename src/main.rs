//! Binary entry point for the `cpdir` Windows directory-to-clipboard tool.

use std::process::ExitCode;

/// Executes the `cpdir` entry point and maps operational outcomes to process exit codes.
fn main() -> ExitCode {
    let _version = cpdir::package_version();
    ExitCode::SUCCESS
}
