//! Windows 11 platform integration for opt-in `HKCU` Explorer context menu and `.reg` export.

pub mod reg_export;
#[cfg(windows)]
pub mod win_registry;

pub use reg_export::{
    CONTEXT_MENU_LABEL, ContextMenuSpec, render_context_menu_reg_text,
    render_context_menu_reg_utf16le,
};
#[cfg(windows)]
pub use win_registry::{
    InstallOutcome, RegTarget, UninstallOutcome, install_context_menu, read_installed_commands,
    uninstall_context_menu,
};
