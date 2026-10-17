use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::Path;

use ratatui::crossterm::terminal;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn is_stdout_tty() -> bool {
    std::io::stdout().is_terminal()
}

pub fn is_stdin_tty() -> bool {
    std::io::stdin().is_terminal()
}

pub fn is_stderr_tty() -> bool {
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
    colors_allowed(
        is_tty,
        std::env::var_os("NO_COLOR"),
        std::env::var_os("TERM"),
    )
}

fn colors_allowed(is_tty: bool, no_color: Option<OsString>, term: Option<OsString>) -> bool {
    is_tty && no_color.is_none_or(|v| v.is_empty()) && term.is_none_or(|t| t != "dumb")
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

pub fn tilde_path(path: &Path) -> String {
    let rest = std::env::home_dir()
        .filter(|home| !home.as_os_str().is_empty())
        .and_then(|home| {
            path.strip_prefix(&home).ok().or_else(|| {
                let canonical = home.canonicalize().ok()?;
                path.strip_prefix(canonical).ok()
            })
        });
    match rest {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

pub fn display_width(s: &str) -> usize {
    s.width()
}

pub fn pad(s: &str, width: usize) -> String {
    format!("{s}{}", " ".repeat(width.saturating_sub(display_width(s))))
}

pub fn trunc(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    if max <= 1 {
        return take_width(s.chars(), max).collect();
    }
    let head: String = take_width(s.chars(), max - 1).collect();
    format!("{head}…")
}

pub fn trunc_tail(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    if max <= 1 {
        return take_width(s.chars().rev(), max).collect();
    }
    let mut tail: Vec<char> = take_width(s.chars().rev(), max - 1).collect();
    tail.reverse();
    let tail: String = tail.into_iter().collect();
    format!("…{tail}")
}

fn take_width(chars: impl Iterator<Item = char>, max: usize) -> impl Iterator<Item = char> {
    let mut used = 0;
    chars.take_while(move |c| {
        used += c.width().unwrap_or(0);
        used <= max
    })
}

pub fn print_cd_hint(path: &Path) {
    if is_stdout_tty() {
        eprintln!("{}", cd_hint(path));
    }
}

fn cd_hint(path: &Path) -> String {
    let escaped = path.to_string_lossy().replace('\'', r"'\''");
    format!("cd '{escaped}'")
}

pub fn width() -> usize {
    let columns = std::env::var("COLUMNS").ok().and_then(|v| v.parse().ok());
    // piped output is read by scripts, which need whole values
    if columns.is_none() && !is_stdout_tty() {
        return usize::MAX;
    }
    width_from(columns.or_else(|| {
        terminal::window_size()
            .ok()
            .map(|ws| ws.columns)
            .filter(|&cols| cols > 0)
    }))
}

fn width_from(columns: Option<u16>) -> usize {
    columns.map_or(132, |cols| usize::from(cols).max(72))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn trunc_short_string_unchanged() {
        assert_eq!(trunc("main", 10), "main");
        assert_eq!(trunc("main", 4), "main");
    }

    #[test]
    fn trunc_long_string_adds_ellipsis() {
        assert_eq!(trunc("feat/very-long-branch-name", 10), "feat/very…");
        assert_eq!(trunc("abcdef", 3), "ab…");
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
            "…abc123/my-repo"
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
        let path = PathBuf::from(&home).join("projects/repo");
        assert_eq!(tilde_path(&path), "~/projects/repo");
    }

    #[test]
    fn tilde_path_home_alone() {
        let home = std::env::var("HOME").unwrap();
        let path = PathBuf::from(&home);
        assert_eq!(tilde_path(&path), "~");
    }

    #[test]
    fn tilde_path_no_match() {
        let path = PathBuf::from("/other/path");
        assert_eq!(tilde_path(&path), "/other/path");
    }

    #[test]
    fn tilde_path_no_false_prefix_match() {
        let home = std::env::var("HOME").unwrap();
        let fake = PathBuf::from(format!("{home}extra/dir"));
        assert_eq!(tilde_path(&fake), fake.to_string_lossy().as_ref());
    }

    #[test]
    fn trunc_multibyte_chars() {
        assert_eq!(trunc("a\u{00e9}b\u{00e9}c\u{00e9}", 4), "a\u{00e9}b…");
        assert_eq!(
            trunc("\u{00e9}\u{00e9}\u{00e9}", 3),
            "\u{00e9}\u{00e9}\u{00e9}"
        );
        assert_eq!(
            trunc("\u{00e9}\u{00e9}\u{00e9}\u{00e9}", 3),
            "\u{00e9}\u{00e9}…"
        );
    }

    #[test]
    fn trunc_tail_multibyte_chars() {
        assert_eq!(
            trunc_tail("a\u{00e9}b\u{00e9}c\u{00e9}", 4),
            "…\u{00e9}c\u{00e9}"
        );
        assert_eq!(
            trunc_tail("\u{00e9}\u{00e9}\u{00e9}", 3),
            "\u{00e9}\u{00e9}\u{00e9}"
        );
        assert_eq!(
            trunc_tail("\u{00e9}\u{00e9}\u{00e9}\u{00e9}", 2),
            "…\u{00e9}"
        );
    }

    #[test]
    fn widths_count_display_columns() {
        assert_eq!(display_width("日本語"), 6);
        assert_eq!(pad("日本", 6), "日本  ");
        assert_eq!(pad("too-long", 3), "too-long");
    }

    #[test]
    fn trunc_wide_chars_fit_display_budget() {
        assert_eq!(trunc("feat/日本語ブランチ", 11), "feat/日本…");
        assert_eq!(trunc("日本語", 4), "日…");
        assert_eq!(trunc("日本語", 3), "日…");
        assert_eq!(trunc("日本語", 6), "日本語");
    }

    #[test]
    fn trunc_tail_wide_chars_fit_display_budget() {
        assert_eq!(trunc_tail("~/wt/日本語", 8), "…/日本語");
        assert_eq!(trunc_tail("日本語", 3), "…語");
        assert_eq!(trunc_tail("日本語", 6), "日本語");
    }

    #[test]
    fn trunc_exact_boundary() {
        assert_eq!(trunc("abcde", 5), "abcde");
        assert_eq!(trunc("abcde", 4), "abc…");
    }

    #[test]
    fn trunc_tail_exact_boundary() {
        assert_eq!(trunc_tail("abcde", 5), "abcde");
        assert_eq!(trunc_tail("abcde", 4), "…cde");
    }

    #[test]
    fn colors_disabled_without_tty() {
        assert!(!colors_allowed(false, None, None));
        assert!(colors_allowed(true, None, Some("xterm".into())));
    }

    #[test]
    fn colors_respect_no_color() {
        assert!(!colors_allowed(true, Some("1".into()), None));
        assert!(
            colors_allowed(true, Some("".into()), None),
            "empty NO_COLOR must not disable color"
        );
    }

    #[test]
    fn colors_disabled_for_dumb_term() {
        assert!(!colors_allowed(true, None, Some("dumb".into())));
    }

    #[test]
    fn cd_hint_quotes_path() {
        assert_eq!(
            cd_hint(Path::new("/wt/it's here")),
            r"cd '/wt/it'\''s here'"
        );
    }

    #[test]
    fn width_uses_columns() {
        assert_eq!(width_from(Some(200)), 200);
    }

    #[test]
    fn width_clamps_to_72() {
        assert_eq!(width_from(Some(40)), 72);
    }

    #[test]
    fn width_defaults_without_terminal() {
        assert_eq!(width_from(None), 132);
    }
}
