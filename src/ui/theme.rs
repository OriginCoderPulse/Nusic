use ratatui::style::{Modifier, Style};

/// Catppuccin Mocha — https://github.com/catppuccin/catppuccin
#[allow(dead_code)]
mod mocha {
    use ratatui::style::Color;

    pub const ROSEWATER: Color = Color::Rgb(245, 224, 220);
    pub const FLAMINGO: Color = Color::Rgb(242, 205, 205);
    pub const PINK: Color = Color::Rgb(245, 194, 231);
    pub const MAUVE: Color = Color::Rgb(203, 166, 247);
    pub const RED: Color = Color::Rgb(243, 139, 168);
    pub const PEACH: Color = Color::Rgb(250, 179, 135);
    pub const YELLOW: Color = Color::Rgb(249, 226, 175);
    pub const GREEN: Color = Color::Rgb(166, 227, 161);
    pub const TEAL: Color = Color::Rgb(148, 226, 213);
    pub const SKY: Color = Color::Rgb(137, 220, 235);
    pub const SAPPHIRE: Color = Color::Rgb(116, 199, 236);
    pub const BLUE: Color = Color::Rgb(137, 180, 250);
    pub const LAVENDER: Color = Color::Rgb(180, 190, 254);
    pub const TEXT: Color = Color::Rgb(205, 214, 244);
    pub const SUBTEXT1: Color = Color::Rgb(186, 194, 222);
    pub const SUBTEXT0: Color = Color::Rgb(166, 173, 200);
    pub const OVERLAY2: Color = Color::Rgb(147, 153, 178);
    pub const OVERLAY1: Color = Color::Rgb(127, 132, 156);
    pub const OVERLAY0: Color = Color::Rgb(108, 112, 134);
    pub const SURFACE2: Color = Color::Rgb(88, 91, 112);
    pub const SURFACE1: Color = Color::Rgb(69, 71, 90);
    pub const SURFACE0: Color = Color::Rgb(49, 50, 68);
    pub const BASE: Color = Color::Rgb(30, 30, 46);
    pub const MANTLE: Color = Color::Rgb(24, 24, 37);
    pub const CRUST: Color = Color::Rgb(17, 17, 27);
}

use mocha::*;

#[derive(Clone, Copy)]
pub struct Theme {
    pub border: Style,
    pub title: Style,
    pub accent: Style,
    pub text: Style,
    pub muted: Style,
    pub selected: Style,
    pub playing: Style,
    pub progress_fill: Style,
    pub progress_empty: Style,
    pub error: Style,
    pub subtitle: Style,
    pub popup: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border: Style::default().fg(SUBTEXT0),
            title: Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            accent: Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
            subtitle: Style::default().fg(LAVENDER),
            text: Style::default().fg(TEXT),
            muted: Style::default().fg(OVERLAY1),
            selected: Style::default()
                .fg(YELLOW)
                .add_modifier(Modifier::BOLD),
            playing: Style::default().fg(TEAL).add_modifier(Modifier::BOLD),
            progress_fill: Style::default().fg(MAUVE),
            progress_empty: Style::default().fg(SURFACE1),
            error: Style::default().fg(RED),
            popup: Style::default().bg(BASE).fg(TEXT),
        }
    }
}
