use std::io;

use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::crossterm::cursor::MoveTo;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};

pub type StderrTerminal = Terminal<CrosstermBackend<io::Stderr>>;

pub fn init(height: u16) -> io::Result<StderrTerminal> {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        original(info);
    }));
    ratatui::crossterm::terminal::enable_raw_mode()?;

    let mut backend = CrosstermBackend::new(io::stderr());
    let size = backend.size()?;
    let pos = backend.get_cursor_position()?;

    let max_height = size.height.min(height);
    let lines_after_cursor = height.saturating_sub(1);
    backend.append_lines(lines_after_cursor)?;

    let available = size.height.saturating_sub(pos.y).saturating_sub(1);
    let missing = lines_after_cursor.saturating_sub(available);
    let row = pos.y.saturating_sub(missing);

    let rect = Rect::new(0, row, size.width, max_height);

    Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(rect),
        },
    )
}

pub fn restore() -> io::Result<()> {
    ratatui::crossterm::terminal::disable_raw_mode()
}

pub fn run<F, R>(height: u16, f: F) -> io::Result<R>
where
    F: FnOnce(&mut StderrTerminal) -> io::Result<R>,
{
    let mut terminal = init(height)?;
    let result = f(&mut terminal);
    let _ = terminal.clear();
    let _ = ratatui::crossterm::execute!(io::stderr(), MoveTo(0, terminal.get_frame().area().y));
    restore()?;
    result
}
