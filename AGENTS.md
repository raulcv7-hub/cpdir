# Repository Engineering Rules

## Product Invariants

- Target Windows 11 CLI + interactive TUI (`cpdir`) writing strict XML 1.0 exclusively to the OS clipboard; keep `stdout` completely empty (`print_stdout = "deny"`).
- Preserve source fidelity without AST compression, comment stripping, or blank-line trimming; wrap non-empty `<file>` bodies in `<![CDATA[...]]>` (splitting literal `]]>`) and normalize `CRLF`/`CR` to `LF` in XML text to eliminate entity double-escaping and `&#xD;` token noise for LLMs.
- Emit `<context_summary ... />` followed by pre-order DFS stacked full paths in `<directory_structure>`, including empty directories (`dir/`), discarded files, and unselected files with omission badges.
- Keep `root_path` (`--abs-path` / `[p]`) ephemeral and off by default; never persist it to `%LOCALAPPDATA%`.
- Keep Windows Explorer `HKCU` context-menu registration (`cpdir install` / `uninstall` / `--export-reg`) strictly opt-in; never mutate Registry during binary installation or non-interactive runs without `--yes`.

## Architecture & Module Discipline

- Maintain Functional Core / Imperative Shell boundaries:
  - Pure Core: `domain/`, `config/`, `scanner/ignore_rules.rs`, `tokens/counter.rs`, `security/{placeholders.rs,rules.rs,scanner.rs}`, `output/{summary_xml.rs,xml_escape.rs,xml_writer.rs,git_xml.rs}`, `platform/reg_export.rs`, `tui/state/`.
  - I/O & UI Adapters: `application/` (`WorkspaceSession`, `event_loop.rs`, `terminal.rs`), `scanner/{walker.rs,rescan.rs,traversal.rs,file_reader.rs,content_store.rs}`, `git/`, `state/`, `output/clipboard.rs`, `platform/{win_registry.rs,install_cli.rs}`, `tui/{input/,view/}`.
- Enforce strict CQS in `tui/`: `tui/view/` reads `&TuiAppState` immutably and records hit-boxes in `&mut TuiViewportAreas`; call `TuiViewportAreas::sync_scroll` prior to rendering.
- Keep `src/` modules between 40 and 150 LOC (hard limit: 250 LOC); forbid `unsafe_code` and `println!`; document every public item with Rustdoc (`///`).

## Non-Obvious Constraints

- Never replace `scanner/traversal.rs` with `ignore::WalkBuilder`: nested `.ignore` `!` whitelists must rescue `.gitignore` matches while keeping excluded nodes visible in the TUI.
- Keep `NaturalExclusion::Pruned` on re-ingested pruned subtrees so `[a]` All/None never activates dependency files.
- Persist only minimal delta overrides in `%LOCALAPPDATA%\cpdir\state\` so unseen new files default to selected.
- Render `[!]` regardless of selection, exempting `.env.example`/templates/lockfiles; trigger `ConfirmSecrets` only when secret-flagged files are selected.
- Harden `git` subprocess calls (`-c core.fsmonitor= -c core.hooksPath= -c core.quotePath=false -c diff.external= -c gpg.program=`, `GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`) and omit `<git_context>` silently outside repositories.
- Normalize Explorer `"C:\"` trailing-quote quirks at CLI ingestion; parse `install`/`uninstall` only on the unflagged first positional argument (`-- install` and `./install` still scan folders).
- Keep explicit `panic = "unwind"` in `Cargo.toml` profiles so RAII guards (`TerminalGuard`) always restore terminal raw mode.

## Verification & Workflow

- Single source of truth: run `task check` (`fmt` -> `lint` -> `test`) before any commit across `tests/{domain,scanner,security,git_output,platform,tui,e2e}/`.
- Follow Conventional Commits (lowercase imperative English) with atomic, bisectable commits.
