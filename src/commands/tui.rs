use std::io;
use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{HighlightSpacing, List, ListItem, ListState};

use crate::fuzzy;
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
    dirty: bool,
    detached: bool,
    locked: bool,
    current: bool,
}

impl WorktreeData {
    fn from_info(wt: &worktree::WorktreeInfo, repo_name: &str) -> Self {
        let status = worktree::format_status(false, wt.dirty, wt.ahead, wt.behind);
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
            dirty: wt.dirty,
            detached: wt.detached,
            locked: wt.locked,
            current: wt.current,
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
        if self.locked {
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
    content_width: u16,
    wt_branch_w: usize,
    wt_status_w: usize,
    match_count: usize,
}

impl App {
    fn new(repos: Vec<RepoData>) -> Self {
        let filtered_repo_indices: Vec<usize> = (0..repos.len()).collect();

        let current_repo = repos
            .iter()
            .position(|r| r.worktrees.iter().any(|wt| wt.current));
        let selected_repo = current_repo.unwrap_or(0);

        let mut repo_state = ListState::default();
        repo_state.select((!filtered_repo_indices.is_empty()).then_some(selected_repo));

        let filtered_wt_indices: Vec<usize> = repos
            .get(selected_repo)
            .map(|r| (0..r.worktrees.len()).collect())
            .unwrap_or_default();

        let wt_start = repos
            .get(selected_repo)
            .and_then(|r| r.worktrees.iter().position(|wt| wt.current))
            .unwrap_or(0);

        let mut wt_state = ListState::default();
        wt_state.select((!filtered_wt_indices.is_empty()).then_some(wt_start));

        let active_pane = if filtered_repo_indices.len() == 1 || current_repo.is_some() {
            Pane::Worktrees
        } else {
            Pane::Repos
        };

        let color = term::color_enabled(term::is_stdout_tty());
        let (repos_w, content_width, wt_branch_w, wt_status_w) = compute_pane_widths(&repos);
        let match_count = repos.iter().map(|r| r.worktrees.len()).sum();

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
            content_width,
            wt_branch_w,
            wt_status_w,
            match_count,
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

    fn selected_worktree_path(&self) -> Option<&Path> {
        self.selected_worktree().map(|wt| wt.path.as_path())
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
            self.match_count = self.repos.iter().map(|r| r.worktrees.len()).sum();
        } else {
            let mut total_matches = 0usize;
            let mut scored: Vec<(usize, usize)> = self
                .repos
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    let mut best: Option<usize> = None;
                    for wt in &r.worktrees {
                        if let Some(s) = fuzzy::filter_score(&self.filter, &wt.filter_candidate) {
                            total_matches += 1;
                            best = Some(best.map_or(s, |b| b.min(s)));
                        }
                    }
                    best.map(|s| (i, s))
                })
                .collect();
            scored.sort_unstable_by_key(|(_, s)| *s);
            self.filtered_repo_indices = scored.into_iter().map(|(i, _)| i).collect();
            self.match_count = total_matches;
        }

        self.repo_state
            .select((!self.filtered_repo_indices.is_empty()).then_some(0));

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
            .select((!self.filtered_wt_indices.is_empty()).then_some(0));
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
                let path = app.selected_worktree_path().map(Path::to_path_buf);
                app.selected_path = path;
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

fn compute_pane_widths(repos: &[RepoData]) -> (u16, u16, usize, usize) {
    let repos_w = {
        let max_name = repos.iter().map(|r| r.name.len()).max().unwrap_or(4);
        let highlight = 2; // "› "
        let count_suffix = 5; // " (NN)"
        (max_name + highlight + count_suffix).max(8) as u16
    };

    let mut global_branch_w = 0usize;
    let mut global_status_w = 0usize;
    let mut max_wt_w = 0u16;

    for repo in repos {
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
        global_branch_w = global_branch_w.max(branch_w);
        global_status_w = global_status_w.max(status_w);
        max_wt_w = max_wt_w.max((2 + branch_w + status_w + badge_w) as u16);
    }

    if max_wt_w == 0 {
        max_wt_w = 20;
    }

    (
        repos_w,
        repos_w + max_wt_w + 2,
        global_branch_w,
        global_status_w,
    )
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

    let pane_w = app.content_width.min(area.width);
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
    render_footer(frame, app, footer_area, content_area.height);
}

fn styled_list<'a>(items: Vec<ListItem<'a>>, active: bool, app: &App) -> List<'a> {
    let highlight = if active {
        app.fg(Color::Cyan).bold()
    } else {
        Style::new()
    };
    let symbol = if active { "› " } else { "  " };
    let mut list = List::new(items)
        .highlight_style(highlight)
        .highlight_symbol(symbol)
        .highlight_spacing(HighlightSpacing::Always)
        .scroll_padding(1);
    if !active {
        list = list.style(Style::new().dim());
    }
    list
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

    let list = styled_list(items, app.active_pane == Pane::Repos, app);
    frame.render_stateful_widget(list, area, &mut app.repo_state);
}

fn render_worktrees(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(repo_idx) = app.selected_repo_index() else {
        return;
    };
    let wts = &app.repos[repo_idx].worktrees;
    let content_w = (area.width as usize).saturating_sub(2);

    let ideal_branch = app.wt_branch_w;
    let status_w = app.wt_status_w;
    let badge_w: usize = app
        .filtered_wt_indices
        .iter()
        .map(|&i| wts[i].badge().map_or(0, |(s, _)| s.len()))
        .max()
        .unwrap_or(0);
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
                let status_style = if wt.dirty {
                    app.fg(Color::Yellow)
                } else {
                    Style::new().dim()
                };
                spans.push(Span::styled(format!("{status:<status_w$}"), status_style));
            }

            if show_badge && let Some((label, color)) = wt.badge() {
                spans.push(Span::styled(label, app.fg(color)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    if items.is_empty() {
        frame.render_widget(EMPTY_HINT.dim(), area);
    } else {
        let list = styled_list(items, app.active_pane == Pane::Worktrees, app);
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

fn footer_line(app: &App, width: u16, visible_rows: u16) -> Line<'static> {
    let w = width as usize;

    if !app.filter.is_empty() {
        let prefix = "/ ";
        let count_label = if app.match_count == 1 {
            " · 1 match".to_owned()
        } else {
            format!(" · {} matches", app.match_count)
        };
        let prefix_w = prefix.chars().count();
        let count_w = count_label.chars().count();

        if w > prefix_w + count_w {
            let query_budget = w - prefix_w - count_w;
            let display = trunc_tail(&app.filter, query_budget);
            return Line::from(vec![prefix.dim(), Span::raw(display), count_label.dim()]);
        }
        let budget = w.saturating_sub(prefix_w);
        let display = trunc_tail(&app.filter, budget);
        return Line::from(vec![prefix.dim(), Span::raw(display)]);
    }

    let (enter_action, esc_action) = match app.active_pane {
        Pane::Repos => (" open", " quit"),
        Pane::Worktrees => (" select", " back"),
    };

    let sep = " · ";
    let base: Vec<Span> = vec![
        Span::raw("enter"),
        enter_action.dim(),
        sep.dim(),
        Span::raw("esc"),
        esc_action.dim(),
    ];
    let tab_tier: Vec<Span> = vec![sep.dim(), Span::raw("tab"), " switch".dim()];
    let filter_tier: Vec<Span> = vec![sep.dim(), "type to filter".dim()];

    let (selected, len) = match app.active_pane {
        Pane::Repos => (app.repo_state.selected(), app.filtered_repo_indices.len()),
        Pane::Worktrees => (app.wt_state.selected(), app.filtered_wt_indices.len()),
    };
    let pos_tier: Option<Vec<Span>> = if len > visible_rows as usize {
        selected.map(|i| vec![sep.dim(), format!("{}/{}", i + 1, len).dim()])
    } else {
        None
    };

    let span_w =
        |spans: &[Span]| -> usize { spans.iter().map(|s| s.content.chars().count()).sum() };
    let base_w = span_w(&base);
    let tab_w = span_w(&tab_tier);
    let filter_w = span_w(&filter_tier);
    let pos_w = pos_tier.as_ref().map_or(0, |t| span_w(t));

    let mut spans = base;
    if w >= base_w + tab_w {
        spans.extend(tab_tier);
        if w >= base_w + tab_w + filter_w + pos_w {
            spans.extend(filter_tier);
        }
        if let Some(pos) = pos_tier
            && w >= span_w(&spans) + pos_w
        {
            spans.extend(pos);
        }
    }

    Line::from(spans)
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect, visible_rows: u16) {
    let line = footer_line(app, area.width, visible_rows);
    if app.filter.is_empty() && (area.width as usize) < line.width() {
        return;
    }
    frame.render_widget(line, area);
}

fn build_repos(infos: Vec<worktree::RepoInfo>) -> Vec<RepoData> {
    infos
        .into_iter()
        .filter_map(|repo| {
            let name = repo.name;
            let mut worktrees: Vec<WorktreeData> = repo
                .worktrees
                .iter()
                .filter(|wt| !wt.bare && !wt.prunable)
                .map(|wt| WorktreeData::from_info(wt, &name))
                .collect();
            worktrees.sort_unstable_by(|a, b| a.sort_key().cmp(&b.sort_key()));
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
        return Err("cannot launch picker, stdout is not a terminal".into());
    }

    let repo_infos = worktree::load_all()?;
    let repos = build_repos(repo_infos);
    if repos.is_empty() {
        return Err("no worktrees".into());
    }
    let mut app = App::new(repos);
    let height = viewport_height(&app);

    crate::tui::run(height, |terminal| event_loop(terminal, &mut app))
        .map_err(|e| format!("cannot run picker: {e}"))?;

    if let Some(path) = &app.selected_path
        && let Ok(f) = std::env::var("__WT_CD")
    {
        let canonical = crate::worktree::canonicalize_or_self(path);
        std::fs::write(&f, canonical.as_os_str().as_encoded_bytes())
            .map_err(|e| format!("cannot write {f}: {e}"))?;
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
                        dirty: true,
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
        assert_eq!(app.repos[app.filtered_repo_indices[0]].name, "my-app");
        assert_eq!(app.repos[app.filtered_repo_indices[1]].name, "other-repo");
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
    fn badge_locked() {
        let base = WorktreeData::default();
        assert!(base.badge().is_none());

        let locked = WorktreeData {
            locked: true,
            ..base
        };
        assert_eq!(locked.badge().unwrap(), (" lock", Color::Yellow));
    }

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn footer_full_width_shows_all_hints() {
        let app = App::new(test_repos());
        let text = line_text(&footer_line(&app, 80, 10));
        assert!(text.contains("enter"));
        assert!(text.contains("esc"));
        assert!(text.contains("tab switch"));
        assert!(text.contains("type to filter"));
    }

    #[test]
    fn footer_medium_width_drops_filter_hint() {
        let app = App::new(test_repos());
        let line = footer_line(&app, 40, 10);
        let text = line_text(&line);
        assert!(text.contains("tab switch"));
        assert!(!text.contains("type to filter"));
    }

    #[test]
    fn footer_narrow_drops_tab_hint() {
        let app = App::new(test_repos());
        let line = footer_line(&app, 25, 10);
        let text = line_text(&line);
        assert!(text.contains("enter"));
        assert!(text.contains("esc"));
        assert!(!text.contains("tab"));
    }

    #[test]
    fn footer_repos_vs_worktrees_actions() {
        let mut app = App::new(test_repos());
        let text = line_text(&footer_line(&app, 80, 10));
        assert!(text.contains("open"));
        assert!(text.contains("quit"));

        app.active_pane = Pane::Worktrees;
        let text = line_text(&footer_line(&app, 80, 10));
        assert!(text.contains("select"));
        assert!(text.contains("back"));
    }

    #[test]
    fn footer_filter_shows_text() {
        let mut app = App::new(test_repos());
        app.filter = "test".into();
        let text = line_text(&footer_line(&app, 80, 10));
        assert!(text.contains("/ "));
        assert!(text.contains("test"));
    }

    #[test]
    fn footer_filter_shows_match_count() {
        let mut app = App::new(test_repos());
        app.filter = "main".into();
        app.refilter();
        let text = line_text(&footer_line(&app, 80, 10));
        assert!(text.contains("3 matches"), "got: {text}");
    }

    #[test]
    fn footer_filter_shows_singular_match() {
        let mut app = App::new(test_repos());
        app.filter = "login".into();
        app.refilter();
        let text = line_text(&footer_line(&app, 80, 10));
        assert!(text.contains("1 match"), "got: {text}");
        assert!(!text.contains("matches"));
    }

    #[test]
    fn footer_filter_hides_count_at_narrow_width() {
        let mut app = App::new(test_repos());
        app.filter = "main".into();
        app.refilter();
        let text = line_text(&footer_line(&app, 12, 10));
        assert!(text.contains("main"));
        assert!(!text.contains("match"));
    }

    #[test]
    fn footer_width_counts_display_columns_not_bytes() {
        let app = App::new(test_repos());
        let line = footer_line(&app, 51, 10);
        let text = line_text(&line);
        assert!(
            text.contains("type to filter"),
            "filter hint should fit at exact display width: {text}"
        );
    }

    #[test]
    fn footer_filter_width_counts_display_columns_not_bytes() {
        let mut app = App::new(test_repos());
        app.filter = "main".into();
        app.refilter();
        let line = footer_line(&app, 16, 10);
        let text = line_text(&line);
        assert!(
            text.contains("3 matches"),
            "match count should fit at exact display width: {text}"
        );
    }

    #[test]
    fn sort_key_main_first() {
        let main = WorktreeData {
            branch: Some("main".into()),
            ..Default::default()
        };
        let master = WorktreeData {
            branch: Some("master".into()),
            ..Default::default()
        };
        let feat = WorktreeData {
            branch: Some("feat/login".into()),
            ..Default::default()
        };
        let detached = WorktreeData {
            detached: true,
            ..Default::default()
        };

        assert!(main.sort_key() < feat.sort_key());
        assert!(master.sort_key() < feat.sort_key());
        assert!(feat.sort_key() < detached.sort_key());
    }

    #[test]
    fn sort_key_branches_alphabetical() {
        let a = WorktreeData {
            branch: Some("feat/a".into()),
            ..Default::default()
        };
        let b = WorktreeData {
            branch: Some("feat/b".into()),
            ..Default::default()
        };
        assert!(a.sort_key() < b.sort_key());
    }

    #[test]
    fn from_info_dirty() {
        let info = worktree::WorktreeInfo {
            path: PathBuf::from("/wt/repo/feat"),
            head: "abc".into(),
            branch: Some("feat".into()),
            bare: false,
            detached: false,
            locked: false,
            prunable: false,
            dirty: true,
            ahead: None,
            behind: None,
            current: false,
        };
        let data = WorktreeData::from_info(&info, "repo");
        assert!(data.dirty);
        assert_eq!(data.filter_candidate, "repo feat");
    }

    #[test]
    fn from_info_detached_no_branch_candidate() {
        let info = worktree::WorktreeInfo {
            path: PathBuf::from("/wt/repo/detached"),
            head: "abc123".into(),
            branch: None,
            bare: false,
            detached: true,
            locked: false,
            prunable: false,
            dirty: false,
            ahead: None,
            behind: None,
            current: false,
        };
        let data = WorktreeData::from_info(&info, "repo");
        assert_eq!(data.filter_candidate, "repo");
        assert_eq!(data.display_branch(), "(detached)");
    }

    #[test]
    fn build_repos_filters_bare_prunable_and_sorts() {
        let infos = vec![worktree::RepoInfo {
            name: "repo".into(),
            worktrees: vec![
                worktree::WorktreeInfo {
                    path: PathBuf::from("/admin"),
                    head: "abc".into(),
                    branch: Some("main".into()),
                    bare: true,
                    detached: false,
                    locked: false,
                    prunable: false,
                    dirty: false,
                    ahead: None,
                    behind: None,
                    current: false,
                },
                worktree::WorktreeInfo {
                    path: PathBuf::from("/wt/stale"),
                    head: "xxx".into(),
                    branch: Some("stale-branch".into()),
                    bare: false,
                    detached: false,
                    locked: false,
                    prunable: true,
                    dirty: false,
                    ahead: None,
                    behind: None,
                    current: false,
                },
                worktree::WorktreeInfo {
                    path: PathBuf::from("/wt/feat"),
                    head: "def".into(),
                    branch: Some("feat/z".into()),
                    bare: false,
                    detached: false,
                    locked: false,
                    prunable: false,
                    dirty: false,
                    ahead: None,
                    behind: None,
                    current: false,
                },
                worktree::WorktreeInfo {
                    path: PathBuf::from("/wt/main"),
                    head: "ghi".into(),
                    branch: Some("main".into()),
                    bare: false,
                    detached: false,
                    locked: false,
                    prunable: false,
                    dirty: false,
                    ahead: None,
                    behind: None,
                    current: false,
                },
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
            worktrees: vec![worktree::WorktreeInfo {
                path: PathBuf::from("/admin"),
                head: "abc".into(),
                branch: None,
                bare: true,
                detached: false,
                locked: false,
                prunable: false,
                dirty: false,
                ahead: None,
                behind: None,
                current: false,
            }],
        }];

        assert!(build_repos(infos).is_empty());
    }

    #[test]
    fn compute_pane_widths_basic() {
        let repos = test_repos();
        let (repos_w, content_w, branch_w, status_w) = compute_pane_widths(&repos);
        assert!(repos_w >= 8);
        assert!(content_w > repos_w);
        assert!(branch_w >= 6);
        assert!(status_w >= 3);
    }

    #[test]
    fn compute_pane_widths_empty() {
        let (repos_w, _, _, _) = compute_pane_widths(&[]);
        assert!(repos_w >= 8);
    }

    #[test]
    fn render_repos_pane() {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(40, 5);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::new(test_repos());

        terminal
            .draw(|frame| {
                render_repos(frame, &mut app, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let first_line: String = (0..buf.area().width)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(first_line.contains("my-app"));
    }

    #[test]
    fn render_worktrees_pane() {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(50, 5);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::new(test_repos());
        app.active_pane = Pane::Worktrees;

        terminal
            .draw(|frame| {
                render_worktrees(frame, &mut app, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let first_line: String = (0..buf.area().width)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(first_line.contains("main"));
    }

    #[test]
    fn render_worktrees_empty_shows_hint() {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(40, 3);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::new(test_repos());
        app.filtered_wt_indices.clear();
        app.wt_state.select(None);

        terminal
            .draw(|frame| {
                render_worktrees(frame, &mut app, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let first_line: String = (0..buf.area().width)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(first_line.contains("no matches"));
    }

    #[test]
    fn render_detail_shows_path() {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(40, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let app = App::new(test_repos());

        terminal
            .draw(|frame| {
                render_detail(frame, &app, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let line: String = (0..buf.area().width)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(line.contains("/wt/my-app/main"));
    }

    #[test]
    fn footer_shows_position_when_scrollable() {
        let app = App::new(test_repos());
        let text = line_text(&footer_line(&app, 80, 1));
        assert!(text.contains("1/2"), "expected position, got: {text}");
    }

    #[test]
    fn footer_no_position_when_fits() {
        let app = App::new(test_repos());
        let text = line_text(&footer_line(&app, 80, 10));
        assert!(
            !text.contains("1/2"),
            "should not show position, got: {text}"
        );
    }

    #[test]
    fn render_detail_no_position() {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(40, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let app = App::new(test_repos());

        terminal
            .draw(|frame| {
                render_detail(frame, &app, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let line: String = (0..buf.area().width)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(
            !line.contains("1/2"),
            "should not show position when list fits"
        );
    }

    #[test]
    fn render_full_layout() {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(60, 6);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::new(test_repos());

        terminal
            .draw(|frame| {
                render(frame, &mut app);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let all_text: String = (0..buf.area().height)
            .flat_map(|y| {
                let buf = &buf;
                (0..buf.area().width)
                    .map(move |x| buf[(x, y)].symbol().chars().next().unwrap_or(' '))
            })
            .collect();
        assert!(all_text.contains("my-app"));
        assert!(all_text.contains("enter"));
    }

    #[test]
    fn render_worktrees_column_collapse() {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(15, 3);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut app = App::new(test_repos());

        terminal
            .draw(|frame| {
                render_worktrees(frame, &mut app, frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let first_line: String = (0..buf.area().width)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(
            first_line.contains("main"),
            "branch should still show at narrow width"
        );
    }

    #[test]
    fn from_info_ahead_behind() {
        let info = worktree::WorktreeInfo {
            path: PathBuf::from("/wt/repo/feat"),
            head: "abc".into(),
            branch: Some("feat".into()),
            bare: false,
            detached: false,
            locked: false,
            prunable: false,
            dirty: true,
            ahead: Some(3),
            behind: Some(1),
            current: false,
        };
        let data = WorktreeData::from_info(&info, "repo");
        assert_eq!(data.status, "* \u{2191}3 \u{2193}1");
    }

    #[test]
    fn from_info_locked_sets_badge() {
        let info = worktree::WorktreeInfo {
            path: PathBuf::from("/wt/repo/feat"),
            head: "abc".into(),
            branch: Some("feat".into()),
            bare: false,
            detached: false,
            locked: true,
            prunable: false,
            dirty: false,
            ahead: None,
            behind: None,
            current: false,
        };
        let data = WorktreeData::from_info(&info, "repo");
        assert!(data.locked);
        assert!(data.badge().is_some());
    }

    #[test]
    fn refilter_on_empty_app() {
        let mut app = App::new(Vec::new());
        app.filter = "test".into();
        app.refilter();
        assert!(app.filtered_repo_indices.is_empty());
        assert_eq!(app.match_count, 0);
        assert!(app.repo_state.selected().is_none());
    }

    #[test]
    fn match_count_tracks_worktree_matches() {
        let mut app = App::new(test_repos());
        assert_eq!(app.match_count, 3);

        app.filter = "main".into();
        app.refilter();
        assert_eq!(app.match_count, 3);

        app.filter = "login".into();
        app.refilter();
        assert_eq!(app.match_count, 1);

        app.filter = "zzzzz".into();
        app.refilter();
        assert_eq!(app.match_count, 0);

        app.filter.clear();
        app.refilter();
        assert_eq!(app.match_count, 3);
    }

    #[test]
    fn build_repos_drops_all_prunable() {
        let infos = vec![worktree::RepoInfo {
            name: "stale-repo".into(),
            worktrees: vec![worktree::WorktreeInfo {
                path: PathBuf::from("/wt/stale"),
                head: "abc".into(),
                branch: Some("feat".into()),
                bare: false,
                detached: false,
                locked: false,
                prunable: true,
                dirty: false,
                ahead: None,
                behind: None,
                current: false,
            }],
        }];

        assert!(build_repos(infos).is_empty());
    }

    #[test]
    fn display_branch_no_branch_not_detached() {
        let wt = WorktreeData::default();
        assert_eq!(wt.display_branch(), "");
    }

    #[test]
    fn viewport_height_single_worktree() {
        let repos = vec![RepoData {
            name: "repo".into(),
            worktrees: vec![WorktreeData {
                path: PathBuf::from("/wt/repo/main"),
                branch: Some("main".into()),
                filter_candidate: "repo main".into(),
                ..Default::default()
            }],
        }];
        let app = App::new(repos);
        assert_eq!(viewport_height(&app), 3);
    }
}
