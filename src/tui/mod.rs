pub mod picker;
pub mod theme;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::CrosstermBackend,
    widgets::ListState,
    Terminal,
};
use std::io::stdout;

use crate::search::SearchResult;

/// Time range filter for results
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeFilter {
    All,
    Day,
    Week,
    Month,
}

impl TimeFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Day,
            Self::Day => Self::Week,
            Self::Week => Self::Month,
            Self::Month => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Day => "24h",
            Self::Week => "7d",
            Self::Month => "30d",
        }
    }

    pub fn max_age_hours(self) -> Option<i64> {
        match self {
            Self::All => None,
            Self::Day => Some(24),
            Self::Week => Some(24 * 7),
            Self::Month => Some(24 * 30),
        }
    }
}

/// TUI application state
pub struct App {
    pub results: Vec<SearchResult>,
    pub query: String,
    pub selected: usize,
    pub list_state: ListState,
    pub filter: String,
    pub filter_mode: bool,
    pub time_filter: TimeFilter,
    pub should_quit: bool,
    pub selected_session_id: Option<String>,
    pub selected_project_path: Option<String>,
}

impl App {
    pub fn new(results: Vec<SearchResult>, query: String) -> Self {
        Self {
            results,
            query,
            selected: 0,
            list_state: ListState::default().with_selected(Some(0)),
            filter: String::new(),
            filter_mode: false,
            time_filter: TimeFilter::All,
            should_quit: false,
            selected_session_id: None,
            selected_project_path: None,
        }
    }

    /// Update selected index and sync list_state
    pub fn select(&mut self, index: usize) {
        self.selected = index;
        self.list_state.select(Some(index));
    }

    /// Returns filtered results based on text filter and time filter
    pub fn filtered_results(&self) -> Vec<&SearchResult> {
        let now = chrono::Utc::now();
        let max_age = self.time_filter.max_age_hours();

        self.results
            .iter()
            .filter(|r| {
                // Time filter
                if let Some(max_hours) = max_age {
                    let age_ok = chrono::DateTime::parse_from_rfc3339(&r.session.modified_at)
                        .map(|dt| {
                            let hours = (now - dt.to_utc()).num_hours();
                            hours <= max_hours
                        })
                        .unwrap_or(true);
                    if !age_ok {
                        return false;
                    }
                }

                // Text filter
                if !self.filter.is_empty() {
                    let lower_filter = self.filter.to_lowercase();
                    return r
                        .session
                        .summary
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&lower_filter)
                        || r.session
                            .first_prompt
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&lower_filter)
                        || r.session
                            .project_path
                            .to_lowercase()
                            .contains(&lower_filter);
                }

                true
            })
            .collect()
    }
}

/// Runs the interactive TUI picker and returns (session_id, project_path)
pub fn run(results: Vec<SearchResult>, query: &str) -> Result<Option<(String, String)>> {
    if results.is_empty() {
        eprintln!("No results found for \"{}\"", query);
        return Ok(None);
    }

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(results, query.to_string());

    let result = run_event_loop(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result?;
    match (app.selected_session_id, app.selected_project_path) {
        (Some(sid), Some(pp)) => Ok(Some((sid, pp))),
        _ => Ok(None),
    }
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        let filtered = app.filtered_results();
        let filtered_owned: Vec<SearchResult> = filtered.into_iter().cloned().collect();

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)])
                .split(f.area());

            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
                .split(chunks[0]);

            // Results list
            picker::render_results_list(
                f,
                main_chunks[0],
                &filtered_owned,
                &mut app.list_state,
                &app.query,
            );

            // Preview pane
            let selected_result = filtered_owned.get(app.selected);
            picker::render_preview(f, main_chunks[1], selected_result, &app.query);

            // Help bar
            picker::render_help_bar(f, chunks[1], app.time_filter);
        })?;

        if app.should_quit {
            break;
        }

        // Handle events
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                let filtered_len = filtered_owned.len();

                if app.filter_mode {
                    match key.code {
                        KeyCode::Esc => {
                            app.filter_mode = false;
                            app.filter.clear();
                            app.select(0);
                        }
                        KeyCode::Enter => {
                            app.filter_mode = false;
                        }
                        KeyCode::Backspace => {
                            app.filter.pop();
                            app.select(0);
                        }
                        KeyCode::Char(c) => {
                            app.filter.push(c);
                            app.select(0);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.should_quit = true;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if filtered_len > 0 {
                                app.select((app.selected + 1) % filtered_len);
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if filtered_len > 0 {
                                let new = if app.selected == 0 {
                                    filtered_len - 1
                                } else {
                                    app.selected - 1
                                };
                                app.select(new);
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(result) = filtered_owned.get(app.selected) {
                                app.selected_session_id = Some(result.session_id.clone());
                                app.selected_project_path =
                                    Some(result.session.project_path.clone());
                                app.should_quit = true;
                            }
                        }
                        KeyCode::Tab => {
                            app.time_filter = app.time_filter.next();
                            app.select(0);
                        }
                        KeyCode::Char('/') => {
                            app.filter_mode = true;
                        }
                        KeyCode::Home | KeyCode::Char('g') => {
                            app.select(0);
                        }
                        KeyCode::End | KeyCode::Char('G') => {
                            if filtered_len > 0 {
                                app.select(filtered_len - 1);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}
