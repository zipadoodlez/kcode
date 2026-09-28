//! Console/terminal ANSI capability helpers.
//!
//! These helpers let early startup output decide whether color is safe before
//! the TUI takes over the screen.

/// Report whether ANSI output is safe to emit on stderr.
///
/// True exactly when stderr is a terminal, so callers can fall back to plain
/// text instead of printing escape sequences into a pipe or a log file.
pub fn stderr_supports_ansi() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}
