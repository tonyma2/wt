use std::io;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{HighlightSpacing, List, ListItem, ListState};

use crate::fuzzy;
use crate::git::Git;
use crate::terminal::{self as term, trunc, trunc_tail};
use crate::worktree;

const EMPTY_HINT: &str = "no matches · backspace to edit";

struct RepoData {
    name: String,
    worktrees: Vec<WorktreeData>,
}

#[derive(Clone, Default)]
struct WorktreeData {
    path: PathBuf,
    display_path: String,
    branch: Option<String>,
    filter_candidate: String,
    status: String,
    status_color: Option<Color>,
    detached: bool,
    locked: bool,
    prunable: bool,
    current: bool,
}

impl WorktreeData {
    fn from_worktree(
        wt: &worktree::Worktree,
        git: &Git,
        repo_name: &str,
        cwd: Option<&std::path::Path>,
    ) -> Self {
        let (dirty, ahead, behind) = worktree::computed_status(git, wt);
        let status = worktree::format_status(false, dirty, ahead, behind);
        let status_color = if dirty {
            Some(Color::Yellow)
        } else if ahead.is_some_and(|a| a > 0) || behind.is_some_and(|b| b > 0) {
            Some(Color::Cyan)
        } else {
            None
        };
        let filter_candidate = match &wt.branch {
            Some(b) => format!("{repo_name} {b}"),
            None => repo_name.to_owned(),
        };
        WorktreeData {
            display_path: term::tilde_path(&wt.path),
            path: wt.path.clone(),
            branch: wt.branch.clone(),
            filter_candidate,
            status,
            status_color,
            detached: wt.detached,
            locked: wt.locked,
            prunable: wt.prunable,
            current: cwd.is_some_and(|c| worktree::is_cwd_inside(&wt.path, Some(c))),
        }
    }

    fn sort_key(&self) -> (u8, &str) {
        let rank = if self
            .branch
            .as_deref()
            .is_some_and(|b| b == "main" || b == "master")
        {
            0
        } else if self.branch.is_some() {
            1
        } else {
            2
        };
        (rank, self.branch.as_deref().unwrap_or(""))
    }

    fn display_branch(&self) -> &str {
        self.branch
            .as_deref()
            .unwrap_or(if self.detached { "(detached)" } else { "" })
    }

    fn badge(&self) -> Option<(&'static str, Color)> {
        if self.prunable {
            Some((" stale", Color::Red))
        } else if self.locked {
            Some((" lock", Color::Yellow))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pane {
    Repos,
    Worktrees,
}

struct App {
    repos: Vec<RepoData>,
    filtered_repo_indices: Vec<usize>,
    filtered_wt_indices: Vec<usize>,
    repo_state: ListState,
    wt_state: ListState,
    active_pane: Pane,
    filter: String,
    quit: bool,
    selected_path: Option<PathBuf>,
    color: bool,
    repos_w: u16,
    render_width: u16,
}

impl App {
    fn new(repos: Vec<RepoData>) -> Self {
        let filtered_repo_indices: Vec<usize> = (0..repos.len()).collect();

        let current_repo = repos
            .iter()
            .position(|r| r.worktrees.iter().any(|wt| wt.current));
        let selected_repo = current_repo.unwrap_or(0);

        let mut repo_state = ListState::default();
        if !filtered_repo_indices.is_empty() {
            repo_state.select(Some(selected_repo));
        }

        let filtered_wt_indices: Vec<usize> = if selected_repo < repos.len() {
            (0..repos[selected_repo].worktrees.len()).collect()
        } else {
            Vec::new()
        };

        let wt_start = repos
            .get(selected_repo)
            .and_then(|r| r.worktrees.iter().position(|wt| wt.current))
            .unwrap_or(0);

        let mut wt_state = ListState::default();
        if !filtered_wt_indices.is_empty() {
            wt_state.select(Some(wt_start));
        }

        let active_pane = if filtered_repo_indices.len() == 1 || current_repo.is_some() {
            Pane::Worktrees
        } else {
            Pane::Repos
        };

        let color = term::color_enabled(term::is_stdout_tty());
        let (repos_w, render_width) = compute_pane_widths(&repos);

        Self {
            repos,
            filtered_repo_indices,
            filtered_wt_indices,
            repo_state,
            wt_state,
            active_pane,
            filter: String::new(),
            quit: false,
            selected_path: None,
            color,
            repos_w,
            render_width,
        }
    }

    fn fg(&self, color: Color) -> Style {
        if self.color {
            Style::new().fg(color)
        } else {
            Style::new()
        }
    }

    fn selected_repo_index(&self) -> Option<usize> {
        self.repo_state
            .selected()
            .and_then(|i| self.filtered_repo_indices.get(i).copied())
    }

    fn selected_worktree(&self) -> Option<&WorktreeData> {
        let repo_idx = self.selected_repo_index()?;
        let wt_idx = self
            .wt_state
            .selected()
            .and_then(|i| self.filtered_wt_indices.get(i).copied())?;
        Some(&self.repos[repo_idx].worktrees[wt_idx])
    }

    fn selected_worktree_path(&self) -> Option<PathBuf> {
        self.selected_worktree().map(|wt| wt.path.clone())
    }

    fn cursor_move(&mut self, delta: isize) {
        let (selected, len) = match self.active_pane {
            Pane::Repos => (self.repo_state.selected(), self.filtered_repo_indices.len()),
            Pane::Worktrees => (self.wt_state.selected(), self.filtered_wt_indices.len()),
        };
        let Some(i) = selected else { return };
        if len == 0 {
            return;
        }
        let next = (i as isize + delta).rem_euclid(len as isize) as usize;
        match self.active_pane {
            Pane::Repos => {
                self.repo_state.select(Some(next));
                self.refresh_wt_filter();
            }
            Pane::Worktrees => self.wt_state.select(Some(next)),
        }
    }

    fn next_pane(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::Repos => Pane::Worktrees,
            Pane::Worktrees => Pane::Repos,
        };
    }

    fn refilter(&mut self) {
        if self.filter.is_empty() {
            self.filtered_repo_indices = (0..self.repos.len()).collect();
        } else {
            let mut scored: Vec<(usize, usize)> = self
                .repos
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    let best = r
                        .worktrees
                        .iter()
                        .filter_map(|wt| fuzzy::filter_score(&self.filter, &wt.filter_candidate))
                        .min();
                    best.map(|s| (i, s))
                })
                .collect();
            scored.sort_unstable_by_key(|(_, s)| *s);
            self.filtered_repo_indices = scored.into_iter().map(|(i, _)| i).collect();
        }

        self.repo_state
            .select(if self.filtered_repo_indices.is_empty() {
                None
            } else {
                Some(0)
            });

        if self.filtered_repo_indices.len() == 1 {
            self.active_pane = Pane::Worktrees;
        } else if self.filtered_repo_indices.len() > 1 {
            self.active_pane = Pane::Repos;
        }

        self.refresh_wt_filter();
    }

    fn refresh_wt_filter(&mut self) {
        if let Some(repo_idx) = self.selected_repo_index() {
            let repo = &self.repos[repo_idx];
            if self.filter.is_empty() {
                self.filtered_wt_indices = (0..repo.worktrees.len()).collect();
            } else {
                let mut scored: Vec<(usize, usize)> = repo
                    .worktrees
                    .iter()
                    .enumerate()
                    .filter_map(|(i, wt)| {
                        fuzzy::filter_score(&self.filter, &wt.filter_candidate).map(|s| (i, s))
                    })
                    .collect();
                scored.sort_unstable_by_key(|(_, s)| *s);
                self.filtered_wt_indices = scored.into_iter().map(|(i, _)| i).collect();
            }
        } else {
            self.filtered_wt_indices = Vec::new();
        }

        self.wt_state
            .select(if self.filtered_wt_indices.is_empty() {
                None
            } else {
                Some(0)
            });
    }
}

fn handle_key(app: &mut App, key: event::KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }

    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if app.filter.is_empty() {
                app.quit = true;
            } else {
                app.filter.clear();
                app.refilter();
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.quit = true;
        }
        KeyCode::Esc => {
            if !app.filter.is_empty() {
                app.filter.clear();
                app.refilter();
            } else if app.active_pane == Pane::Worktrees {
                app.active_pane = Pane::Repos;
            } else {
                app.quit = true;
            }
        }
        KeyCode::Enter => match app.active_pane {
            Pane::Repos => {
                if app.filtered_repo_indices.is_empty() {
                    app.quit = true;
                } else if !app.filtered_wt_indices.is_empty() {
                    app.active_pane = Pane::Worktrees;
                }
            }
            Pane::Worktrees => {
                if let Some(path) = app.selected_worktree_path() {
                    app.selected_path = Some(path);
                }
                app.quit = true;
            }
        },
        KeyCode::Tab | KeyCode::BackTab => app.next_pane(),
        KeyCode::Left | KeyCode::Right => app.next_pane(),
        KeyCode::Up => app.cursor_move(-1),
        KeyCode::Down => app.cursor_move(1),
        KeyCode::Backspace => {
            app.filter.pop();
            app.refilter();
        }
        KeyCode::Char(c) => {
            app.filter.push(c);
            app.refilter();
        }
        _ => {}
    }
}

fn compute_pane_widths(repos: &[RepoData]) -> (u16, u16) {
    let repos_w = {
        let max_name = repos.iter().map(|r| r.name.len()).max().unwrap_or(4);
        (max_name + 7).max(8) as u16
    };
    let wt_w: u16 = repos
        .iter()
        .map(|repo| {
            let branch_w = repo
                .worktrees
                .iter()
                .map(|wt| wt.display_branch().len())
                .max()
                .unwrap_or(4)
                .clamp(4, 40)
                + 2;
            let status_w = repo
                .worktrees
                .iter()
                .map(|wt| wt.status.len())
                .max()
                .unwrap_or(1)
                .max(1)
                + 2;
            let badge_w: usize = repo
                .worktrees
                .iter()
                .map(|wt| wt.badge().map_or(0, |(s, _)| s.len()))
                .max()
                .unwrap_or(0);
            (2 + branch_w + status_w + badge_w) as u16
        })
        .max()
        .unwrap_or(20);
    (repos_w, repos_w + wt_w + 2)
}

fn viewport_height(app: &App) -> u16 {
    let repo_count = app.repos.len();
    let max_wt = app
        .repos
        .iter()
        .map(|r| r.worktrees.len())
        .max()
        .unwrap_or(0);
    let content_rows = repo_count.max(max_wt).max(1) as u16;
    let total = content_rows + 2;
    total.min(10)
}

fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let [content_area, detail_area, footer_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let pane_w = app.render_width.min(area.width);
    let pane_area = Rect::new(content_area.x, content_area.y, pane_w, content_area.height);

    if app.filtered_repo_indices.is_empty() {
        frame.render_widget(EMPTY_HINT.dim(), pane_area);
    } else {
        let repos_w = app.repos_w.min(pane_w / 2);
        let [repos_area, wt_area] =
            Layout::horizontal([Constraint::Length(repos_w), Constraint::Min(10)])
                .spacing(2)
                .areas(pane_area);

        render_repos(frame, app, repos_area);
        render_worktrees(frame, app, wt_area);
    }
    render_detail(frame, app, detail_area);
    render_footer(frame, app, footer_area);
}

fn render_repos(frame: &mut Frame, app: &mut App, area: Rect) {
    let content_w = (area.width as usize).saturating_sub(2);
    let items: Vec<ListItem> = app
        .filtered_repo_indices
        .iter()
        .map(|&i| {
            let repo = &app.repos[i];
            let wt_count = repo.worktrees.len();
            let suffix = format!(" ({wt_count})");
            let name_budget = content_w.saturating_sub(suffix.len());
            let name = trunc(&repo.name, name_budget);
            ListItem::new(Line::from(vec![Span::raw(name), suffix.dim()]))
        })
        .collect();

    let active = app.active_pane == Pane::Repos;
    let highlight = if active {
        app.fg(Color::Cyan).bold()
    } else {
        Style::new()
    };

    let mut list = List::new(items)
        .highlight_style(highlight)
        .highlight_symbol("› ")
        .highlight_spacing(HighlightSpacing::Always)
        .scroll_padding(1);
    if !active {
        list = list.style(Style::new().dim());
    }
    frame.render_stateful_widget(list, area, &mut app.repo_state);
}

fn render_worktrees(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(repo_idx) = app.selected_repo_index() else {
        return;
    };
    let wts = &app.repos[repo_idx].worktrees;
    let content_w = (area.width as usize).saturating_sub(2);

    let (ideal_branch, status_w, badge_w) = app.filtered_wt_indices.iter().fold(
        (0usize, 0usize, 0usize),
        |(max_b, max_s, max_badge), &i| {
            let wt = &wts[i];
            (
                max_b.max(wt.display_branch().len()),
                max_s.max(wt.status.len()),
                max_badge.max(wt.badge().map_or(0, |(s, _)| s.len())),
            )
        },
    );
    let ideal_branch = ideal_branch.clamp(4, 40) + 2;
    let status_w = status_w.max(1) + 2;
    let has_badge = badge_w > 0;

    let (branch_width, show_status, show_badge) = if content_w >= ideal_branch + status_w + badge_w
    {
        (ideal_branch, true, has_badge)
    } else if content_w >= ideal_branch + status_w {
        (ideal_branch, true, false)
    } else if content_w >= ideal_branch {
        (ideal_branch, false, false)
    } else {
        (content_w, false, false)
    };
    let trunc_len = branch_width.saturating_sub(2);

    let items: Vec<ListItem> = app
        .filtered_wt_indices
        .iter()
        .map(|&i| {
            let wt = &wts[i];

            let display_branch = trunc(wt.display_branch(), trunc_len);

            let branch_style = if wt.current {
                app.fg(Color::Green)
            } else {
                Style::new()
            };

            let mut spans = vec![Span::styled(
                format!("{display_branch:<branch_width$}"),
                branch_style,
            )];

            if show_status {
                let status = &wt.status;
                let status_style = wt.status_color.map_or(Style::new().dim(), |c| app.fg(c));
                spans.push(Span::styled(format!("{status:<status_w$}"), status_style));
            }

            if show_badge && let Some((label, color)) = wt.badge() {
                spans.push(Span::styled(label, app.fg(color)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let active = app.active_pane == Pane::Worktrees;
    let highlight = if active {
        app.fg(Color::Cyan).bold()
    } else {
        Style::new()
    };

    if items.is_empty() {
        if app.selected_repo_index().is_some() {
            frame.render_widget(EMPTY_HINT.dim(), area);
        }
    } else {
        let mut list = List::new(items)
            .highlight_style(highlight)
            .highlight_symbol("› ")
            .highlight_spacing(HighlightSpacing::Always)
            .scroll_padding(1);
        if !active {
            list = list.style(Style::new().dim());
        }
        frame.render_stateful_widget(list, area, &mut app.wt_state);
    }
}

fn render_detail(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(wt) = app.selected_worktree() {
        let budget = area.width as usize;
        let display = trunc_tail(&wt.display_path, budget);
        frame.render_widget(display.dim(), area);
    }
}

fn footer_line(app: &App, width: u16) -> Line<'static> {
    let w = width as usize;

    if !app.filter.is_empty() {
        let prefix = "/ ";
        let budget = w.saturating_sub(prefix.len());
        let display = trunc_tail(&app.filter, budget);
        return Line::from(vec![prefix.dim(), Span::raw(display)]);
    }

    let (enter_action, esc_action) = match app.active_pane {
        Pane::Repos => (" open", " quit"),
        Pane::Worktrees => (" select", " back"),
    };

    let base_w = 11 + enter_action.len() + esc_action.len();
    let tab_w = 13;
    let filter_w = 17;

    let sep = " · ";
    let mut spans: Vec<Span> = vec![
        Span::raw("enter"),
        enter_action.dim(),
        sep.dim(),
        Span::raw("esc"),
        esc_action.dim(),
    ];
    if w >= base_w + tab_w {
        spans.extend([sep.dim(), Span::raw("tab"), " switch".dim()]);
        if w >= base_w + tab_w + filter_w {
            spans.extend([sep.dim(), "type to filter".dim()]);
        }
    }

    Line::from(spans)
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let line = footer_line(app, area.width);
    if app.filter.is_empty() && (area.width as usize) < line.width() {
        return;
    }
    frame.render_widget(line, area);
}

fn load_repos() -> Result<Vec<RepoData>, String> {
    let wt_root = worktree::worktrees_root()?;
    load_repos_from(&wt_root)
}

fn load_repos_from(wt_root: &std::path::Path) -> Result<Vec<RepoData>, String> {
    let wt_root = worktree::canonicalize_or_self(wt_root);
    let admin_repos = worktree::discover_repos(&wt_root);

    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok());

    let mut repos: Vec<RepoData> = std::thread::scope(|s| {
        let handles: Vec<_> = admin_repos
            .iter()
            .map(|repo_path| {
                let cwd = &cwd;
                s.spawn(move || {
                    let git = Git::new(repo_path);
                    let output = git.list_worktrees().ok()?;
                    let worktrees = worktree::parse_porcelain(&output);
                    let name = worktree::repo_basename(repo_path);

                    let mut wt_data: Vec<WorktreeData> = worktrees
                        .iter()
                        .filter(|wt| !wt.bare)
                        .map(|wt| WorktreeData::from_worktree(wt, &git, &name, cwd.as_deref()))
                        .collect();

                    wt_data.sort_unstable_by(|a, b| a.sort_key().cmp(&b.sort_key()));

                    if wt_data.is_empty() {
                        return None;
                    }

                    Some(RepoData {
                        name,
                        worktrees: wt_data,
                    })
                })
            })
            .collect();

        handles
            .into_iter()
            .filter_map(|h| match h.join() {
                Ok(repo) => repo,
                Err(e) => std::panic::resume_unwind(e),
            })
            .collect()
    });

    if repos.is_empty() {
        return Err("no worktrees".into());
    }

    repos.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    Ok(repos)
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
        return Err("cannot launch picker, stdout is not a terminal".into());
    }

    let repos = load_repos()?;
    let mut app = App::new(repos);
    let height = viewport_height(&app);

    crate::tui::run(height, |terminal| event_loop(terminal, &mut app))
        .map_err(|e| format!("cannot run picker: {e}"))?;

    #[cfg(unix)]
    if let Some(path) = &app.selected_path
        && let Ok(f) = std::env::var("__WT_CD")
    {
        std::fs::write(&f, path.as_os_str().as_bytes())
            .map_err(|e| format!("cannot write cd path: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_repos() -> Vec<RepoData> {
        vec![
            RepoData {
                name: "my-app".into(),
                worktrees: vec![
                    WorktreeData {
                        path: PathBuf::from("/wt/my-app/main"),
                        display_path: "/wt/my-app/main".into(),
                        branch: Some("main".into()),
                        filter_candidate: "my-app main".into(),
                        status: "-".into(),
                        ..Default::default()
                    },
                    WorktreeData {
                        path: PathBuf::from("/wt/my-app/feat"),
                        display_path: "/wt/my-app/feat".into(),
                        branch: Some("feat/login".into()),
                        filter_candidate: "my-app feat/login".into(),
                        status: "* ↑2".into(),
                        status_color: Some(Color::Yellow),
                        ..Default::default()
                    },
                ],
            },
            RepoData {
                name: "other-repo".into(),
                worktrees: vec![WorktreeData {
                    path: PathBuf::from("/wt/other-repo/main"),
                    display_path: "/wt/other-repo/main".into(),
                    branch: Some("main".into()),
                    filter_candidate: "other-repo main".into(),
                    status: "↓1".into(),
                    status_color: Some(Color::Cyan),
                    ..Default::default()
                }],
            },
        ]
    }

    #[test]
    fn app_initial_state() {
        let app = App::new(test_repos());
        assert_eq!(app.filtered_repo_indices, vec![0, 1]);
        assert_eq!(app.filtered_wt_indices, vec![0, 1]);
        assert_eq!(app.repo_state.selected(), Some(0));
        assert_eq!(app.wt_state.selected(), Some(0));
        assert_eq!(app.active_pane, Pane::Repos);
        assert!(app.filter.is_empty());
    }

    #[test]
    fn cursor_down_up_repos() {
        let mut app = App::new(test_repos());
        app.cursor_move(1);
        assert_eq!(app.repo_state.selected(), Some(1));
        assert_eq!(app.filtered_wt_indices, vec![0]);
        app.cursor_move(1);
        assert_eq!(app.repo_state.selected(), Some(0));
        app.cursor_move(-1);
        assert_eq!(app.repo_state.selected(), Some(1));
    }

    #[test]
    fn cursor_down_up_worktrees() {
        let mut app = App::new(test_repos());
        app.active_pane = Pane::Worktrees;
        app.cursor_move(1);
        assert_eq!(app.wt_state.selected(), Some(1));
        app.cursor_move(1);
        assert_eq!(app.wt_state.selected(), Some(0));
        app.cursor_move(-1);
        assert_eq!(app.wt_state.selected(), Some(1));
    }

    #[test]
    fn pane_switching() {
        let mut app = App::new(test_repos());
        assert_eq!(app.active_pane, Pane::Repos);
        app.next_pane();
        assert_eq!(app.active_pane, Pane::Worktrees);
        app.next_pane();
        assert_eq!(app.active_pane, Pane::Repos);
    }

    #[test]
    fn left_right_arrows_switch_panes() {
        let mut app = App::new(test_repos());
        assert_eq!(app.active_pane, Pane::Repos);
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
        );
        assert_eq!(app.active_pane, Pane::Worktrees);
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
        );
        assert_eq!(app.active_pane, Pane::Repos);
    }

    #[test]
    fn enter_in_repos_pane_switches_to_worktrees() {
        let mut app = App::new(test_repos());
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(app.active_pane, Pane::Worktrees);
        assert!(!app.quit);
    }

    #[test]
    fn enter_in_worktrees_pane_selects_and_quits() {
        let mut app = App::new(test_repos());
        app.active_pane = Pane::Worktrees;
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert!(app.quit);
        assert_eq!(app.selected_path, Some(PathBuf::from("/wt/my-app/main")));
    }

    #[test]
    fn enter_quits_when_no_results() {
        for pane in [Pane::Repos, Pane::Worktrees] {
            let mut app = App::new(test_repos());
            app.active_pane = pane;
            app.filter = "zzzzz".into();
            app.refilter();
            handle_key(
                &mut app,
                event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            );
            assert!(app.quit, "enter should quit with no results in {pane:?}");
            assert!(app.selected_path.is_none());
        }
    }

    #[test]
    fn esc_quits_from_repos_pane() {
        let mut app = App::new(test_repos());
        assert_eq!(app.active_pane, Pane::Repos);
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert!(app.quit);
        assert!(app.selected_path.is_none());
    }

    #[test]
    fn esc_goes_back_to_repos_from_worktrees() {
        let mut app = App::new(test_repos());
        app.active_pane = Pane::Worktrees;
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert!(!app.quit);
        assert_eq!(app.active_pane, Pane::Repos);
    }

    #[test]
    fn ctrl_c_quits_when_filter_empty() {
        let mut app = App::new(test_repos());
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(app.quit);
        assert!(app.selected_path.is_none());
    }

    #[test]
    fn ctrl_d_quits() {
        let mut app = App::new(test_repos());
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
        );
        assert!(app.quit);
        assert!(app.selected_path.is_none());
    }

    #[test]
    fn ctrl_c_clears_filter_when_nonempty() {
        let mut app = App::new(test_repos());
        app.filter = "ot".into();
        app.refilter();
        assert_eq!(app.filtered_repo_indices.len(), 1);
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(!app.quit);
        assert!(app.filter.is_empty());
        assert_eq!(app.filtered_repo_indices.len(), 2);
    }

    #[test]
    fn esc_clears_filter_when_nonempty() {
        let mut app = App::new(test_repos());
        app.filter = "ot".into();
        app.refilter();
        assert_eq!(app.filtered_repo_indices.len(), 1);
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert!(!app.quit);
        assert!(app.filter.is_empty());
        assert_eq!(app.filtered_repo_indices.len(), 2);
    }

    #[test]
    fn typing_filters_repos() {
        let mut app = App::new(test_repos());
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE),
        );
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE),
        );
        assert_eq!(app.filter, "ot");
        assert_eq!(app.filtered_repo_indices, vec![1]);
    }

    #[test]
    fn backspace_removes_filter_char() {
        let mut app = App::new(test_repos());
        app.filter = "ot".into();
        app.refilter();
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        assert_eq!(app.filter, "o");
    }

    #[test]
    fn filter_matches_worktree_branches() {
        let mut app = App::new(test_repos());
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
        );
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE),
        );
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
        );
        assert_eq!(app.filter, "log");
        assert!(app.filtered_repo_indices.contains(&0));
    }

    #[test]
    fn filter_matches_across_repo_and_branch() {
        let mut app = App::new(test_repos());
        for c in "my log".chars() {
            handle_key(
                &mut app,
                event::KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE),
            );
        }
        assert_eq!(app.filter, "my log");
        assert_eq!(app.filtered_repo_indices, vec![0]);
        assert_eq!(app.filtered_wt_indices, vec![1]);
    }

    #[test]
    fn empty_repos_does_not_panic() {
        let app = App::new(Vec::new());
        assert!(app.filtered_repo_indices.is_empty());
        assert!(app.repo_state.selected().is_none());
    }

    #[test]
    fn single_repo_starts_in_worktrees_pane() {
        let repos = vec![test_repos().remove(0)];
        let app = App::new(repos);
        assert_eq!(app.active_pane, Pane::Worktrees);
    }

    #[test]
    fn filter_auto_switches_pane_with_result_count() {
        let mut app = App::new(test_repos());
        assert_eq!(app.active_pane, Pane::Repos);
        app.filter = "ot".into();
        app.refilter();
        assert_eq!(app.filtered_repo_indices.len(), 1);
        assert_eq!(app.active_pane, Pane::Worktrees);
        app.filter.pop();
        app.refilter();
        assert_eq!(app.filtered_repo_indices.len(), 2);
        assert_eq!(app.active_pane, Pane::Repos);
    }

    #[test]
    fn selecting_second_repo_updates_worktrees() {
        let mut app = App::new(test_repos());
        app.cursor_move(1);
        assert_eq!(app.repo_state.selected(), Some(1));
        assert_eq!(app.filtered_wt_indices, vec![0]);
        app.active_pane = Pane::Worktrees;
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(
            app.selected_path,
            Some(PathBuf::from("/wt/other-repo/main"))
        );
    }

    #[test]
    fn cursor_movement_on_empty_filtered_list() {
        let mut app = App::new(test_repos());
        app.filter = "zzzzz".into();
        app.refilter();
        assert!(app.filtered_repo_indices.is_empty());
        assert!(app.repo_state.selected().is_none());
        app.cursor_move(1);
        app.cursor_move(-1);
        assert!(app.repo_state.selected().is_none());
    }

    #[test]
    fn filter_no_match_clears_selection() {
        let mut app = App::new(test_repos());
        assert_eq!(app.repo_state.selected(), Some(0));
        app.filter = "zzzzz".into();
        app.refilter();
        assert!(app.filtered_repo_indices.is_empty());
        assert!(app.repo_state.selected().is_none());
        assert!(app.filtered_wt_indices.is_empty());
        assert!(app.wt_state.selected().is_none());
    }

    #[test]
    fn enter_in_repos_pane_does_not_switch_when_no_worktrees() {
        let mut app = App::new(test_repos());
        app.filtered_wt_indices.clear();
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(app.active_pane, Pane::Repos);
    }

    #[test]
    fn backspace_on_empty_filter_is_noop() {
        let mut app = App::new(test_repos());
        assert!(app.filter.is_empty());
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE),
        );
        assert!(app.filter.is_empty());
        assert_eq!(app.filtered_repo_indices, vec![0, 1]);
    }

    #[test]
    fn viewport_height_caps_at_10() {
        let mut repos = Vec::new();
        for i in 0..15 {
            repos.push(RepoData {
                name: format!("repo-{i}"),
                worktrees: vec![WorktreeData {
                    path: PathBuf::from(format!("/wt/repo-{i}/main")),
                    branch: Some("main".into()),
                    filter_candidate: format!("repo-{i} main"),
                    ..Default::default()
                }],
            });
        }
        let app = App::new(repos);
        assert_eq!(viewport_height(&app), 10);
    }

    #[test]
    fn viewport_height_fits_content() {
        let app = App::new(test_repos());
        let h = viewport_height(&app);
        assert_eq!(h, 4);
    }

    #[test]
    fn filter_ranks_better_matches_first() {
        let mut app = App::new(test_repos());
        app.filter = "main".into();
        app.refilter();
        assert_eq!(app.filtered_repo_indices.len(), 2);
        assert_eq!(app.repos[app.filtered_repo_indices[0]].name, "other-repo");
        assert_eq!(app.repos[app.filtered_repo_indices[1]].name, "my-app");
    }

    #[test]
    fn tab_switches_panes() {
        let mut app = App::new(test_repos());
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        assert_eq!(app.active_pane, Pane::Worktrees);
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        );
        assert_eq!(app.active_pane, Pane::Repos);
    }

    #[test]
    fn load_repos_nonexistent_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let Err(err) = load_repos_from(&missing) else {
            panic!("expected error");
        };
        assert!(err.contains("no worktrees"));
    }

    #[test]
    fn load_repos_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let Err(err) = load_repos_from(tmp.path()) else {
            panic!("expected error");
        };
        assert!(err.contains("no worktrees"));
    }

    #[test]
    fn pre_selects_current_worktree() {
        let repos = vec![
            RepoData {
                name: "alpha".into(),
                worktrees: vec![WorktreeData {
                    path: PathBuf::from("/wt/alpha/main"),
                    branch: Some("main".into()),
                    filter_candidate: "alpha main".into(),
                    ..Default::default()
                }],
            },
            RepoData {
                name: "beta".into(),
                worktrees: vec![
                    WorktreeData {
                        path: PathBuf::from("/wt/beta/main"),
                        branch: Some("main".into()),
                        filter_candidate: "beta main".into(),
                        ..Default::default()
                    },
                    WorktreeData {
                        path: PathBuf::from("/wt/beta/feat"),
                        branch: Some("feat/work".into()),
                        filter_candidate: "beta feat/work".into(),
                        current: true,
                        ..Default::default()
                    },
                ],
            },
        ];
        let app = App::new(repos);
        assert_eq!(app.repo_state.selected(), Some(1));
        assert_eq!(app.wt_state.selected(), Some(1));
        assert_eq!(app.active_pane, Pane::Worktrees);
    }

    #[test]
    fn pre_selects_current_worktree_after_navigation() {
        let mut app = App::new(test_repos());
        assert_eq!(app.wt_state.selected(), Some(0));
        app.cursor_move(1);
        app.cursor_move(-1);
        assert_eq!(app.repo_state.selected(), Some(0));
        assert_eq!(app.wt_state.selected(), Some(0));
    }

    #[test]
    fn no_current_worktree_selects_first() {
        let repos = vec![RepoData {
            name: "repo".into(),
            worktrees: vec![
                WorktreeData {
                    path: PathBuf::from("/wt/repo/main"),
                    branch: Some("main".into()),
                    filter_candidate: "repo main".into(),
                    ..Default::default()
                },
                WorktreeData {
                    path: PathBuf::from("/wt/repo/feat"),
                    branch: Some("feat".into()),
                    filter_candidate: "repo feat".into(),
                    ..Default::default()
                },
            ],
        }];
        let app = App::new(repos);
        assert_eq!(app.repo_state.selected(), Some(0));
        assert_eq!(app.wt_state.selected(), Some(0));
    }

    #[test]
    fn badge_priority() {
        let base = WorktreeData::default();

        assert!(base.badge().is_none());

        let locked = WorktreeData {
            locked: true,
            ..base.clone()
        };
        assert_eq!(locked.badge().unwrap(), (" lock", Color::Yellow));

        let prunable = WorktreeData {
            prunable: true,
            ..base.clone()
        };
        assert_eq!(prunable.badge().unwrap(), (" stale", Color::Red));

        let both = WorktreeData {
            locked: true,
            prunable: true,
            ..base
        };
        assert_eq!(both.badge().unwrap(), (" stale", Color::Red));
    }

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn footer_full_width_shows_all_hints() {
        let app = App::new(test_repos());
        let text = line_text(&footer_line(&app, 80));
        assert!(text.contains("enter"));
        assert!(text.contains("esc"));
        assert!(text.contains("tab switch"));
        assert!(text.contains("type to filter"));
    }

    #[test]
    fn footer_medium_width_drops_filter_hint() {
        let app = App::new(test_repos());
        let line = footer_line(&app, 40);
        let text = line_text(&line);
        assert!(text.contains("tab switch"));
        assert!(!text.contains("type to filter"));
    }

    #[test]
    fn footer_narrow_drops_tab_hint() {
        let app = App::new(test_repos());
        let line = footer_line(&app, 25);
        let text = line_text(&line);
        assert!(text.contains("enter"));
        assert!(text.contains("esc"));
        assert!(!text.contains("tab"));
    }

    #[test]
    fn footer_repos_vs_worktrees_actions() {
        let mut app = App::new(test_repos());
        let text = line_text(&footer_line(&app, 80));
        assert!(text.contains("open"));
        assert!(text.contains("quit"));

        app.active_pane = Pane::Worktrees;
        let text = line_text(&footer_line(&app, 80));
        assert!(text.contains("select"));
        assert!(text.contains("back"));
    }

    #[test]
    fn footer_filter_shows_text() {
        let mut app = App::new(test_repos());
        app.filter = "test".into();
        let text = line_text(&footer_line(&app, 80));
        assert!(text.contains("/ "));
        assert!(text.contains("test"));
    }
}
