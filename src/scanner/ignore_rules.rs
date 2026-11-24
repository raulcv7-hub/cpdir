//! Hierarchical `.ignore` over `.gitignore` rule evaluation across arbitrary directory depths.

use std::path::Path;

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::domain::{NaturalExclusion, NaturalSelection};

/// Result of evaluating `.ignore` and `.gitignore` stacks for a filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreDecision {
    /// Entry is included by default (either unmatched or whitelisted via `!` rule).
    Included,
    /// Excluded by a `.ignore` rule at current or ancestor directory level.
    ExcludedByDotIgnore,
    /// Excluded by a `.gitignore` rule without a overriding `.ignore` whitelist.
    ExcludedByGitIgnore,
}

impl IgnoreDecision {
    /// Converts the ignore decision into the domain's [`NaturalSelection`] baseline.
    #[must_use]
    pub const fn to_natural_selection(self) -> NaturalSelection {
        match self {
            Self::Included => NaturalSelection::Included,
            Self::ExcludedByDotIgnore => NaturalSelection::Excluded(NaturalExclusion::DotIgnore),
            Self::ExcludedByGitIgnore => NaturalSelection::Excluded(NaturalExclusion::GitIgnore),
        }
    }
}

#[derive(Debug)]
struct IgnoreLayer {
    dot_ignore: Option<Gitignore>,
    git_ignore: Option<Gitignore>,
}

/// Depth-aligned stack of nested `.ignore` and `.gitignore` matchers.
#[derive(Debug)]
pub struct IgnoreRuleStack {
    layers: Vec<IgnoreLayer>,
    case_insensitive: bool,
}

impl Default for IgnoreRuleStack {
    fn default() -> Self {
        Self::new()
    }
}

impl IgnoreRuleStack {
    /// Creates an empty stack defaulting to platform path case sensitivity (`true` on Windows).
    #[must_use]
    pub fn new() -> Self {
        Self::with_case_sensitivity(cfg!(windows))
    }

    /// Creates an empty stack with explicit case-sensitivity control for deterministic tests.
    #[must_use]
    pub fn with_case_sensitivity(case_insensitive: bool) -> Self {
        Self {
            layers: Vec::new(),
            case_insensitive,
        }
    }

    /// Returns the number of directory levels currently pushed onto the stack.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.layers.len()
    }

    /// Returns `true` when any active `.ignore` layer contains a `!` whitelist rule.
    #[must_use]
    pub fn has_dot_ignore_whitelist(&self) -> bool {
        self.layers.iter().any(|layer| {
            layer
                .dot_ignore
                .as_ref()
                .is_some_and(|gi| gi.num_whitelists() > 0)
        })
    }

    /// Loads optional `.ignore` and `.gitignore` files inside `dir_path` as a new layer.
    pub fn push_dir(&mut self, dir_path: &Path) {
        let dot_ignore =
            compile_ignore_file(dir_path, &dir_path.join(".ignore"), self.case_insensitive);
        let git_ignore = compile_ignore_file(
            dir_path,
            &dir_path.join(".gitignore"),
            self.case_insensitive,
        );
        self.layers.push(IgnoreLayer {
            dot_ignore,
            git_ignore,
        });
    }

    /// Pops the deepest directory layer upon leaving a directory subtree.
    pub fn pop_dir(&mut self) {
        self.layers.pop();
    }

    /// Evaluates `abs_path` against active layers, prioritizing `.ignore` strictly over `.gitignore`.
    #[must_use]
    pub fn evaluate(&self, abs_path: &Path, is_dir: bool) -> IgnoreDecision {
        for layer in self.layers.iter().rev() {
            if let Some(matcher) = &layer.dot_ignore {
                match matcher.matched_path_or_any_parents(abs_path, is_dir) {
                    Match::Ignore(_) => return IgnoreDecision::ExcludedByDotIgnore,
                    Match::Whitelist(_) => return IgnoreDecision::Included,
                    Match::None => {}
                }
            }
        }

        for layer in self.layers.iter().rev() {
            if let Some(matcher) = &layer.git_ignore {
                match matcher.matched_path_or_any_parents(abs_path, is_dir) {
                    Match::Ignore(_) => return IgnoreDecision::ExcludedByGitIgnore,
                    Match::Whitelist(_) => return IgnoreDecision::Included,
                    Match::None => {}
                }
            }
        }

        IgnoreDecision::Included
    }
}

fn compile_ignore_file(
    dir_path: &Path,
    file_path: &Path,
    case_insensitive: bool,
) -> Option<Gitignore> {
    if !file_path.is_file() {
        return None;
    }
    let mut builder = GitignoreBuilder::new(dir_path);
    // Continue gracefully if an ignore file contains malformed pattern lines or unreadable globs.
    let _ = builder.case_insensitive(case_insensitive);
    let _ = builder.add(file_path);
    builder.build().ok()
}
