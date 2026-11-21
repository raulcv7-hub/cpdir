//! Binary entry point for the `cpdir` Windows directory-to-clipboard tool.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use cpdir::cli::{CliCommand, parse_cli_args, usage_text};
use cpdir::config::defaults::{HARD_READ_BYTE_CAP, is_pruned_dir_name};
use cpdir::domain::{
    DirectoryTree, NaturalSelection, NodeId, PrunedDirReason, TextFileData, TextFileState,
};
use cpdir::output::{copy_to_clipboard, render_strict_xml};
use cpdir::package_version;
use cpdir::scanner::{ReadFileOutcome, read_bounded_utf8};

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
        CliCommand::Run { target_dir } => match run_tracer_clipboard(&target_dir) {
            Ok(summary) => {
                eprintln!("{summary}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("cpdir error: {err}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run_tracer_clipboard(target_dir: &Path) -> Result<String, String> {
    if !target_dir.is_dir() {
        return Err(format!(
            "target path does not exist or is not a directory: {}",
            target_dir.display()
        ));
    }

    let root_name = resolve_root_name(target_dir);
    let mut tree = DirectoryTree::new(root_name);
    let mut contents: HashMap<NodeId, String> = HashMap::new();

    let entries = fs::read_dir(target_dir).map_err(|err| format!("failed to read dir: {err}"))?;
    let mut children = Vec::new();
    for entry_res in entries {
        let entry = entry_res.map_err(|err| format!("failed reading entry: {err}"))?;
        children.push((
            entry.file_name().to_string_lossy().into_owned(),
            entry.path(),
        ));
    }
    children.sort_by_key(|a| a.0.to_ascii_lowercase());

    for (name, path) in children {
        if path.is_dir() {
            if is_pruned_dir_name(&name) {
                tree.record_pruned_dir(&name, PrunedDirReason::DependencyOrVcs);
            } else {
                tree.add_dir(DirectoryTree::ROOT, &name, true)
                    .map_err(|err| err.to_string())?;
            }
        } else if path.is_file() {
            match read_bounded_utf8(&path, HARD_READ_BYTE_CAP) {
                ReadFileOutcome::Text { byte_len, content } => {
                    let id = tree
                        .add_text_file(
                            DirectoryTree::ROOT,
                            &name,
                            TextFileData {
                                byte_len,
                                tokens: 0,
                                secret_hits: 0,
                                state: TextFileState::new(NaturalSelection::Included),
                            },
                        )
                        .map_err(|err| err.to_string())?;
                    contents.insert(id, content);
                }
                ReadFileOutcome::Discarded(reason) => {
                    tree.add_discarded_file(DirectoryTree::ROOT, &name, reason)
                        .map_err(|err| err.to_string())?;
                }
            }
        }
    }

    let metrics = tree.compute_metrics();
    let xml = render_strict_xml(&tree, &contents, None).map_err(|err| err.to_string())?;
    copy_to_clipboard(&xml).map_err(|err| err.to_string())?;
    Ok(format!(
        "Copied {} files ({} bytes XML) to clipboard.",
        metrics.counts.read_files,
        xml.len()
    ))
}

fn resolve_root_name(target_dir: &Path) -> String {
    target_dir
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .or_else(|| {
            target_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "workspace".to_owned())
}
