use std::io;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph,
};
use ratatui::{Frame, Terminal};

use crate::fuzzy;
use crate::git::Git;
use crate::terminal as term;
use crate::worktree::{self, Worktree};

struct RepoData {
    name: String,
    #[allow(dead_code)] // needed for future actions (delete, prune from TUI)
    path: PathBuf,
    worktrees: Vec<WorktreeData>,
}

struct WorktreeData {
    path: PathBuf,
    branch: Option<String>,
    #[allow(dead_code)] // planned for detail pane display
    head: String,
    bare: bool,
    detached: bool,
    locked: bool,
    prunable: bool,
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
            Style::default().fg(color)
        } else {
            Style::default()
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
                    self.repo_state
                        .select(Some(if i > 0 { i - 1 } else { len - 1 }));
                    self.refresh_wt_filter();
                }
            }
            Pane::Worktrees => {
                if let Some(i) = self.wt_state.selected() {
                    let len = self.filtered_wt_indices.len();
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
                    let next = i + 1;
                    let len = self.filtered_repo_indices.len();
                    self.repo_state
                        .select(Some(if next < len { next } else { 0 }));
                    self.refresh_wt_filter();
                }
            }
            Pane::Worktrees => {
                if let Some(i) = self.wt_state.selected() {
                    let next = i + 1;
                    let len = self.filtered_wt_indices.len();
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

    fn combined_candidate(repo_name: &str, branch: Option<&str>) -> String {
        match branch {
            Some(b) => format!("{repo_name} {b}"),
            None => repo_name.to_string(),
        }
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
                        .filter_map(|wt| {
                            let combined = Self::combined_candidate(&r.name, wt.branch.as_deref());
                            fuzzy::filter_score(&self.filter, &combined)
                        })
                        .min();
                    best.map(|s| (i, s))
                })
                .collect();
            scored.sort_by_key(|(_, s)| *s);
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
                        let combined = Self::combined_candidate(&repo.name, wt.branch.as_deref());
                        fuzzy::filter_score(&self.filter, &combined).map(|s| (i, s))
                    })
                    .collect();
                scored.sort_by_key(|(_, s)| *s);
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
                        .unwrap_or(if wt.detached { "(detached)" } else { "(bare)" })
                        .len()
                })
                .max()
                .unwrap_or(4)
                .clamp(4, 40)
                + 2;

            let max_badge: usize = repo
                .worktrees
                .iter()
                .map(|wt| {
                    let mut b = 0;
                    if wt.locked {
                        b += 7;
                    }
                    if wt.prunable {
                        b += 9;
                    }
                    b
                })
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
        .map(|wt| term::tilde_path(&wt.path).len() + 2)
        .max()
        .unwrap_or(0) as u16
}

fn float_rect(app: &App, terminal: Rect) -> Rect {
    let repo_count = app.filtered_repo_indices.len();
    let max_wt = app
        .filtered_repo_indices
        .iter()
        .map(|&i| app.repos[i].worktrees.len())
        .max()
        .unwrap_or(0);
    let content_rows = repo_count.max(max_wt).max(1) as u16;
    let height = (content_rows + 5).min(terminal.height.saturating_sub(2));

    let panes_width = repos_pane_width(app) + 1 + max_wt_pane_width(app);
    let footer_width: u16 = 58;
    let content_width = panes_width.max(max_detail_width(app)).max(footer_width);
    let width = (content_width + 4).min(terminal.width.saturating_sub(2));

    let x = terminal.x + (terminal.width.saturating_sub(width)) / 2;
    let y = terminal.y + (terminal.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn repos_pane_width(app: &App) -> u16 {
    let max_name = app.repos.iter().map(|r| r.name.len()).max().unwrap_or(4);
    (max_name + 7).max(8) as u16
}

fn render(frame: &mut Frame, app: &mut App) {
    let float = float_rect(app, frame.area());

    frame.render_widget(Clear, float);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().add_modifier(Modifier::DIM));
    let inner = block.inner(float);
    frame.render_widget(block, float);

    let [content_area, _spacer, detail_area, footer_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let padded_content = Rect::new(
        content_area.x + 1,
        content_area.y,
        content_area.width.saturating_sub(2),
        content_area.height,
    );

    let available = padded_content.width.saturating_sub(1);
    let repos_w = repos_pane_width(app).min(available * 2 / 5);

    let [repos_area, _gap, wt_area] = Layout::horizontal([
        Constraint::Length(repos_w),
        Constraint::Length(1),
        Constraint::Min(10),
    ])
    .areas(padded_content);

    render_repos(frame, app, repos_area);
    render_worktrees(frame, app, wt_area);
    render_detail(frame, app, detail_area);
    render_footer(frame, app, footer_area);
}

fn render_repos(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
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
            ListItem::new(Line::from(vec![
                Span::raw(name),
                Span::styled(suffix, Style::default().add_modifier(Modifier::DIM)),
            ]))
        })
        .collect();

    let highlight = if app.active_pane == Pane::Repos {
        app.fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    };

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "    no matches \u{b7} backspace to edit",
                Style::default().add_modifier(Modifier::DIM),
            )),
            area,
        );
    } else {
        let list = List::new(items)
            .highlight_style(highlight)
            .highlight_symbol("\u{203a} ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(list, area, &mut app.repo_state);
    }
}

fn render_worktrees(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let repo_idx = app.selected_repo_index().unwrap_or(0);
    let data_branch_width = app
        .filtered_wt_indices
        .iter()
        .map(|&i| {
            let wt = &app.repos[repo_idx].worktrees[i];
            wt.branch
                .as_deref()
                .unwrap_or(if wt.detached { "(detached)" } else { "(bare)" })
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
                    .unwrap_or(if wt.detached { "(detached)" } else { "(bare)" });

            let display_branch = trunc(branch, trunc_len);
            let status = format_status(wt.bare, wt.dirty, wt.ahead, wt.behind);

            let branch_style = if wt.current {
                app.fg(Color::Green)
            } else {
                Style::default()
            };

            let status_style = if wt.dirty {
                app.fg(Color::Yellow)
            } else if wt.ahead.is_some_and(|a| a > 0) || wt.behind.is_some_and(|b| b > 0) {
                app.fg(Color::Cyan)
            } else {
                Style::default().add_modifier(Modifier::DIM)
            };

            let mut spans = vec![
                Span::styled(format!("{display_branch:<branch_width$}"), branch_style),
                Span::styled(format!("{status:<8}"), status_style),
            ];

            if wt.locked {
                spans.push(Span::styled(
                    " locked",
                    app.fg(Color::Yellow).add_modifier(Modifier::DIM),
                ));
            }
            if wt.prunable {
                spans.push(Span::styled(
                    " prunable",
                    app.fg(Color::Red).add_modifier(Modifier::DIM),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let highlight = if app.active_pane == Pane::Worktrees {
        app.fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::DIM)
    };

    if items.is_empty() {
        if app.selected_repo_index().is_some() {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "    no matches \u{b7} backspace to edit",
                    Style::default().add_modifier(Modifier::DIM),
                )),
                area,
            );
        }
    } else {
        let list = List::new(items)
            .highlight_style(highlight)
            .highlight_symbol("\u{203a} ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(list, area, &mut app.wt_state);
    }
}

fn render_detail(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let dim = Style::default().add_modifier(Modifier::DIM);
    if let Some(wt) = app.selected_worktree() {
        let path_str = term::tilde_path(&wt.path);
        let budget = (area.width as usize).saturating_sub(3);
        let display = trunc_head(&path_str, budget);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ", dim),
                Span::styled(display, dim),
            ])),
            area,
        );
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let dim = Style::default().add_modifier(Modifier::DIM);

    let line = if app.filter.is_empty() {
        Line::from(vec![
            Span::raw("  "),
            Span::raw("\u{2191}\u{2193}"),
            Span::styled(" navigate", dim),
            Span::styled("  \u{b7}  ", dim),
            Span::raw("\u{2190}\u{2192}"),
            Span::styled(" switch", dim),
            Span::styled("  \u{b7}  ", dim),
            Span::raw("enter"),
            Span::styled(" select", dim),
            Span::styled("  \u{b7}  ", dim),
            Span::raw("esc"),
            Span::styled(" quit", dim),
        ])
    } else {
        Line::from(vec![Span::styled("  / ", dim), Span::raw(&app.filter)])
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn trunc(s: &str, max: usize) -> String {
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

fn trunc_head(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    if max <= 3 {
        return s
            .chars()
            .rev()
            .take(max)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }
    let skip = count - max + 3;
    let start = s.char_indices().nth(skip).map_or(s.len(), |(i, _)| i);
    format!("...{}", &s[start..])
}

fn format_status(bare: bool, dirty: bool, ahead: Option<u64>, behind: Option<u64>) -> String {
    if bare {
        return "bare".into();
    }
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
    if wt.bare || wt.prunable {
        return (false, None, None);
    }
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

    let repos: Vec<RepoData> = admin_repos
        .iter()
        .filter_map(|repo_path| {
            let git = Git::new(repo_path);
            let output = match git.list_worktrees() {
                Ok(o) => o,
                Err(_) => return None,
            };
            let worktrees = worktree::parse_porcelain(&output);
            let name = worktree::repo_basename(repo_path);

            let wt_data: Vec<WorktreeData> = worktrees
                .iter()
                .filter(|wt| wt.live() && !wt.bare)
                .map(|wt| {
                    let (dirty, ahead, behind) = computed_status(&git, wt);
                    let current = cwd
                        .as_deref()
                        .is_some_and(|c| worktree::is_cwd_inside(&wt.path, Some(c)));
                    WorktreeData {
                        path: wt.path.clone(),
                        branch: wt.branch.clone(),
                        head: wt.head.clone(),
                        bare: wt.bare,
                        detached: wt.detached,
                        locked: wt.locked,
                        prunable: wt.prunable,
                        dirty,
                        ahead,
                        behind,
                        current,
                    }
                })
                .collect();

            if wt_data.is_empty() {
                return None;
            }

            Some(RepoData {
                name,
                path: repo_path.clone(),
                worktrees: wt_data,
            })
        })
        .collect();

    if repos.is_empty() {
        return Err("no managed worktrees found, use `wt clone` or `wt new` first".into());
    }

    Ok(repos)
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        crossterm::terminal::disable_raw_mode().ok();
        crossterm::execute!(io::stderr(), LeaveAlternateScreen).ok();
        original(info);
    }));
}

fn run_tui(app: &mut App) -> Result<(), String> {
    install_panic_hook();
    crossterm::terminal::enable_raw_mode().map_err(|e| format!("cannot enable raw mode: {e}"))?;
    let mut stderr = io::stderr();
    crossterm::execute!(stderr, EnterAlternateScreen)
        .map_err(|e| format!("cannot enter alternate screen: {e}"))?;
    let backend = ratatui::backend::CrosstermBackend::new(io::stderr());
    let mut terminal =
        Terminal::new(backend).map_err(|e| format!("cannot create terminal: {e}"))?;

    let result = event_loop(&mut terminal, app);

    crossterm::terminal::disable_raw_mode().ok();
    crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();

    result
}

fn event_loop(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<io::Stderr>>,
    app: &mut App,
) -> Result<(), String> {
    loop {
        terminal
            .draw(|frame| render(frame, app))
            .map_err(|e| format!("cannot draw: {e}"))?;

        match event::read().map_err(|e| format!("cannot read event: {e}"))? {
            Event::Key(key) => handle_key(app, key),
            Event::Resize(_, _) => {
                terminal.clear().map_err(|e| format!("cannot clear: {e}"))?;
            }
            _ => {}
        }

        if app.quit {
            break;
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
    run_tui(&mut app)?;

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
                path: PathBuf::from("/repos/my-app"),
                worktrees: vec![
                    WorktreeData {
                        path: PathBuf::from("/wt/my-app/main"),
                        branch: Some("main".into()),
                        head: "abc123".into(),
                        bare: false,
                        detached: false,
                        locked: false,
                        prunable: false,
                        dirty: false,
                        ahead: None,
                        behind: None,
                        current: true,
                    },
                    WorktreeData {
                        path: PathBuf::from("/wt/my-app/feat"),
                        branch: Some("feat/login".into()),
                        head: "def456".into(),
                        bare: false,
                        detached: false,
                        locked: false,
                        prunable: false,
                        dirty: true,
                        ahead: Some(2),
                        behind: None,
                        current: false,
                    },
                ],
            },
            RepoData {
                name: "other-repo".into(),
                path: PathBuf::from("/repos/other-repo"),
                worktrees: vec![WorktreeData {
                    path: PathBuf::from("/wt/other-repo/main"),
                    branch: Some("main".into()),
                    head: "789abc".into(),
                    bare: false,
                    detached: false,
                    locked: false,
                    prunable: false,
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
    fn format_status_variants() {
        assert_eq!(format_status(true, false, None, None), "bare");
        assert_eq!(format_status(false, false, None, None), "-");
        assert_eq!(format_status(false, true, None, None), "*");
        assert_eq!(format_status(false, false, Some(2), None), "\u{2191}2");
        assert_eq!(format_status(false, false, None, Some(3)), "\u{2193}3");
        assert_eq!(
            format_status(false, true, Some(1), Some(2)),
            "* \u{2191}1 \u{2193}2"
        );
        assert_eq!(format_status(false, false, Some(0), Some(0)), "-");
    }

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
    fn trunc_head_short_string_unchanged() {
        assert_eq!(trunc_head("main", 10), "main");
        assert_eq!(trunc_head("main", 4), "main");
    }

    #[test]
    fn trunc_head_long_string_keeps_tail() {
        assert_eq!(
            trunc_head("~/.wt/worktrees/abc123/my-repo", 15),
            "...c123/my-repo"
        );
    }

    #[test]
    fn trunc_head_zero_budget() {
        assert_eq!(trunc_head("anything", 0), "");
    }

    #[test]
    fn trunc_head_budget_one() {
        assert_eq!(trunc_head("abc", 1), "c");
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
