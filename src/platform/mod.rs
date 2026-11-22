//! Windows 11 platform integration for opt-in `HKCU` Explorer context menu and `.reg` export.

pub mod install_cli;
pub mod reg_export;
#[cfg(windows)]
pub mod win_registry;

pub use install_cli::{
    ConsoleIo, InstallSubcommandOutcome, UninstallSubcommandOutcome, export_reg_template,
    run_install_subcommand, run_uninstall_subcommand,
};
#[cfg(windows)]
pub use install_cli::{execute_install_with_io, execute_uninstall_with_io};
pub use reg_export::{
    CONTEXT_MENU_LABEL, ContextMenuSpec, render_context_menu_reg_text,
    render_context_menu_reg_utf16le,
};
#[cfg(windows)]
pub use win_registry::{
    InstallOutcome, RegTarget, UninstallOutcome, install_context_menu, read_installed_commands,
    uninstall_context_menu,
};
