use std::io;
use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::fuzzy;
use crate::layout;
use crate::terminal::{self as term, display_width, trunc, trunc_tail};
use crate::worktree;

const EMPTY_HINT: &str = "no matches · backspace to edit";
const LOCK_BADGE: &str = "lock";
const SEP: &str = " · ";
const MAX_ROWS: usize = 8;
const REPO_MIN: usize = 8;
// branch columns include the 2-column gap before the next column
const BRANCH_MIN: usize = 16 + 2;

struct RepoData {
    name: String,
    worktrees: Vec<WorktreeData>,
}

#[derive(Default)]
struct WorktreeData {
    path: PathBuf,
    display_path: String,
    branch: Option<String>,
    filter_candidate: String,
    status: String,
    dirty: bool,
    detached: bool,
    locked: bool,
    current: bool,
}

impl WorktreeData {
    fn from_info(info: &worktree::WorktreeInfo, repo_name: &str) -> Self {
        let wt = &info.worktree;
        let filter_candidate = match &wt.branch {
            Some(b) => format!("{repo_name} {b}"),
            None => repo_name.to_owned(),
        };
        WorktreeData {
            display_path: term::tilde_path(&wt.path),
            path: wt.path.clone(),
            branch: wt.branch.clone(),
            filter_candidate,
            status: info.status.summary().unwrap_or_default(),
            dirty: info.status.dirty,
            detached: wt.detached,
            locked: wt.locked,
            current: info.current,
        }
    }

    fn display_branch(&self) -> &str {
        self.branch
            .as_deref()
            .unwrap_or(if self.detached { "(detached)" } else { "" })
    }
}

struct Row {
    repo: usize,
    wt: WorktreeData,
}

struct App {
    repos: Vec<String>,
    rows: Vec<Row>,
    filtered: Vec<usize>,
    cursor: Option<usize>,
    offset: usize,
    filter: String,
    quit: bool,
    selected_path: Option<PathBuf>,
    color: bool,
}

impl App {
    fn new(repos: Vec<RepoData>) -> Self {
        let mut names = Vec::new();
        let mut rows = Vec::new();
        for (repo, data) in repos.into_iter().enumerate() {
            names.push(data.name);
            rows.extend(data.worktrees.into_iter().map(|wt| Row { repo, wt }));
        }
        let mut app = Self {
            repos: names,
            filtered: Vec::new(),
            rows,
            cursor: None,
            offset: 0,
            filter: String::new(),
            quit: false,
            selected_path: None,
            color: term::color_enabled(term::is_stdout_tty()),
        };
        app.refilter();
        app
    }

    fn fg(&self, color: Color) -> Style {
        if self.color {
            Style::new().fg(color)
        } else {
            Style::new()
        }
    }

    fn row(&self, i: usize) -> &Row {
        &self.rows[self.filtered[i]]
    }

    fn selected_row(&self) -> Option<&Row> {
        self.cursor.map(|i| self.row(i))
    }

    fn group_start(&self, mut i: usize) -> usize {
        while i > 0 && self.row(i - 1).repo == self.row(i).repo {
            i -= 1;
        }
        i
    }

    // Clearing the filter keeps the highlighted worktree; typing moves to the best match.
    fn refilter(&mut self) {
        let kept = self.cursor.map(|i| self.filtered[i]);
        if self.filter.is_empty() {
            self.filtered = (0..self.rows.len()).collect();
            self.cursor = kept
                .or_else(|| self.rows.iter().position(|row| row.wt.current))
                .or((!self.rows.is_empty()).then_some(0));
            self.snap();
            return;
        }

        let mut scored: Vec<(usize, usize)> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                fuzzy::filter_score(&self.filter, &row.wt.filter_candidate).map(|s| (i, s))
            })
            .collect();
        let mut best = vec![usize::MAX; self.repos.len()];
        for &(i, score) in &scored {
            let repo = self.rows[i].repo;
            best[repo] = best[repo].min(score);
        }
        scored.sort_by_key(|&(i, score)| {
            let repo = self.rows[i].repo;
            (best[repo], repo, score, i)
        });
        self.filtered = scored.into_iter().map(|(i, _)| i).collect();
        self.cursor = (!self.filtered.is_empty()).then_some(0);
        self.offset = 0;
    }

    // Jumps put the cursor's repo at the top of the view.
    fn snap(&mut self) {
        let Some(cursor) = self.cursor else {
            self.offset = 0;
            return;
        };
        self.offset = self
            .group_start(cursor)
            .min(self.filtered.len().saturating_sub(MAX_ROWS));
        if cursor >= self.offset + MAX_ROWS {
            self.scroll();
        }
    }

    // Moves within the list scroll as little as possible, keeping a row of context.
    fn scroll(&mut self) {
        let len = self.filtered.len();
        let Some(cursor) = self.cursor.filter(|_| len > MAX_ROWS) else {
            self.offset = 0;
            return;
        };
        if cursor < self.offset + 1 {
            self.offset = cursor.saturating_sub(1);
        }
        if cursor + 2 > self.offset + MAX_ROWS {
            self.offset = (cursor + 2 - MAX_ROWS).min(len - MAX_ROWS);
        }
    }

    fn move_cursor(&mut self, delta: isize) {
        let Some(cursor) = self.cursor else { return };
        let len = self.filtered.len() as isize;
        self.cursor = Some((cursor as isize + delta).rem_euclid(len) as usize);
        self.scroll();
    }

    fn move_to_end(&mut self, last: bool) {
        if self.cursor.is_none() {
            return;
        }
        self.cursor = Some(if last { self.filtered.len() - 1 } else { 0 });
        self.scroll();
    }

    fn page(&mut self, delta: isize) {
        let Some(cursor) = self.cursor else { return };
        let target = cursor.saturating_add_signed(delta * MAX_ROWS as isize);
        self.cursor = Some(target.min(self.filtered.len() - 1));
        self.scroll();
    }

    // Back goes to the top of the current repo before the previous one, like vim's `{`.
    fn jump_repo(&mut self, forward: bool) {
        let Some(cursor) = self.cursor else { return };
        let len = self.filtered.len();
        let start = self.group_start(cursor);
        let target = if forward {
            (cursor..len)
                .find(|&i| self.row(i).repo != self.row(cursor).repo)
                .unwrap_or(0)
        } else if cursor != start {
            start
        } else {
            self.group_start((start + len - 1) % len)
        };
        self.cursor = Some(target);
        self.snap();
    }
}

fn handle_key(app: &mut App, key: event::KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') if app.filter.is_empty() => app.quit = true,
            KeyCode::Char('c' | 'u') if !app.filter.is_empty() => {
                app.filter.clear();
                app.refilter();
            }
            KeyCode::Char('d') => app.quit = true,
            KeyCode::Char('n') => app.move_cursor(1),
            KeyCode::Char('p') => app.move_cursor(-1),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Esc if app.filter.is_empty() => app.quit = true,
        KeyCode::Esc => {
            app.filter.clear();
            app.refilter();
        }
        KeyCode::Enter => {
            app.selected_path = app.selected_row().map(|row| row.wt.path.clone());
            app.quit = true;
        }
        KeyCode::Up => app.move_cursor(-1),
        KeyCode::Down => app.move_cursor(1),
        KeyCode::Left | KeyCode::BackTab => app.jump_repo(false),
        KeyCode::Right | KeyCode::Tab => app.jump_repo(true),
        KeyCode::PageUp => app.page(-1),
        KeyCode::PageDown => app.page(1),
        KeyCode::Home => app.move_to_end(false),
        KeyCode::End => app.move_to_end(true),
        KeyCode::Backspace if !app.filter.is_empty() => {
            app.filter.pop();
            app.refilter();
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::ALT) => {
            app.filter.push(c);
            app.refilter();
        }
        _ => {}
    }
}

fn worktree_column_widths<'a>(
    wts: impl Iterator<Item = &'a WorktreeData>,
) -> (usize, usize, usize) {
    let (mut max_branch, mut max_status, mut badge_w) = (0usize, 0usize, 0usize);
    for wt in wts {
        max_branch = max_branch.max(display_width(wt.display_branch()));
        max_status = max_status.max(display_width(&wt.status));
        if wt.locked {
            badge_w = LOCK_BADGE.len();
        }
    }
    let branch_w = max_branch.clamp(4, 40) + 2;
    let status_w = if max_status > 0 { max_status + 2 } else { 0 };
    (branch_w, status_w, badge_w)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Columns {
    name: usize,
    count: usize,
    branch: usize,
    status: usize,
    badge: usize,
}

impl Columns {
    fn width(&self) -> usize {
        let repo = if self.name > 0 {
            self.name + self.count + 2
        } else {
            0
        };
        repo + self.branch + self.status + self.badge
    }
}

// Widths come from every row, so columns don't shift while filtering. When the
// terminal is too narrow, columns give way in order of least use for picking.
fn columns(app: &App, width: u16) -> Columns {
    let mut repo_sizes = vec![0usize; app.repos.len()];
    for row in &app.rows {
        repo_sizes[row.repo] += 1;
    }
    let multi = app.repos.len() > 1;
    let name_w = if multi {
        app.repos
            .iter()
            .map(|n| display_width(n))
            .max()
            .unwrap_or(0)
    } else {
        0
    };
    let count_w = if multi {
        repo_sizes
            .iter()
            .map(|n| format!(" ({n})").len())
            .max()
            .unwrap_or(0)
    } else {
        0
    };
    let (branch_w, status_w, badge_w) = worktree_column_widths(app.rows.iter().map(|r| &r.wt));
    let mut c = Columns {
        name: name_w,
        count: count_w,
        branch: branch_w,
        status: status_w,
        badge: badge_w,
    };

    let avail = usize::from(width).saturating_sub(2);
    let over = |c: &Columns| c.width().saturating_sub(avail);
    if over(&c) > 0 {
        c.badge = 0;
    }
    if over(&c) > 0 {
        c.count = 0;
    }
    if over(&c) > 0 {
        c.name = c.name.saturating_sub(over(&c)).max(REPO_MIN.min(name_w));
    }
    if over(&c) > 0 {
        c.branch = c
            .branch
            .saturating_sub(over(&c))
            .max(BRANCH_MIN.min(branch_w));
    }
    if over(&c) > 0 {
        c.status = 0;
        let rest = avail.saturating_sub(Columns { branch: 0, ..c }.width());
        c.branch = branch_w.min(rest).max(1);
    }
    c
}

fn viewport_height(app: &App) -> u16 {
    app.rows.len().clamp(1, MAX_ROWS) as u16 + 2
}

fn row_lines(app: &App, width: u16, height: usize) -> Vec<Line<'static>> {
    let c = columns(app, width);
    let mut counts = vec![0usize; app.repos.len()];
    for &i in &app.filtered {
        counts[app.rows[i].repo] += 1;
    }
    let branch_text_w = if c.status > 0 || c.badge > 0 {
        c.branch.saturating_sub(2)
    } else {
        c.branch
    };
    // A terminal shorter than the picker gets fewer rows than MAX_ROWS.
    let offset = app.cursor.map_or(app.offset, |cursor| {
        app.offset.max((cursor + 1).saturating_sub(height))
    });

    (offset..app.filtered.len().min(offset + height))
        .map(|i| {
            let row = app.row(i);
            let selected = app.cursor == Some(i);
            let mut spans = vec![Span::raw(if selected { "› " } else { "  " })];
            if c.name > 0 {
                let label = i == offset || app.row(i - 1).repo != row.repo;
                let (name, count) = if label {
                    let count = if c.count > 0 {
                        format!(" ({})", counts[row.repo])
                    } else {
                        String::new()
                    };
                    (trunc(&app.repos[row.repo], c.name), count)
                } else {
                    (String::new(), String::new())
                };
                let gap = c.name + c.count + 2 - display_width(&name) - display_width(&count);
                spans.push(Span::raw(name));
                spans.push(Span::styled(count, Style::new().dim()));
                spans.push(Span::raw(" ".repeat(gap)));
            }

            let branch_style = if row.wt.current {
                app.fg(Color::Green)
            } else if row.wt.detached {
                app.fg(Color::Yellow)
            } else {
                Style::new()
            };
            spans.push(Span::styled(
                term::pad(&trunc(row.wt.display_branch(), branch_text_w), c.branch),
                branch_style,
            ));
            if c.status > 0 {
                let style = if row.wt.dirty {
                    app.fg(Color::Yellow)
                } else {
                    Style::new().dim()
                };
                spans.push(Span::styled(term::pad(&row.wt.status, c.status), style));
            }
            if c.badge > 0 && row.wt.locked {
                spans.push(Span::styled(LOCK_BADGE, app.fg(Color::Yellow)));
            }

            let line = Line::from(spans);
            if selected {
                line.patch_style(app.fg(Color::Cyan).bold())
            } else {
                line
            }
        })
        .collect()
}

fn render(frame: &mut Frame, app: &App) {
    let [rows_area, detail_area, footer_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    if app.filtered.is_empty() {
        frame.render_widget(EMPTY_HINT.dim(), rows_area);
    } else {
        let lines = row_lines(app, rows_area.width, rows_area.height.into());
        frame.render_widget(Paragraph::new(lines), rows_area);
    }
    if let Some(row) = app.selected_row() {
        let path = trunc_tail(&row.wt.display_path, detail_area.width.into());
        frame.render_widget(path.dim(), detail_area);
    }
    frame.render_widget(
        footer_line(app, footer_area.width, rows_area.height),
        footer_area,
    );
}

fn footer_line(app: &App, width: u16, visible_rows: u16) -> Line<'static> {
    let w = usize::from(width);
    let spans_w =
        |spans: &[Span]| -> usize { spans.iter().map(|s| display_width(&s.content)).sum() };

    if !app.filter.is_empty() {
        let prefix = "/ ";
        let count = match app.filtered.len() {
            1 => " · 1 match".to_owned(),
            n => format!(" · {n} matches"),
        };
        let fixed = display_width(prefix) + display_width(&count);
        if w > fixed {
            let query = trunc_tail(&app.filter, w - fixed);
            return Line::from(vec![prefix.dim(), Span::raw(query), count.dim()]);
        }
        let query = trunc_tail(&app.filter, w.saturating_sub(display_width(prefix)));
        return Line::from(vec![prefix.dim(), Span::raw(query)]);
    }

    let base = vec![
        Span::raw("enter"),
        " select".dim(),
        SEP.dim(),
        Span::raw("esc"),
        " quit".dim(),
    ];
    if w < spans_w(&base) {
        return Line::default();
    }
    let repo_tier = if app.repos.len() > 1 {
        vec![SEP.dim(), Span::raw("←→"), " repo".dim()]
    } else {
        Vec::new()
    };
    let filter_tier = vec![SEP.dim(), "type to filter".dim()];
    let position_tier = match app.cursor {
        Some(i) if app.filtered.len() > usize::from(visible_rows) => {
            vec![SEP.dim(), format!("{}/{}", i + 1, app.filtered.len()).dim()]
        }
        _ => Vec::new(),
    };

    let mut spans = base;
    for tier in [repo_tier, filter_tier, position_tier] {
        if !tier.is_empty() && w >= spans_w(&spans) + spans_w(&tier) {
            spans.extend(tier);
        }
    }
    Line::from(spans)
}

fn build_repos(infos: Vec<worktree::RepoInfo>) -> Vec<RepoData> {
    infos
        .into_iter()
        .filter_map(|repo| {
            let name = repo.name;
            let mut infos: Vec<_> = repo
                .worktrees
                .iter()
                .filter(|info| !info.worktree.bare && !info.worktree.prunable)
                .collect();
            infos.sort_by(|a, b| a.worktree.display_order().cmp(&b.worktree.display_order()));
            let worktrees: Vec<WorktreeData> = infos
                .into_iter()
                .map(|info| WorktreeData::from_info(info, &name))
                .collect();
            if worktrees.is_empty() {
                return None;
            }
            Some(RepoData { name, worktrees })
        })
        .collect()
}

fn event_loop(terminal: &mut crate::tui::StdoutTerminal, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| render(frame, app))?;
        loop {
            match event::read()? {
                Event::Key(key) => {
                    handle_key(app, key);
                    break;
                }
                Event::Resize(..) => break,
                _ => {}
            }
        }
        if app.quit {
            break;
        }
    }
    Ok(())
}

pub fn run() -> Result<(), String> {
    if !term::is_stdout_tty() {
        return Err(
            "cannot launch picker, stdout is not a terminal, use `wt list` in scripts".into(),
        );
    }

    let repos = build_repos(worktree::load_all(&layout::worktrees_root()?));
    if repos.is_empty() {
        return Err(
            "no worktrees, use `wt clone <url>`, or `wt switch <branch>` inside a repository"
                .into(),
        );
    }
    let mut app = App::new(repos);
    let height = viewport_height(&app);

    crate::tui::run(height, |terminal| event_loop(terminal, &mut app))
        .map_err(|e| format!("cannot run picker: {e}"))?;

    let Some(path) = app.selected_path else {
        return Ok(());
    };
    let path = layout::canonicalize_or_self(&path);
    match std::env::var_os("__WT_CD") {
        Some(cd_file) => std::fs::write(&cd_file, path.as_os_str().as_encoded_bytes())
            .map_err(|e| format!("cannot write {}: {e}", Path::new(&cd_file).display())),
        None => {
            println!("{}", path.display());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;

    use super::*;

    fn info(worktree: worktree::Worktree, status: worktree::Status) -> worktree::WorktreeInfo {
        worktree::WorktreeInfo {
            worktree,
            status,
            current: false,
        }
    }

    fn wt(repo: &str, branch: &str) -> WorktreeData {
        WorktreeData {
            path: PathBuf::from(format!("/wt/{repo}/{branch}")),
            display_path: format!("/wt/{repo}/{branch}"),
            branch: Some(branch.into()),
            filter_candidate: format!("{repo} {branch}"),
            ..Default::default()
        }
    }

    fn repo(name: &str, branches: &[&str]) -> RepoData {
        RepoData {
            name: name.into(),
            worktrees: branches.iter().map(|b| wt(name, b)).collect(),
        }
    }

    fn test_repos() -> Vec<RepoData> {
        let mut my_app = repo("my-app", &["main", "feat/login"]);
        my_app.worktrees[1].status = "* ↑2".into();
        my_app.worktrees[1].dirty = true;
        let mut other = repo("other-repo", &["main"]);
        other.worktrees[0].status = "↓1".into();
        vec![my_app, other]
    }

    // Three repos of 5 worktrees each: 15 rows, more than fit on screen.
    fn long_repos() -> Vec<RepoData> {
        ["alpha", "beta", "gamma"]
            .iter()
            .map(|name| repo(name, &["main", "b1", "b2", "b3", "b4"]))
            .collect()
    }

    fn press(app: &mut App, code: KeyCode) {
        handle_key(app, event::KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn ctrl(app: &mut App, c: char) {
        handle_key(
            app,
            event::KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL),
        );
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    fn selected_branch(app: &App) -> Option<&str> {
        app.selected_row().and_then(|r| r.wt.branch.as_deref())
    }

    fn selected_repo(app: &App) -> &str {
        &app.repos[app.selected_row().unwrap().repo]
    }

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn screen(app: &App, width: u16, height: u16) -> Vec<String> {
        let mut terminal = ratatui::Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn app_initial_state() {
        let app = App::new(test_repos());
        assert_eq!(app.filtered, vec![0, 1, 2]);
        assert_eq!(app.cursor, Some(0));
        assert_eq!(app.offset, 0);
        assert!(app.filter.is_empty());
    }

    #[test]
    fn empty_repos_does_not_panic() {
        let mut app = App::new(Vec::new());
        assert_eq!(app.cursor, None);
        for code in [
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::PageDown,
        ] {
            press(&mut app, code);
        }
        press(&mut app, KeyCode::Enter);
        assert!(app.quit);
        assert_eq!(app.selected_path, None);
    }

    #[test]
    fn pre_selects_current_worktree() {
        let mut repos = test_repos();
        repos[1].worktrees[0].current = true;
        let app = App::new(repos);
        assert_eq!(selected_repo(&app), "other-repo");
    }

    #[test]
    fn opens_with_current_repo_at_top() {
        let mut repos = long_repos();
        repos[1].worktrees[3].current = true;
        let app = App::new(repos);
        assert_eq!(selected_branch(&app), Some("b3"));
        assert_eq!(app.offset, 5, "beta's first row should be at the top");
    }

    #[test]
    fn arrows_move_and_wrap() {
        let mut app = App::new(test_repos());
        press(&mut app, KeyCode::Down);
        assert_eq!(selected_branch(&app), Some("feat/login"));
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.cursor, Some(0));
        press(&mut app, KeyCode::Up);
        assert_eq!(app.cursor, Some(2));
    }

    #[test]
    fn ctrl_n_and_ctrl_p_move() {
        let mut app = App::new(test_repos());
        ctrl(&mut app, 'n');
        assert_eq!(app.cursor, Some(1));
        ctrl(&mut app, 'p');
        assert_eq!(app.cursor, Some(0));
        assert!(app.filter.is_empty());
    }

    #[test]
    fn arrows_scroll_minimally_keeping_a_row_of_context() {
        let mut app = App::new(long_repos());
        for _ in 0..6 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.cursor, Some(6));
        assert_eq!(app.offset, 0, "row 6 plus one row of context still fits");
        press(&mut app, KeyCode::Down);
        assert_eq!(app.offset, 1);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.offset, 1, "moving up inside the view does not scroll");
    }

    #[test]
    fn right_jumps_to_next_repo_and_wraps() {
        let mut app = App::new(long_repos());
        press(&mut app, KeyCode::Right);
        assert_eq!(
            (selected_repo(&app), selected_branch(&app)),
            ("beta", Some("main"))
        );
        press(&mut app, KeyCode::Right);
        assert_eq!(selected_repo(&app), "gamma");
        press(&mut app, KeyCode::Right);
        assert_eq!(selected_repo(&app), "alpha");
    }

    #[test]
    fn left_goes_to_repo_top_then_previous_repo() {
        let mut app = App::new(long_repos());
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Left);
        assert_eq!(
            (selected_repo(&app), selected_branch(&app)),
            ("beta", Some("main"))
        );
        press(&mut app, KeyCode::Left);
        assert_eq!(
            (selected_repo(&app), selected_branch(&app)),
            ("alpha", Some("main"))
        );
        press(&mut app, KeyCode::Left);
        assert_eq!(selected_repo(&app), "gamma", "wraps to the last repo");
    }

    #[test]
    fn tab_and_backtab_jump_like_arrows() {
        let mut app = App::new(long_repos());
        press(&mut app, KeyCode::Tab);
        assert_eq!(selected_repo(&app), "beta");
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
        assert_eq!(selected_repo(&app), "alpha");
    }

    #[test]
    fn jumps_put_the_repo_at_the_top_of_the_view() {
        let mut app = App::new(long_repos());
        press(&mut app, KeyCode::Right);
        assert_eq!(app.offset, 5);
        press(&mut app, KeyCode::Right);
        assert_eq!(app.offset, 7, "the last repo can't scroll past the end");
    }

    #[test]
    fn jump_into_single_repo_is_noop() {
        let mut app = App::new(vec![repo("solo", &["main", "feat"])]);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Right);
        assert_eq!(selected_branch(&app), Some("main"), "wraps to its own top");
        press(&mut app, KeyCode::Left);
        assert_eq!(selected_branch(&app), Some("main"));
    }

    #[test]
    fn page_keys_move_by_a_page_and_clamp() {
        let mut app = App::new(long_repos());
        press(&mut app, KeyCode::PageDown);
        assert_eq!(app.cursor, Some(MAX_ROWS));
        press(&mut app, KeyCode::PageDown);
        assert_eq!(app.cursor, Some(14));
        assert_eq!(app.offset, 7);
        press(&mut app, KeyCode::PageUp);
        press(&mut app, KeyCode::PageUp);
        assert_eq!(app.cursor, Some(0));
        assert_eq!(app.offset, 0);
    }

    #[test]
    fn home_and_end_jump_to_first_and_last_rows() {
        let mut app = App::new(long_repos());
        press(&mut app, KeyCode::End);
        assert_eq!(app.cursor, Some(14));
        assert_eq!(app.offset, 7);
        press(&mut app, KeyCode::Home);
        assert_eq!(app.cursor, Some(0));
        assert_eq!(app.offset, 0);
    }

    #[test]
    fn same_named_repos_stay_separate_groups() {
        let app = App::new(vec![repo("api", &["main"]), repo("api", &["main"])]);
        let lines = screen(&app, 40, 4);
        assert!(lines[0].contains("api"));
        assert!(lines[1].contains("api"), "second repo keeps its own label");
    }

    #[test]
    fn enter_selects_and_quits() {
        let mut app = App::new(test_repos());
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert!(app.quit);
        assert_eq!(
            app.selected_path,
            Some(PathBuf::from("/wt/my-app/feat/login"))
        );
    }

    #[test]
    fn enter_quits_when_no_results() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "zzz");
        press(&mut app, KeyCode::Enter);
        assert!(app.quit);
        assert_eq!(app.selected_path, None);
    }

    #[test]
    fn esc_quits_when_filter_empty() {
        let mut app = App::new(test_repos());
        press(&mut app, KeyCode::Esc);
        assert!(app.quit);
        assert_eq!(app.selected_path, None);
    }

    #[test]
    fn esc_clears_filter_when_nonempty() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "ot");
        press(&mut app, KeyCode::Esc);
        assert!(!app.quit);
        assert!(app.filter.is_empty());
        assert_eq!(app.filtered.len(), 3);
    }

    #[test]
    fn ctrl_c_quits_when_filter_empty() {
        let mut app = App::new(test_repos());
        ctrl(&mut app, 'c');
        assert!(app.quit);
    }

    #[test]
    fn ctrl_c_clears_filter_when_nonempty() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "ot");
        ctrl(&mut app, 'c');
        assert!(!app.quit);
        assert!(app.filter.is_empty());
    }

    #[test]
    fn ctrl_d_quits() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "ot");
        ctrl(&mut app, 'd');
        assert!(app.quit);
    }

    #[test]
    fn ctrl_u_clears_filter() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "ot");
        ctrl(&mut app, 'u');
        assert!(!app.quit);
        assert!(app.filter.is_empty());
    }

    #[test]
    fn ctrl_u_noop_when_filter_empty() {
        let mut app = App::new(long_repos());
        press(&mut app, KeyCode::Down);
        ctrl(&mut app, 'u');
        assert!(!app.quit);
        assert_eq!(app.cursor, Some(1));
    }

    #[test]
    fn clearing_filter_keeps_highlighted_worktree() {
        let mut app = App::new(long_repos());
        type_str(&mut app, "gamma b2");
        assert_eq!(
            (selected_repo(&app), selected_branch(&app)),
            ("gamma", Some("b2"))
        );
        press(&mut app, KeyCode::Esc);
        assert_eq!(
            (selected_repo(&app), selected_branch(&app)),
            ("gamma", Some("b2"))
        );
        assert_eq!(app.offset, 7, "its repo moves to the top");
    }

    #[test]
    fn clearing_filter_without_match_returns_to_current() {
        let mut repos = test_repos();
        repos[0].worktrees[1].current = true;
        let mut app = App::new(repos);
        type_str(&mut app, "zzz");
        assert_eq!(app.cursor, None);
        press(&mut app, KeyCode::Esc);
        assert_eq!(selected_branch(&app), Some("feat/login"));
    }

    #[test]
    fn typing_filters_and_selects_best_match() {
        let mut app = App::new(test_repos());
        press(&mut app, KeyCode::Down);
        type_str(&mut app, "login");
        assert_eq!(app.filtered.len(), 1);
        assert_eq!(app.cursor, Some(0));
        assert_eq!(selected_branch(&app), Some("feat/login"));
    }

    #[test]
    fn filter_matches_across_repo_and_branch() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "other main");
        assert_eq!(app.filtered.len(), 1);
        assert_eq!(selected_repo(&app), "other-repo");
    }

    #[test]
    fn filter_groups_rows_by_repo_with_best_repo_first() {
        let mut app = App::new(vec![
            repo("alpha", &["main", "fix/a"]),
            repo("beta", &["fix"]),
        ]);
        type_str(&mut app, "fix");
        let order: Vec<_> = (0..app.filtered.len())
            .map(|i| {
                (
                    app.repos[app.row(i).repo].as_str(),
                    app.row(i).wt.branch.as_deref(),
                )
            })
            .collect();
        assert_eq!(order, [("beta", Some("fix")), ("alpha", Some("fix/a"))]);
    }

    #[test]
    fn modified_chars_do_not_edit_filter() {
        let mut app = App::new(test_repos());
        for modifiers in [KeyModifiers::CONTROL, KeyModifiers::ALT] {
            handle_key(
                &mut app,
                event::KeyEvent::new(KeyCode::Char('w'), modifiers),
            );
        }
        assert!(app.filter.is_empty());
    }

    #[test]
    fn backspace_removes_filter_char() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "ot");
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.filter, "o");
    }

    #[test]
    fn backspace_on_empty_filter_is_noop() {
        let mut app = App::new(long_repos());
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.cursor, Some(1));
    }

    #[test]
    fn viewport_height_caps_at_10() {
        assert_eq!(viewport_height(&App::new(long_repos())), 10);
    }

    #[test]
    fn viewport_height_fits_content() {
        assert_eq!(viewport_height(&App::new(test_repos())), 5);
    }

    #[test]
    fn columns_fit_content_when_wide() {
        let mut repos = test_repos();
        repos[0].worktrees[0].locked = true;
        let app = App::new(repos);
        assert_eq!(
            columns(&app, 80),
            Columns {
                name: 10,
                count: 4,
                branch: 12,
                status: 6,
                badge: 4,
            }
        );
    }

    #[test]
    fn columns_give_way_in_order_as_the_terminal_narrows() {
        let mut repos = vec![
            repo("billing-service", &["main", "feat/graphql-subscriptions"]),
            repo("wt", &["main"]),
        ];
        repos[0].worktrees[1].status = "↑7 ↓2".into();
        repos[0].worktrees[1].locked = true;
        let app = App::new(repos);
        let wide = columns(&app, 80);
        assert_eq!(
            (wide.name, wide.count, wide.branch, wide.status, wide.badge),
            (15, 4, 28, 7, 4)
        );

        let c = columns(&app, 56);
        assert_eq!(
            (c.badge, c.count, c.name),
            (0, 0, 15),
            "badge, then counts go first"
        );
        let c = columns(&app, 48);
        assert_eq!(
            (c.name, c.branch, c.status),
            (9, 28, 7),
            "repo names shorten next"
        );
        let c = columns(&app, 40);
        assert_eq!((c.name, c.branch, c.status), (8, 21, 7), "then branches");
        let c = columns(&app, 32);
        assert_eq!(
            (c.name, c.branch, c.status),
            (8, 20, 0),
            "status goes last, branch takes its room"
        );
    }

    #[test]
    fn single_repo_hides_repo_column() {
        let app = App::new(vec![repo("solo", &["main", "feat"])]);
        assert_eq!(columns(&app, 80).name, 0);
        let lines = screen(&app, 40, 4);
        assert_eq!(lines[0], "› main");
    }

    #[test]
    fn repo_label_and_count_show_on_first_row_of_each_group() {
        let app = App::new(test_repos());
        let lines = screen(&app, 60, 5);
        assert_eq!(lines[0], "› my-app (2)      main");
        assert_eq!(lines[1], "                  feat/login  * ↑2");
        assert_eq!(lines[2], "  other-repo (1)  main        ↓1");
    }

    #[test]
    fn counts_show_matches_while_filtering() {
        let mut app = App::new(long_repos());
        type_str(&mut app, "beta b");
        let lines = screen(&app, 40, 10);
        assert!(lines[0].starts_with("› beta (4)"), "{lines:?}");
    }

    #[test]
    fn repo_label_sticks_to_top_row_when_scrolled() {
        let mut app = App::new(long_repos());
        for _ in 0..8 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.offset, 2);
        let lines = screen(&app, 40, 10);
        assert!(lines[0].starts_with("  alpha (5)"), "{lines:?}");
        assert!(lines[3].starts_with("  beta (5)"), "{lines:?}");
    }

    #[test]
    fn short_terminal_keeps_cursor_visible() {
        let mut app = App::new(long_repos());
        for _ in 0..5 {
            press(&mut app, KeyCode::Down);
        }
        let lines = screen(&app, 40, 5);
        assert!(lines[2].starts_with("› beta (5)"), "{lines:?}");
    }

    #[test]
    fn current_and_detached_branches_are_colored() {
        let mut repos = test_repos();
        repos[0].worktrees[0].current = true;
        repos[1].worktrees[0].branch = None;
        repos[1].worktrees[0].detached = true;
        let mut app = App::new(repos);
        app.color = true;
        let lines = row_lines(&app, 60, 8);
        let branch_fg = |line: &Line| line.spans[4].style.fg;
        assert_eq!(branch_fg(&lines[0]), Some(Color::Green));
        assert_eq!(branch_fg(&lines[2]), Some(Color::Yellow));
        assert_eq!(lines[2].spans[4].content.trim(), "(detached)");
    }

    #[test]
    fn selected_row_is_highlighted() {
        let mut app = App::new(test_repos());
        app.color = true;
        let lines = row_lines(&app, 60, 8);
        assert_eq!(lines[0].style.fg, Some(Color::Cyan));
        assert_eq!(lines[1].style.fg, None);
    }

    #[test]
    fn render_truncates_long_branches_in_narrow_terminal() {
        let app = App::new(vec![repo(
            "repo",
            &["feature/a-very-long-branch-name-that-keeps-going"],
        )]);
        let lines = screen(&app, 20, 3);
        assert_eq!(lines[0], "› feature/a-very-lo…");
    }

    #[test]
    fn render_empty_filter_shows_hint() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "zzzzzzz");
        let lines = screen(&app, 60, 5);
        assert!(lines[0].contains("no matches"), "{lines:?}");
    }

    #[test]
    fn render_detail_shows_selected_path() {
        let app = App::new(test_repos());
        let lines = screen(&app, 60, 5);
        assert_eq!(lines[3], "/wt/my-app/main");
    }

    #[test]
    fn footer_full_width_shows_all_hints() {
        let app = App::new(test_repos());
        let text = line_text(&footer_line(&app, 80, 3));
        assert_eq!(text, "enter select · esc quit · ←→ repo · type to filter");
    }

    #[test]
    fn footer_drops_filter_hint_before_repo_hint() {
        let app = App::new(test_repos());
        assert_eq!(
            line_text(&footer_line(&app, 40, 3)),
            "enter select · esc quit · ←→ repo"
        );
        assert_eq!(
            line_text(&footer_line(&app, 30, 3)),
            "enter select · esc quit"
        );
        assert_eq!(line_text(&footer_line(&app, 10, 3)), "");
    }

    #[test]
    fn footer_hides_repo_hint_for_single_repo() {
        let app = App::new(vec![repo("solo", &["main"])]);
        assert!(!line_text(&footer_line(&app, 80, 1)).contains("repo"));
    }

    #[test]
    fn footer_shows_position_when_scrollable() {
        let mut app = App::new(long_repos());
        press(&mut app, KeyCode::Down);
        let text = line_text(&footer_line(&app, 80, 8));
        assert!(text.ends_with(" · 2/15"), "{text}");
        let app = App::new(test_repos());
        assert!(!line_text(&footer_line(&app, 80, 3)).contains("1/3"));
    }

    #[test]
    fn footer_filter_shows_text_and_match_count() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "ma");
        assert_eq!(line_text(&footer_line(&app, 80, 3)), "/ ma · 3 matches");
        let mut app = App::new(test_repos());
        type_str(&mut app, "login");
        assert_eq!(line_text(&footer_line(&app, 80, 3)), "/ login · 1 match");
    }

    #[test]
    fn footer_filter_hides_count_at_narrow_width() {
        let mut app = App::new(test_repos());
        type_str(&mut app, "main");
        assert_eq!(line_text(&footer_line(&app, 10, 3)), "/ main");
    }

    #[test]
    fn footer_width_counts_display_columns_not_bytes() {
        let app = App::new(test_repos());
        let text = line_text(&footer_line(&app, 33, 3));
        assert_eq!(display_width(&text), 33, "{text}");
    }

    #[test]
    fn worktree_column_widths_empty_status() {
        let wts = [wt("r", "main")];
        let (branch_w, status_w, badge_w) = worktree_column_widths(wts.iter());
        assert_eq!((branch_w, status_w, badge_w), (6, 0, 0));
    }

    #[test]
    fn worktree_column_widths_branch_clamped() {
        let long = [wt("r", &"x".repeat(60))];
        assert_eq!(worktree_column_widths(long.iter()).0, 42);
        let short = [wt("r", "ab")];
        assert_eq!(worktree_column_widths(short.iter()).0, 6);
    }

    #[test]
    fn from_info_dirty() {
        let info = info(
            worktree::Worktree {
                path: PathBuf::from("/wt/repo/feat"),
                branch: Some("feat".into()),
                ..Default::default()
            },
            worktree::Status {
                dirty: true,
                ..Default::default()
            },
        );
        let data = WorktreeData::from_info(&info, "repo");
        assert!(data.dirty);
        assert_eq!(data.filter_candidate, "repo feat");
    }

    #[test]
    fn from_info_detached_no_branch_candidate() {
        let info = info(
            worktree::Worktree {
                path: PathBuf::from("/wt/repo/detached"),
                detached: true,
                ..Default::default()
            },
            worktree::Status::default(),
        );
        let data = WorktreeData::from_info(&info, "repo");
        assert_eq!(data.filter_candidate, "repo");
        assert_eq!(data.display_branch(), "(detached)");
    }

    #[test]
    fn from_info_clean_status_is_empty() {
        let info = info(
            worktree::Worktree {
                path: PathBuf::from("/wt/repo/main"),
                branch: Some("main".into()),
                ..Default::default()
            },
            worktree::Status::default(),
        );
        let data = WorktreeData::from_info(&info, "repo");
        assert!(data.status.is_empty());
    }

    #[test]
    fn from_info_ahead_behind() {
        let info = info(
            worktree::Worktree {
                path: PathBuf::from("/wt/repo/feat"),
                branch: Some("feat".into()),
                ..Default::default()
            },
            worktree::Status {
                dirty: true,
                ahead: Some(3),
                behind: Some(1),
            },
        );
        let data = WorktreeData::from_info(&info, "repo");
        assert_eq!(data.status, "* \u{2191}3 \u{2193}1");
    }

    #[test]
    fn from_info_locked() {
        let info = info(
            worktree::Worktree {
                path: PathBuf::from("/wt/repo/feat"),
                branch: Some("feat".into()),
                locked: true,
                ..Default::default()
            },
            worktree::Status::default(),
        );
        let data = WorktreeData::from_info(&info, "repo");
        assert!(data.locked);
    }

    #[test]
    fn build_repos_filters_bare_prunable_and_sorts() {
        let infos = vec![worktree::RepoInfo {
            name: "repo".into(),
            worktrees: vec![
                info(
                    worktree::Worktree {
                        path: PathBuf::from("/admin"),
                        branch: Some("main".into()),
                        bare: true,
                        ..Default::default()
                    },
                    worktree::Status::default(),
                ),
                info(
                    worktree::Worktree {
                        path: PathBuf::from("/wt/stale"),
                        branch: Some("stale-branch".into()),
                        prunable: true,
                        ..Default::default()
                    },
                    worktree::Status::default(),
                ),
                info(
                    worktree::Worktree {
                        path: PathBuf::from("/wt/feat"),
                        branch: Some("feat/z".into()),
                        ..Default::default()
                    },
                    worktree::Status::default(),
                ),
                info(
                    worktree::Worktree {
                        path: PathBuf::from("/wt/main"),
                        branch: Some("main".into()),
                        ..Default::default()
                    },
                    worktree::Status::default(),
                ),
            ],
        }];

        let repos = build_repos(infos);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].worktrees.len(), 2);
        assert_eq!(repos[0].worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(repos[0].worktrees[1].branch.as_deref(), Some("feat/z"));
    }

    #[test]
    fn build_repos_drops_empty() {
        let infos = vec![worktree::RepoInfo {
            name: "bare-only".into(),
            worktrees: vec![info(
                worktree::Worktree {
                    path: PathBuf::from("/admin"),
                    bare: true,
                    ..Default::default()
                },
                worktree::Status::default(),
            )],
        }];

        assert!(build_repos(infos).is_empty());
    }
}
