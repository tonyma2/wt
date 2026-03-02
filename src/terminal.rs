pub fn is_stdout_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

pub struct Colors {
    pub green: &'static str,
    pub bold_yellow: &'static str,
    pub red: &'static str,
    pub dim: &'static str,
    pub reset: &'static str,
}

pub fn colors() -> Colors {
    let enabled = is_stdout_tty()
        && std::env::var("NO_COLOR").is_err()
        && std::env::var("TERM").as_deref() != Ok("dumb");
    if enabled {
        Colors {
            green: "\x1b[32m",
            bold_yellow: "\x1b[1;33m",
            red: "\x1b[31m",
            dim: "\x1b[2m",
            reset: "\x1b[0m",
        }
    } else {
        Colors {
            green: "",
            bold_yellow: "",
            red: "",
            dim: "",
            reset: "",
        }
    }
}

pub fn tilde_path(path: &std::path::Path) -> String {
    let path_str = path.to_string_lossy();
    let Ok(home) = std::env::var("HOME") else {
        return path_str.into_owned();
    };
    if let Some(rest) = path_str.strip_prefix(&home)
        && (rest.is_empty() || rest.starts_with('/'))
    {
        return format!("~{rest}");
    }
    // Also match the canonical form of HOME (e.g. /private/var vs /var on macOS)
    if let Ok(canon) = std::path::Path::new(&home).canonicalize()
        && let Some(rest) = path_str.strip_prefix(canon.to_string_lossy().as_ref())
        && (rest.is_empty() || rest.starts_with('/'))
    {
        return format!("~{rest}");
    }
    path_str.into_owned()
}

pub fn print_cd_hint(name: &str) {
    if is_stdout_tty() {
        let escaped = name.replace("'", r"'\''");
        eprintln!("cd \"$(wt path '{escaped}')\"");
    }
}

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
