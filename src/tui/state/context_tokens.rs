//! Encapsulated context toggles and token accounting state for [`crate::tui::TuiAppState`].

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ContextTokenState {
    has_git_context: bool,
    include_git_context: bool,
    git_context_tokens: usize,
    xml_overhead_tokens: usize,
    include_abs_path: bool,
}

impl ContextTokenState {
    pub(super) const fn new(has_git_context: bool) -> Self {
        Self {
            has_git_context,
            include_git_context: has_git_context,
            git_context_tokens: 0,
            xml_overhead_tokens: 0,
            include_abs_path: false,
        }
    }

    pub(super) const fn has_git_context(self) -> bool {
        self.has_git_context
    }

    pub(super) const fn include_git_context(self) -> bool {
        self.has_git_context && self.include_git_context
    }

    pub(super) const fn set_git_context_tokens(&mut self, tokens: usize) {
        self.git_context_tokens = tokens;
    }

    pub(super) const fn git_context_tokens(self) -> usize {
        self.git_context_tokens
    }

    pub(super) const fn set_xml_overhead_tokens(&mut self, tokens: usize) {
        self.xml_overhead_tokens = tokens;
    }

    pub(super) const fn xml_overhead_tokens(self) -> usize {
        self.xml_overhead_tokens
    }

    pub(super) const fn total_effective_tokens(self, selected_file_tokens: usize) -> usize {
        let base = selected_file_tokens.saturating_add(self.xml_overhead_tokens);
        if self.include_git_context() {
            base.saturating_add(self.git_context_tokens)
        } else {
            base
        }
    }

    pub(super) const fn set_include_git_context(&mut self, enabled: bool) {
        if self.has_git_context {
            self.include_git_context = enabled;
        }
    }

    pub(super) const fn toggle_git_context(&mut self) {
        if self.has_git_context {
            self.include_git_context = !self.include_git_context;
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
