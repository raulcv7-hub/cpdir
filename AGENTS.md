# Repository Engineering Rules

## Product Invariants

- Target Windows 11 CLI + interactive TUI (`cpdir`).
- Emit strictly escaped XML 1.0 exclusively to the system clipboard; never emit XML to `stdout` or split output into files.
- Preserve literal file fidelity: never strip comments, strip blank lines, or apply AST compression.
- Always emit the full `<directory_structure>` (including empty directories, binaries, and unselected files), pruning only hardcoded dependency/VCS folders.

## Architecture & Module Discipline

- Separate pure logic (Functional Core: `domain/`, `scanner/ignore_rules.rs`, `tokens/`, `security/`, `output/xml_writer.rs`) from I/O adapters (`scanner/walker.rs`, `scanner/file_reader.rs`, `git/`, `state/`, `platform/`, `tui/`).
- Keep `src/` modules between 40 and 150 LOC (hard limit: 250 LOC); split cohesive responsibilities before exceeding the ceiling.
- Make illegal states unrepresentable with algebraic enums; forbid `unsafe_code` and `println!` (`print_stdout = "deny"`).
- Document every public item and new/modified function with Rustdoc contracts (`///`); keep inline comments strictly to `WHY > WHAT`.

## Non-Obvious Constraints

- Never use `ignore::WalkBuilder` blindly: directory pruning hides `.gitignore` matches from the TUI and blocks nested `.ignore` `!` re-inclusions. Walk non-dependency directories with nested matcher stacks (`.ignore` > `.gitignore`).
- Persist only explicit user delta overrides in `%LOCALAPPDATA%\cpdir\state\` so unseen new files default to selected.
- Render `[!]` secret indicators regardless of file selection, but count active alerts and require confirmation only when secret-flagged files are `Selected`. Exempt `.env.example` and template fixtures.
- Harden all `git` subprocess calls with `-c core.fsmonitor= -c core.quotePath=false`; omit `<git_context>` and `[g]` silently outside Git repositories.
- Normalize Explorer `%V` trailing-backslash drive-root quoting quirks (`"C:\"`) at CLI argument ingestion.
- Keep `panic = "unwind"` in `Cargo.toml` so TUI RAII `Drop` guards always restore terminal raw mode.

## Verification & Workflow

- Single source of truth: run `task check` (`fmt` -> `lint` -> `test`) before any commit.
- Favor real filesystem fixture ingestion, XML round-trip parsing, `proptest`, real `git` repos, and `ratatui::backend::TestBackend` workflows over trivial unit assertions.
- Follow Conventional Commits (lowercase English imperative) and atomic, bisectable commits.
