use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    pub fn selected() -> Style {
        Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    }

    pub fn normal() -> Style {
        Style::default().fg(Color::White)
    }

    pub fn title() -> Style {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    pub fn subtitle() -> Style {
        Style::default().fg(Color::Gray)
    }

    pub fn project() -> Style {
        Style::default().fg(Color::Green)
    }

    pub fn date() -> Style {
        Style::default().fg(Color::Blue)
    }

    pub fn branch() -> Style {
        Style::default().fg(Color::Magenta)
    }

    #[allow(dead_code)]
    pub fn highlight() -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }

    pub fn border() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    pub fn status_bar() -> Style {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    }

    pub fn help_text() -> Style {
        Style::default().fg(Color::DarkGray)
    }
}
