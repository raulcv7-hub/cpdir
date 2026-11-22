//! RAII guard managing raw terminal mode, alternate screen, buffered stderr, and mouse capture.

use std::io::{self, BufWriter, Write as _};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::application::error::AppError;

pub(super) struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<BufWriter<io::Stderr>>>,
}

impl TerminalGuard {
    pub(super) fn enter() -> Result<Self, AppError> {
        enable_raw_mode().map_err(|err| AppError::TerminalIo(err.to_string()))?;
        let mut stderr = io::stderr();
        if let Err(err) = execute!(stderr, EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(AppError::TerminalIo(err.to_string()));
        }
        let buffered = BufWriter::with_capacity(32 * 1024, stderr);
        let backend = CrosstermBackend::new(buffered);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(err) => {
                let _ = execute!(io::stderr(), DisableMouseCapture, LeaveAlternateScreen);
                let _ = disable_raw_mode();
                Err(AppError::TerminalIo(err.to_string()))
            }
        }
    }

    pub(super) fn terminal_mut(
        &mut self,
    ) -> &mut Terminal<CrosstermBackend<BufWriter<io::Stderr>>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.terminal.show_cursor();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = self.terminal.backend_mut().flush();
        let _ = disable_raw_mode();
    }
}
