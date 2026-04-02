pub fn is_stdout_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

pub fn is_stderr_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

pub struct Colors {
    pub bold: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
    pub bold_yellow: &'static str,
    pub red: &'static str,
    pub dim: &'static str,
    pub reset: &'static str,
}

pub fn color_enabled(is_tty: bool) -> bool {
    is_tty && std::env::var("NO_COLOR").is_err() && std::env::var("TERM").as_deref() != Ok("dumb")
}

fn make_colors(enabled: bool) -> Colors {
    if enabled {
        Colors {
            bold: "\x1b[1m",
            green: "\x1b[32m",
            yellow: "\x1b[33m",
            bold_yellow: "\x1b[1;33m",
            red: "\x1b[31m",
            dim: "\x1b[2m",
            reset: "\x1b[0m",
        }
    } else {
        Colors {
            bold: "",
            green: "",
            yellow: "",
            bold_yellow: "",
            red: "",
            dim: "",
            reset: "",
        }
    }
}

pub fn colors() -> Colors {
    make_colors(color_enabled(is_stdout_tty()))
}

pub fn stderr_colors() -> Colors {
    make_colors(color_enabled(is_stderr_tty()))
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

pub fn trunc(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    if max <= 3 {
        return s.chars().take(max).collect();
    }
    let end = s.char_indices().nth(max - 3).map_or(s.len(), |(i, _)| i);
    format!("{}...", &s[..end])
}

pub fn trunc_tail(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    if max <= 3 {
        let start = s.char_indices().nth(count - max).map_or(0, |(i, _)| i);
        return s[start..].to_string();
    }
    let start = s.char_indices().nth(count - max + 3).map_or(0, |(i, _)| i);
    format!("...{}", &s[start..])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trunc_short_string_unchanged() {
        assert_eq!(trunc("main", 10), "main");
        assert_eq!(trunc("main", 4), "main");
    }

    #[test]
    fn trunc_long_string_adds_ellipsis() {
        assert_eq!(trunc("feat/very-long-branch-name", 10), "feat/ve...");
        assert_eq!(trunc("abcdef", 3), "abc");
    }

    #[test]
    fn trunc_zero_budget() {
        assert_eq!(trunc("anything", 0), "");
    }

    #[test]
    fn trunc_budget_one() {
        assert_eq!(trunc("abc", 1), "a");
    }

    #[test]
    fn trunc_tail_short_string_unchanged() {
        assert_eq!(trunc_tail("main", 10), "main");
        assert_eq!(trunc_tail("main", 4), "main");
    }

    #[test]
    fn trunc_tail_long_string_keeps_tail() {
        assert_eq!(
            trunc_tail("~/.wt/worktrees/abc123/my-repo", 15),
            "...c123/my-repo"
        );
    }

    #[test]
    fn trunc_tail_zero_budget() {
        assert_eq!(trunc_tail("anything", 0), "");
    }

    #[test]
    fn trunc_tail_budget_one() {
        assert_eq!(trunc_tail("abc", 1), "c");
    }

    #[test]
    fn tilde_path_substitutes_home() {
        let home = std::env::var("HOME").unwrap();
        let path = std::path::PathBuf::from(&home).join("projects/repo");
        assert_eq!(tilde_path(&path), "~/projects/repo");
    }

    #[test]
    fn tilde_path_home_alone() {
        let home = std::env::var("HOME").unwrap();
        let path = std::path::PathBuf::from(&home);
        assert_eq!(tilde_path(&path), "~");
    }

    #[test]
    fn tilde_path_no_match() {
        let path = std::path::PathBuf::from("/other/path");
        assert_eq!(tilde_path(&path), "/other/path");
    }

    #[test]
    fn tilde_path_no_false_prefix_match() {
        let home = std::env::var("HOME").unwrap();
        let fake = std::path::PathBuf::from(format!("{home}extra/dir"));
        assert_eq!(tilde_path(&fake), fake.to_string_lossy().as_ref());
    }
}
