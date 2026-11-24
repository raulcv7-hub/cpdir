//! Encapsulated context toggles and token accounting state for [`crate::tui::TuiAppState`].

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitContextStatus {
    Unavailable,
    Pending,
    Ready { enabled: bool, tokens: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ContextTokenState {
    git: GitContextStatus,
    xml_overhead_tokens: usize,
    include_abs_path: bool,
}

impl ContextTokenState {
    pub(super) const fn new(has_git_context: bool) -> Self {
        let git = if has_git_context {
            GitContextStatus::Ready {
                enabled: true,
                tokens: 0,
            }
        } else {
            GitContextStatus::Unavailable
        };
        Self {
            git,
            xml_overhead_tokens: 0,
            include_abs_path: false,
        }
    }

    pub(super) const fn has_git_context(self) -> bool {
        matches!(self.git, GitContextStatus::Ready { .. })
    }

    pub(super) const fn is_git_pending(self) -> bool {
        matches!(self.git, GitContextStatus::Pending)
    }

    pub(super) const fn set_git_pending(&mut self, pending: bool) {
        if pending && !matches!(self.git, GitContextStatus::Ready { .. }) {
            self.git = GitContextStatus::Pending;
        } else if !pending && matches!(self.git, GitContextStatus::Pending) {
            self.git = GitContextStatus::Unavailable;
        }
    }

    pub(super) fn attach_git_context(&mut self, tokens: usize, saved_enabled: Option<bool>) {
        self.git = GitContextStatus::Ready {
            enabled: saved_enabled.unwrap_or(true),
            tokens,
        };
    }

    pub(super) const fn include_git_context(self) -> bool {
        matches!(self.git, GitContextStatus::Ready { enabled: true, .. })
    }

    pub(super) const fn set_git_context_tokens(&mut self, tokens: usize) {
        if let GitContextStatus::Ready { tokens: cur, .. } = &mut self.git {
            *cur = tokens;
        }
    }

    pub(super) const fn git_context_tokens(self) -> usize {
        match self.git {
            GitContextStatus::Ready { tokens, .. } => tokens,
            GitContextStatus::Unavailable | GitContextStatus::Pending => 0,
        }
    }

    pub(super) const fn set_xml_overhead_tokens(&mut self, tokens: usize) {
        self.xml_overhead_tokens = tokens;
    }

    pub(super) const fn xml_overhead_tokens(self) -> usize {
        self.xml_overhead_tokens
    }

    pub(super) const fn total_effective_tokens(self, selected_file_tokens: usize) -> usize {
        let base = selected_file_tokens.saturating_add(self.xml_overhead_tokens);
        match self.git {
            GitContextStatus::Ready {
                enabled: true,
                tokens,
            } => base.saturating_add(tokens),
            _ => base,
        }
    }

    pub(super) const fn set_include_git_context(&mut self, enabled: bool) {
        if let GitContextStatus::Ready { enabled: cur, .. } = &mut self.git {
            *cur = enabled;
        }
    }

    pub(super) const fn toggle_git_context(&mut self) {
        if let GitContextStatus::Ready { enabled, .. } = &mut self.git {
            *enabled = !*enabled;
        }
    }

    pub(super) const fn include_abs_path(self) -> bool {
        self.include_abs_path
    }

    pub(super) const fn set_include_abs_path(&mut self, enabled: bool) {
        self.include_abs_path = enabled;
    }

    pub(super) const fn toggle_abs_path(&mut self) {
        self.include_abs_path = !self.include_abs_path;
    }
}
