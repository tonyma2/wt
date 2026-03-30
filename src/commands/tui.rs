use std::io;
use std::path::PathBuf;

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState};

use crate::fuzzy;
use crate::git::Git;
use crate::terminal::{self as term, trunc, trunc_tail};
use crate::worktree::{self, Worktree};

struct RepoData {
    name: String,
    worktrees: Vec<WorktreeData>,
}

struct WorktreeData {
    path: PathBuf,
    display_path: String,
    branch: Option<String>,
    filter_candidate: String,
    detached: bool,
    locked: bool,
    dirty: bool,
    ahead: Option<u64>,
    behind: Option<u64>,
    current: bool,
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
}

impl App {
    fn new(repos: Vec<RepoData>) -> Self {
        let filtered_repo_indices: Vec<usize> = (0..repos.len()).collect();
        let filtered_wt_indices = if let Some(&first) = filtered_repo_indices.first() {
            (0..repos[first].worktrees.len()).collect()
        } else {
            Vec::new()
        };

        let mut repo_state = ListState::default();
        if !filtered_repo_indices.is_empty() {
            repo_state.select(Some(0));
        }
        let mut wt_state = ListState::default();
        if !filtered_wt_indices.is_empty() {
            wt_state.select(Some(0));
        }

        let active_pane = if filtered_repo_indices.len() == 1 {
            Pane::Worktrees
        } else {
            Pane::Repos
        };

        let color = term::color_enabled(term::is_stderr_tty());

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

    fn cursor_up(&mut self) {
        match self.active_pane {
            Pane::Repos => {
                if let Some(i) = self.repo_state.selected() {
                    let len = self.filtered_repo_indices.len();
                    if len == 0 {
                        return;
                    }
                    self.repo_state
                        .select(Some(if i > 0 { i - 1 } else { len - 1 }));
                    self.refresh_wt_filter();
                }
            }
            Pane::Worktrees => {
                if let Some(i) = self.wt_state.selected() {
                    let len = self.filtered_wt_indices.len();
                    if len == 0 {
                        return;
                    }
                    self.wt_state
                        .select(Some(if i > 0 { i - 1 } else { len - 1 }));
                }
            }
        }
    }

    fn cursor_down(&mut self) {
        match self.active_pane {
            Pane::Repos => {
                if let Some(i) = self.repo_state.selected() {
                    let len = self.filtered_repo_indices.len();
                    if len == 0 {
                        return;
                    }
                    let next = i + 1;
                    self.repo_state
                        .select(Some(if next < len { next } else { 0 }));
                    self.refresh_wt_filter();
                }
            }
            Pane::Worktrees => {
                if let Some(i) = self.wt_state.selected() {
                    let len = self.filtered_wt_indices.len();
                    if len == 0 {
                        return;
                    }
                    let next = i + 1;
                    self.wt_state
                        .select(Some(if next < len { next } else { 0 }));
                }
            }
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
        KeyCode::Esc => app.quit = true,
        KeyCode::Enter => match app.active_pane {
            Pane::Repos => {
                if !app.filtered_wt_indices.is_empty() {
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
        KeyCode::Up => app.cursor_up(),
        KeyCode::Down => app.cursor_down(),
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

fn repos_pane_width(app: &App) -> u16 {
    let max_name = app.repos.iter().map(|r| r.name.len()).max().unwrap_or(4);
    (max_name + 7).max(8) as u16
}

fn max_wt_pane_width(app: &App) -> u16 {
    app.repos
        .iter()
        .map(|repo| {
            let branch_width = repo
                .worktrees
                .iter()
                .map(|wt| {
                    wt.branch
                        .as_deref()
                        .unwrap_or(if wt.detached { "(detached)" } else { "" })
                        .len()
                })
                .max()
                .unwrap_or(4)
                .clamp(4, 40)
                + 2;

            let max_badge: usize = repo
                .worktrees
                .iter()
                .map(|wt| if wt.locked { 7 } else { 0 })
                .max()
                .unwrap_or(0);

            (2 + branch_width + 8 + max_badge) as u16
        })
        .max()
        .unwrap_or(20)
}

fn max_detail_width(app: &App) -> u16 {
    app.repos
        .iter()
        .flat_map(|r| &r.worktrees)
        .map(|wt| wt.display_path.len() + 2)
        .max()
        .unwrap_or(0) as u16
}

fn render_width(app: &App) -> u16 {
    let panes = repos_pane_width(app) + max_wt_pane_width(app) + 2;
    let detail = max_detail_width(app);
    let footer = footer_help_line().width() as u16;
    panes.max(detail).max(footer)
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
    let width = render_width(app).min(area.width);
    let area = Rect::new(area.x, area.y, width, area.height);

    let [content_area, detail_area, footer_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    if app.filtered_repo_indices.is_empty() {
        frame.render_widget("  no matches \u{b7} backspace to edit".dim(), content_area);
    } else {
        let repos_w = repos_pane_width(app) + 1;
        let [repos_area, wt_area] =
            Layout::horizontal([Constraint::Length(repos_w), Constraint::Min(10)])
                .spacing(1)
                .areas(content_area);

        render_repos(frame, app, repos_area);
        render_worktrees(frame, app, wt_area);
    }
    render_detail(frame, app, detail_area);
    render_footer(frame, app, footer_area);
}

fn render_repos(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(Style::new().dim());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let content_w = (inner.width as usize).saturating_sub(2);
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
        .highlight_symbol("\u{203a} ")
        .highlight_spacing(HighlightSpacing::Always)
        .scroll_padding(1);
    if !active {
        list = list.style(Style::new().dim());
    }
    frame.render_stateful_widget(list, inner, &mut app.repo_state);
}

fn render_worktrees(frame: &mut Frame, app: &mut App, area: Rect) {
    let repo_idx = app.selected_repo_index().unwrap_or(0);
    let data_branch_width = app
        .filtered_wt_indices
        .iter()
        .map(|&i| {
            let wt = &app.repos[repo_idx].worktrees[i];
            wt.branch
                .as_deref()
                .unwrap_or(if wt.detached { "(detached)" } else { "" })
                .len()
        })
        .max()
        .unwrap_or(4)
        .clamp(4, 40)
        + 2;

    let content_w = (area.width as usize).saturating_sub(2);
    let branch_width = data_branch_width.min(content_w.saturating_sub(8));
    let trunc_len = branch_width.saturating_sub(2);

    let items: Vec<ListItem> = app
        .filtered_wt_indices
        .iter()
        .map(|&i| {
            let wt = &app.repos[repo_idx].worktrees[i];
            let branch =
                wt.branch
                    .as_deref()
                    .unwrap_or(if wt.detached { "(detached)" } else { "" });

            let display_branch = trunc(branch, trunc_len);
            let status = format_status(wt.dirty, wt.ahead, wt.behind);

            let branch_style = if wt.current {
                app.fg(Color::Green)
            } else {
                Style::new()
            };

            let status_style = if wt.dirty {
                app.fg(Color::Yellow)
            } else if wt.ahead.is_some_and(|a| a > 0) || wt.behind.is_some_and(|b| b > 0) {
                app.fg(Color::Cyan)
            } else {
                Style::new().dim()
            };

            let mut spans = vec![
                Span::styled(format!("{display_branch:<branch_width$}"), branch_style),
                Span::styled(format!("{status:<8}"), status_style),
            ];

            if wt.locked {
                spans.push(Span::styled(" locked", app.fg(Color::Yellow)));
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
            frame.render_widget("    no matches \u{b7} backspace to edit".dim(), area);
        }
    } else {
        let mut list = List::new(items)
            .highlight_style(highlight)
            .highlight_symbol("\u{203a} ")
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
        let budget = (area.width as usize).saturating_sub(3);
        let display = trunc_tail(&wt.display_path, budget);
        frame.render_widget(Line::from(vec!["  ".dim(), display.dim()]), area);
    }
}

fn footer_help_line() -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::raw("\u{2191}\u{2193}"),
        " navigate".dim(),
        " \u{b7} ".dim(),
        Span::raw("\u{2190}\u{2192}"),
        " switch".dim(),
        " \u{b7} ".dim(),
        "type to filter".dim(),
    ])
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let line = if !app.filter.is_empty() {
        Line::from(vec!["  / ".dim(), Span::raw(&app.filter)])
    } else {
        let help = footer_help_line();
        if (area.width as usize) < help.width() {
            return;
        }
        help
    };
    frame.render_widget(line, area);
}

fn format_status(dirty: bool, ahead: Option<u64>, behind: Option<u64>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if dirty {
        parts.push("*".into());
    }
    if let Some(a) = ahead
        && a > 0
    {
        parts.push(format!("\u{2191}{a}"));
    }
    if let Some(b) = behind
        && b > 0
    {
        parts.push(format!("\u{2193}{b}"));
    }
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(" ")
    }
}

fn computed_status(git: &Git, wt: &Worktree) -> (bool, Option<u64>, Option<u64>) {
    let dirty = git.is_dirty(&wt.path);
    let (ahead, behind) = wt
        .branch
        .as_deref()
        .and_then(|b| git.ahead_behind(b))
        .map_or((None, None), |(a, b)| (Some(a), Some(b)));
    (dirty, ahead, behind)
}

fn load_repos() -> Result<Vec<RepoData>, String> {
    let wt_root = worktree::worktrees_root()?;
    if !wt_root.is_dir() {
        return Err("no managed worktrees found, use `wt clone` or `wt new` first".into());
    }
    let wt_root = worktree::canonicalize_or_self(&wt_root);
    let admin_repos = worktree::discover_repos(&wt_root);
    if admin_repos.is_empty() {
        return Err("no managed worktrees found, use `wt clone` or `wt new` first".into());
    }

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
                        .filter(|wt| wt.live() && !wt.bare)
                        .map(|wt| {
                            let (dirty, ahead, behind) = computed_status(&git, wt);
                            let current = cwd
                                .as_deref()
                                .is_some_and(|c| worktree::is_cwd_inside(&wt.path, Some(c)));
                            let filter_candidate = match &wt.branch {
                                Some(b) => format!("{name} {b}"),
                                None => name.clone(),
                            };
                            WorktreeData {
                                display_path: term::tilde_path(&wt.path),
                                path: wt.path.clone(),
                                branch: wt.branch.clone(),
                                filter_candidate,
                                detached: wt.detached,
                                locked: wt.locked,
                                dirty,
                                ahead,
                                behind,
                                current,
                            }
                        })
                        .collect();

                    wt_data.sort_unstable_by(|a, b| {
                        fn key(wt: &WorktreeData) -> (u8, &str) {
                            let rank = if wt.current {
                                0
                            } else if wt
                                .branch
                                .as_deref()
                                .is_some_and(|b| b == "main" || b == "master")
                            {
                                1
                            } else if wt.branch.is_some() {
                                2
                            } else {
                                3
                            };
                            (rank, wt.branch.as_deref().unwrap_or(""))
                        }
                        key(a).cmp(&key(b))
                    });

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
        return Err("no managed worktrees found, use `wt clone` or `wt new` first".into());
    }

    repos.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    Ok(repos)
}

fn event_loop(terminal: &mut crate::tui::StderrTerminal, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| render(frame, app))?;
        if app.quit {
            break;
        }
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
    }
    Ok(())
}

pub fn run() -> Result<(), String> {
    if !term::is_stderr_tty() {
        return Err("cannot launch picker, stderr is not a terminal".into());
    }

    let repos = load_repos()?;
    let mut app = App::new(repos);
    let height = viewport_height(&app);

    crate::tui::run(height, |terminal| event_loop(terminal, &mut app))
        .map_err(|e| format!("cannot run picker: {e}"))?;

    if let Some(path) = &app.selected_path {
        println!("{}", path.display());
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
                        detached: false,
                        locked: false,
                        dirty: false,
                        ahead: None,
                        behind: None,
                        current: true,
                    },
                    WorktreeData {
                        path: PathBuf::from("/wt/my-app/feat"),
                        display_path: "/wt/my-app/feat".into(),
                        branch: Some("feat/login".into()),
                        filter_candidate: "my-app feat/login".into(),
                        detached: false,
                        locked: false,
                        dirty: true,
                        ahead: Some(2),
                        behind: None,
                        current: false,
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
                    detached: false,
                    locked: false,
                    dirty: false,
                    ahead: Some(0),
                    behind: Some(1),
                    current: false,
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
        app.cursor_down();
        assert_eq!(app.repo_state.selected(), Some(1));
        assert_eq!(app.filtered_wt_indices, vec![0]);
        app.cursor_down();
        assert_eq!(app.repo_state.selected(), Some(0));
        app.cursor_up();
        assert_eq!(app.repo_state.selected(), Some(1));
    }

    #[test]
    fn cursor_down_up_worktrees() {
        let mut app = App::new(test_repos());
        app.active_pane = Pane::Worktrees;
        app.cursor_down();
        assert_eq!(app.wt_state.selected(), Some(1));
        app.cursor_down();
        assert_eq!(app.wt_state.selected(), Some(0));
        app.cursor_up();
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
    fn esc_quits_without_selection() {
        let mut app = App::new(test_repos());
        handle_key(
            &mut app,
            event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        assert!(app.quit);
        assert!(app.selected_path.is_none());
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
    fn filter_narrows_to_one_repo_then_widens_restores_pane() {
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
    fn format_status_variants() {
        assert_eq!(format_status(false, None, None), "-");
        assert_eq!(format_status(true, None, None), "*");
        assert_eq!(format_status(false, Some(2), None), "\u{2191}2");
        assert_eq!(format_status(false, None, Some(3)), "\u{2193}3");
        assert_eq!(
            format_status(true, Some(1), Some(2)),
            "* \u{2191}1 \u{2193}2"
        );
        assert_eq!(format_status(false, Some(0), Some(0)), "-");
    }

    #[test]
    fn selecting_second_repo_updates_worktrees() {
        let mut app = App::new(test_repos());
        app.cursor_down();
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
}
