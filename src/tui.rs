use std::io;

use ratatui::backend::CrosstermBackend;
use ratatui::{Terminal, TerminalOptions, Viewport};

pub type StdoutTerminal = Terminal<CrosstermBackend<io::Stdout>>;

fn init(height: u16) -> io::Result<StdoutTerminal> {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        original(info);
    }));
    ratatui::crossterm::terminal::enable_raw_mode()?;

    let backend = CrosstermBackend::new(io::stdout());
    Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )
    .inspect_err(|_| {
        let _ = restore();
    })
}

fn restore() -> io::Result<()> {
    let cursor = ratatui::crossterm::execute!(io::stdout(), ratatui::crossterm::cursor::Show);
    let raw = ratatui::crossterm::terminal::disable_raw_mode();
    cursor.and(raw)
}

pub fn run<F, R>(height: u16, f: F) -> io::Result<R>
where
    F: FnOnce(&mut StdoutTerminal) -> io::Result<R>,
{
    let mut terminal = init(height)?;
    let result = f(&mut terminal);
    let _ = terminal.clear();
    if result.is_ok() {
        restore()?;
    } else {
        let _ = restore();
    }
    result
}
