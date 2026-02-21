use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::theme::Theme;
use crate::search::SearchResult;

/// Renders the search results list on the left
pub fn render_results_list(
    f: &mut Frame,
    area: Rect,
    results: &[SearchResult],
    list_state: &mut ListState,
    query: &str,
) {
    let items: Vec<ListItem> = results
        .iter()
        .map(|result| {
            // Title line: summary or first prompt
            let title = result
                .session
                .summary
                .as_deref()
                .or(result.session.first_prompt.as_deref())
                .unwrap_or("(no title)")
                .chars()
                .take(60)
                .collect::<String>();

            let date = format_date(&result.session.created_at);
            let project = short_project_path(&result.session.project_path);
            let branch = result
                .session
                .git_branch
                .as_deref()
                .map(|b| format!(" [{}]", b))
                .unwrap_or_default();
            let msgs = result
                .session
                .message_count
                .map(|c| format!(" ({} msgs)", c))
                .unwrap_or_default();

            let meta_line = Line::from(vec![
                Span::styled(format!(" {}", date), Theme::date()),
                Span::styled(format!("  {}", project), Theme::project()),
                Span::styled(branch, Theme::branch()),
                Span::styled(msgs, Theme::subtitle()),
            ]);

            let title_line = Line::from(vec![Span::styled(format!(" {} ", title), Theme::normal())]);

            let separator = Line::from("");

            ListItem::new(vec![meta_line, title_line, separator])
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .title(Span::styled(
                    format!(" Results for \"{}\" ({}) ", query, results.len()),
                    Theme::title(),
                )),
        )
        .highlight_style(Theme::selected());

    f.render_stateful_widget(list, area, list_state);
}

/// Renders the preview pane on the right
pub fn render_preview(f: &mut Frame, area: Rect, result: Option<&SearchResult>, query: &str) {
    let content = if let Some(result) = result {
        let mut lines = Vec::new();

        // Header
        if let Some(ref summary) = result.session.summary {
            lines.push(Line::from(Span::styled(summary.clone(), Theme::title())));
            lines.push(Line::from(""));
        }

        // Metadata
        lines.push(Line::from(vec![
            Span::styled("Session:  ", Theme::subtitle()),
            Span::raw(&result.session_id),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Project:  ", Theme::subtitle()),
            Span::styled(&result.session.project_path, Theme::project()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Created:  ", Theme::subtitle()),
            Span::styled(format_date(&result.session.created_at), Theme::date()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Modified: ", Theme::subtitle()),
            Span::styled(format_date(&result.session.modified_at), Theme::date()),
        ]));
        if let Some(ref branch) = result.session.git_branch {
            lines.push(Line::from(vec![
                Span::styled("Branch:   ", Theme::subtitle()),
                Span::styled(branch, Theme::branch()),
            ]));
        }
        if let Some(count) = result.session.message_count {
            lines.push(Line::from(vec![
                Span::styled("Messages: ", Theme::subtitle()),
                Span::raw(count.to_string()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "─── Conversation Preview ───",
            Theme::subtitle(),
        )));
        lines.push(Line::from(""));

        // Show first prompt
        if let Some(ref prompt) = result.session.first_prompt {
            lines.push(Line::from(Span::styled("First prompt:", Theme::subtitle())));
            for line in prompt.lines().take(5) {
                lines.push(Line::from(format!("  {}", line)));
            }
            lines.push(Line::from(""));
        }

        // Show snippet from full_text with context around query terms
        let snippet = extract_snippet(&result.session.full_text, query, 500);
        if !snippet.is_empty() {
            lines.push(Line::from(Span::styled(
                "Matching text:",
                Theme::subtitle(),
            )));
            for line in snippet.lines() {
                lines.push(Line::from(format!("  {}", line)));
            }
        }

        lines
    } else {
        vec![Line::from(Span::styled(
            "No result selected",
            Theme::subtitle(),
        ))]
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::border())
                .title(Span::styled(" Preview ", Theme::title())),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Renders the help bar at the bottom
pub fn render_help_bar(f: &mut Frame, area: Rect) {
    let help = Line::from(vec![
        Span::styled(" ↑/↓ ", Theme::title()),
        Span::styled("Navigate  ", Theme::help_text()),
        Span::styled(" Enter ", Theme::title()),
        Span::styled("Resume session  ", Theme::help_text()),
        Span::styled(" / ", Theme::title()),
        Span::styled("Filter  ", Theme::help_text()),
        Span::styled(" q/Esc ", Theme::title()),
        Span::styled("Quit", Theme::help_text()),
    ]);

    let paragraph = Paragraph::new(help).style(Theme::status_bar());
    f.render_widget(paragraph, area);
}

/// Extracts a snippet around query terms with context
fn extract_snippet(text: &str, query: &str, max_chars: usize) -> String {
    let lower_text = text.to_lowercase();
    let query_terms: Vec<&str> = query.split_whitespace().collect();

    // Find first occurrence of any query term
    let mut best_pos = None;
    for term in &query_terms {
        if let Some(pos) = lower_text.find(&term.to_lowercase()) {
            if best_pos.is_none() || pos < best_pos.unwrap() {
                best_pos = Some(pos);
            }
        }
    }

    let start_byte = match best_pos {
        Some(pos) => pos.saturating_sub(100),
        None => 0,
    };

    // Snap to char boundaries
    let start = text
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= start_byte)
        .unwrap_or(0);
    let end = text
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= start + max_chars)
        .unwrap_or(text.len());

    let snippet = &text[start..end];

    let mut result = String::new();
    if start > 0 {
        result.push_str("...");
    }
    result.push_str(snippet.trim());
    if end < text.len() {
        result.push_str("...");
    }

    result
}

/// Shortens a project path for display
fn short_project_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > 3 {
        format!(".../{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    } else {
        path.to_string()
    }
}

/// Formats an RFC3339 date string for display
fn format_date(date_str: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(date_str)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| date_str.chars().take(16).collect())
}
