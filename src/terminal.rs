pub fn width() -> usize {
    if let Ok(val) = std::env::var("COLUMNS")
        && let Ok(cols) = val.parse::<usize>()
    {
        return cols.max(72);
    }

    if let Some(cols) = tiocgwinsz() {
        return cols.max(72);
    }

    132
}

#[cfg(unix)]
fn tiocgwinsz() -> Option<usize> {
    use libc::{STDIN_FILENO, STDOUT_FILENO, TIOCGWINSZ, ioctl, winsize};
    use std::mem::zeroed;

    for fd in [STDOUT_FILENO, STDIN_FILENO] {
        unsafe {
            let mut ws: winsize = zeroed();
            if ioctl(fd, TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
                return Some(ws.ws_col as usize);
            }
        }
    }
    None
}

#[cfg(not(unix))]
fn tiocgwinsz() -> Option<usize> {
    None
}
