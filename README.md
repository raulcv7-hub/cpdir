# cpdir

Interactive Windows 11 TUI and CLI utility that inspects a repository tree, counts exact `o200k_base` BPE tokens offline, scans for potential secrets, attaches optional scoped Git context, and copies LLM-clean strict XML 1.0 directly to the Windows clipboard.

## Key Features

- **Clipboard-Only Strict XML 1.0 (`CDATA`):** Emits `<cpdir_context>` exclusively to the system clipboard (`stdout` stays silent). File bodies are wrapped in `<![CDATA[...]]>` (splitting literal `]]>`) with Windows `CRLF` normalized to `LF`, so SVGs, HTML/TSX, shell scripts, and existing XML entities arrive unescaped without `&lt;`, `&amp;gt;`, or `&#xD;` token noise—while staying 100% valid XML 1.0 without AST compression or comment stripping.
- **Enriched Context Header:** Emits `<context_summary ... />` (with file counts and `pruned_dirs`) plus DFS-ordered stacked full paths in `<directory_structure>` including empty directories (`empty_dir/`) and omission/warning markers (`[binary]`, `[UTF8]`, `[>5MB]`, `[unreadable]`, `[unselected: ...] (N)`, `[!]`).
- **Privacy by Default:** Local absolute path (`root_path="..."`) is omitted unless explicitly opted in per session via `--abs-path` or the `[p]` TUI toggle, and is never persisted to disk.
- **Non-Blocking Startup & Bounded Memory:** Opens the TUI immediately with live scan progress and `Esc`/`q`/`Ctrl+C` cancellation, probes unknown files at `8 KiB` before allocating larger buffers, streams parallel `o200k_base` token counting (`rayon`) without double-buffering the repository, drops RAM buffers above inline cache budgets (`>64 KiB` unselected / `64 MiB` selected cap), and re-hydrates selected files from disk on copy.
- **Hierarchical Ignore & GPU/C++ Pruning:** Evaluates nested `.ignore` > `.gitignore` stacks (`.ignore` `!` rules can rescue `.gitignore` entries) while keeping unselected entries visible in the tree. Unconditionally prunes VCS/dependency/build trees (`.git`, `node_modules`, `target`, `CMakeFiles`, `vcpkg_installed`, `.cache`, `.venv`, etc.), contextually prunes `build`/`dist`/`vendor`/`obj`/`coverage`/`out` outside source subtrees, and converts massive `.gitignore`d subtrees (`>1,000` entries without `.ignore` rescues) into on-demand re-ingestable `[IGNORED]` entries.
- **Local Secret Detection Guard:** Linear-time multi-pattern scanning flags files with `[!]` while exempting template files (`.env.example`, `*.sample`, lockfiles) and common UI placeholders. Copying selected secret-flagged files requires explicit confirmation (`[x]` to exclude all flagged files).
- **Windows-Hardened I/O & Git Context:** Skips symlinks/junctions and OneDrive cloud placeholders (`OFFLINE` / `RECALL_ON_*`), enforces a `5 MiB` hard read cap, and extracts scoped Git metadata with repository hooks and LFS filters neutralized plus per-command timeouts.
- **Per-Directory Delta Memory:** Persists only explicit user selection overrides and disabled-Git state under `%LOCALAPPDATA%\cpdir\state\`, so newly created files on disk default to selected.

## Installation (Windows 11)

### Option 1: One-Liner PowerShell Install (No Rust required)

Downloads the latest release asset from GitHub Releases, checks its SHA-256 hash, extracts `cpdir.exe` to `%LOCALAPPDATA%\Programs\cpdir`, and adds it to your User `PATH` (without modifying Explorer Registry keys):

```powershell
irm https://raw.githubusercontent.com/raulcv7-hub/cpdir/main/scripts/install-windows.ps1 | iex
```

### Option 2: Cargo (from crates.io or Git)

Once published to `crates.io`:

```powershell
cargo install cpdir-cli --locked
```

Or directly from GitHub:

```powershell
cargo install --git https://github.com/raulcv7-hub/cpdir --locked
```

### Option 3: Local Script (from cloned repository)

Installs `cpdir.exe` into `%LOCALAPPDATA%\Programs\cpdir` and adds it to your User `PATH`:

```powershell
.\scripts\install-windows.ps1
```

Optional switches:

- `-UseCargoInstall`: compiles and installs into `%USERPROFILE%\.cargo\bin` via `cargo install --path . --locked --force`.
- `-WithContextMenu`: additionally registers the optional `HKCU` Windows Explorer context menu (`cpdir install --yes`).

Open a new terminal after installation so Windows refreshes `PATH`.

## CLI Usage

```text
cpdir [OPTIONS] [DIRECTORY]
cpdir install [--yes] [--export-reg <PATH>]
cpdir uninstall [--yes]
cpdir [-h | --help]
cpdir [-V | --version]
```

Common invocations from any terminal:

- `cpdir`: inspect the current directory (`.`).
- `cpdir C:\path\to\project`: inspect a specific folder.
- `cpdir --abs-path`: start with `root_path` enabled in `<context_summary>`.
- `cpdir -- install` or `cpdir .\install`: inspect a folder literally named `install`.

### Optional Explorer Context Menu (`HKCU`)

Context-menu integration is strictly opt-in, requires no administrator privileges (`HKCU\Software\Classes`), and appears in Windows 11 under **Show more options** (`Shift + F10`):

- `cpdir install`: prompts `[y/N]` and registers `Directory` (`"%1"`) and `Directory\Background` (`"%V"`).
- `cpdir install --export-reg cpdir-menu.reg`: writes a `UTF-16LE` Registry Editor 5.00 `.reg` file for review without modifying live keys.
- `cpdir uninstall`: removes the registered `HKCU` context-menu keys idempotently.

## Interactive TUI Controls

### Keyboard

| Keys                            | Action                                                 |
| ------------------------------- | ------------------------------------------------------ |
| `Up` / `Down`, `j` / `k`        | Move cursor                                            |
| `PgUp` / `PgDn`, `Home` / `End` | Scroll page or jump to start/end                       |
| `Right` / `l`, `Left` / `h`     | Expand/enter folder, collapse or jump to parent        |
| `Tab`                           | Fold or unfold directory under cursor                  |
| `c` / `e`                       | Collapse all subfolders / Expand all folders           |
| `Space`                         | Toggle selection on file or folder subtree recursively |
| `a`                             | Toggle All (safe natural defaults) / None              |
| `u`                             | Unselect all files                                     |
| `i` / `n`, `d`, `s`             | Open Inspector on `Unread`, `Discarded`, or `Secrets`  |
| `g`                             | Toggle `<git_context>` (inside Git repositories)       |
| `p`                             | Toggle opt-in `root_path` in `<context_summary>`       |
| `?`                             | Toggle help overlay                                    |
| `Enter`                         | Render XML 1.0 and copy to clipboard                   |
| `Esc`, `q`, `Ctrl+C`            | Exit without modifying clipboard or saved state        |

Inside the **Inspector** modal: `Tab`/`BackTab`/`Left`/`Right` cycle tabs (`n`/`d`/`s` jump directly), `Space` toggles unread/secret items or loads `[PRUNED]` / `[IGNORED]` folders on demand (collapsed and unselected, capped at 2,000 entries), `Enter` reveals and jumps to the entry in the tree, and `Esc`/`q` closes the modal.

Inside confirmation modals: `y`/`Enter` copies anyway, `x` (secret modal) excludes all active flagged files and returns to the tree, and `n`/`Esc`/`q` cancels back to the tree.

### Mouse

- **Wheel Up / Down:** Scroll tree or inspector list.
- **Left Click on Fold Glyph (`▸` / `▾`):** Fold or unfold directory.
- **Left Click on Tree / Inspector Row:** Focus row and toggle selection (or load `[PRUNED]` / `[IGNORED]` folder).
- **Left Click on Inspector Tab Header:** Switch between `Unread`, `Discarded`, and `Secrets`.

### Color Coding

- **Hue = Token Weight or Secrets:** Green (`<2k`), Yellow (`<15k`), Orange (`<50k`), Red (`>=50k`, auto-deselected), Purple (`[!]` potential secret).
- **Brightness = Selection State:** Bright rows are active/selected; dimmed rows are unselected. Discarded non-text files appear in muted gray (`[BIN]`, `[UTF8]`, `[>5MB]`, `[ERR]`).

## Output XML Schema

```xml
<cpdir_context>
  <context_summary root="my_project" included_files="12" unselected_files="3" discarded_files="2" pruned_dirs=".git, node_modules, target" />
  <directory_structure>
    assets/logo.png [binary]
    empty_dir/
    logs/app.log [unselected: .gitignore] (420)
    src/.env [unselected: .gitignore] (45) [!]
    src/main.rs
  </directory_structure>
  <git_context>
    <branch>main</branch>
    <working_tree/>
    <recent_commits count="1">...</recent_commits>
  </git_context>
  <files>
    <file path="src/main.rs"><![CDATA[...]]></file>
  </files>
</cpdir_context>
```

## Development

```powershell
task check
task build
```

`task check` runs formatting (`cargo fmt --all -- --check`), strict linting (`cargo clippy --all-targets --locked -- -D warnings`), and all stratified test suites (`domain`, `scanner`, `security`, `git_output`, `platform`, `tui`, `e2e`).
