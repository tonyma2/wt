use std::io;

use ratatui::backend::CrosstermBackend;
use ratatui::{Terminal, TerminalOptions, Viewport};

pub type StderrTerminal = Terminal<CrosstermBackend<io::Stderr>>;

pub fn init(height: u16) -> io::Result<StderrTerminal> {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        original(info);
    }));
    ratatui::crossterm::terminal::enable_raw_mode()?;

    let backend = CrosstermBackend::new(io::stderr());
    Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )
}

pub fn restore() -> io::Result<()> {
    ratatui::crossterm::execute!(io::stderr(), ratatui::crossterm::cursor::Show)?;
    ratatui::crossterm::terminal::disable_raw_mode()
}

pub fn run<F, R>(height: u16, f: F) -> io::Result<R>
where
    F: FnOnce(&mut StderrTerminal) -> io::Result<R>,
{
    let mut terminal = init(height)?;
    let result = f(&mut terminal);
    let _ = terminal.clear();
    restore()?;
    result
}
