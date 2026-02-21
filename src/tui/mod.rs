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
    Terminal,
};
use std::io::stdout;

use crate::search::SearchResult;

/// TUI application state
pub struct App {
    pub results: Vec<SearchResult>,
    pub query: String,
    pub selected: usize,
    pub filter: String,
    pub filter_mode: bool,
    pub should_quit: bool,
    pub selected_session_id: Option<String>,
}

impl App {
    pub fn new(results: Vec<SearchResult>, query: String) -> Self {
        Self {
            results,
            query,
            selected: 0,
            filter: String::new(),
            filter_mode: false,
            should_quit: false,
            selected_session_id: None,
        }
    }

    /// Returns filtered results based on current filter
    pub fn filtered_results(&self) -> Vec<&SearchResult> {
        if self.filter.is_empty() {
            self.results.iter().collect()
        } else {
            let lower_filter = self.filter.to_lowercase();
            self.results
                .iter()
                .filter(|r| {
                    r.session
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
                            .contains(&lower_filter)
                })
                .collect()
        }
    }
}

/// Runs the interactive TUI picker and returns the selected session ID
pub fn run(results: Vec<SearchResult>, query: &str) -> Result<Option<String>> {
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
    Ok(app.selected_session_id)
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
                app.selected,
                &app.query,
            );

            // Preview pane
            let selected_result = filtered_owned.get(app.selected);
            picker::render_preview(f, main_chunks[1], selected_result, &app.query);

            // Help bar
            picker::render_help_bar(f, chunks[1]);
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
                            app.selected = 0;
                        }
                        KeyCode::Enter => {
                            app.filter_mode = false;
                        }
                        KeyCode::Backspace => {
                            app.filter.pop();
                            app.selected = 0;
                        }
                        KeyCode::Char(c) => {
                            app.filter.push(c);
                            app.selected = 0;
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
                                app.selected = (app.selected + 1) % filtered_len;
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if filtered_len > 0 {
                                app.selected = if app.selected == 0 {
                                    filtered_len - 1
                                } else {
                                    app.selected - 1
                                };
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(result) = filtered_owned.get(app.selected) {
                                app.selected_session_id = Some(result.session_id.clone());
                                app.should_quit = true;
                            }
                        }
                        KeyCode::Char('/') => {
                            app.filter_mode = true;
                        }
                        KeyCode::Home | KeyCode::Char('g') => {
                            app.selected = 0;
                        }
                        KeyCode::End | KeyCode::Char('G') => {
                            if filtered_len > 0 {
                                app.selected = filtered_len - 1;
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
