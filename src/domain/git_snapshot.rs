//! Pure Git metadata snapshot models shared across Git extraction and XML rendering.

/// Single commit summary rendered inside `<git_context>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommitEntry {
    /// Abbreviated commit hash.
    pub short_hash: String,
    /// Commit title line.
    pub subject: String,
    /// Optional truncated commit body.
    pub body: Option<String>,
}

/// Recent Git tag summary rendered inside `<recent_tags>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTagEntry {
    /// Short tag name (e.g. `v0.4.0`).
    pub name: String,
    /// Optional single/truncated summary from annotation or tagged commit.
    pub summary: Option<String>,
}

/// File modification frequency entry rendered inside `<hotspots>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHotspotEntry {
    /// Forward-slash relative path inside the repository.
    pub rel_path: String,
    /// Number of commits touching this file within the sampled history window.
    pub commits: usize,
}

/// Line-change summary reported by `git diff --numstat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitDiffStat {
    /// Textual file line additions and deletions.
    Text {
        /// Number of added lines (`+N`).
        added: usize,
        /// Number of deleted lines (`-M`).
        deleted: usize,
    },
    /// Binary file modification reported as `-\t-` by Git.
    Binary,
}

/// Single modified-file numstat entry rendered inside `<working_diff_numstat>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiffNumstatEntry {
    /// Text line delta or binary change indicator.
    pub stat: GitDiffStat,
    /// Path relative to the inspected root or monorepo subdirectory.
    pub rel_path: String,
}

/// Optional Git repository context snapshot rendered when enabled (`[g]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitContextSnapshot {
    /// Active branch summary (including ahead/behind status when available).
    pub branch: String,
    /// Short `git status` lines for the current working tree.
    pub working_tree_lines: Vec<String>,
    /// Bounded `git diff --numstat` entries for tracked working-tree/staged changes.
    pub working_diff_numstat: Vec<GitDiffNumstatEntry>,
    /// Recent commits ordered newest-first.
    pub recent_commits: Vec<GitCommitEntry>,
    /// Recent repository tags ordered newest-first.
    pub recent_tags: Vec<GitTagEntry>,
    /// Most frequently modified files in recent history.
    pub hotspots: Vec<GitHotspotEntry>,
}
