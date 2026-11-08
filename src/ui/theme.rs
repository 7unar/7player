//! Built-in colour themes.
//!
//! Palettes are inspired by the widely used community themes of the same names
//! (Tokyo Night, Catppuccin, Dracula, Nord, Gruvbox, Solarized, Rosé Pine and
//! One Dark). RGB values are hand-tuned for terminal contrast.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub label: &'static str,
    pub bg: Color,
    pub surface: Color,
    pub surface_alt: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub accent_alt: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
    pub border: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

pub const THEME_NAMES: &[&str] = &[
    "tokyo-night",
    "catppuccin-mocha",
    "dracula",
    "nord",
    "gruvbox-dark",
    "solarized-dark",
    "rose-pine",
    "one-dark",
    "catppuccin-latte",
    "solarized-light",
];

pub const DEFAULT_THEME_INDEX: usize = 0;

pub fn theme_index(name: &str) -> usize {
    THEME_NAMES
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(name))
        .unwrap_or(DEFAULT_THEME_INDEX)
}

pub fn theme_by_name(name: &str) -> Theme {
    theme_at(theme_index(name))
}

pub fn theme_at(index: usize) -> Theme {
    let index = index % THEME_NAMES.len();
    match THEME_NAMES[index] {
        "catppuccin-mocha" => Theme {
            name: "catppuccin-mocha",
            label: "Catppuccin Mocha",
            bg: rgb(0x1e1e2e),
            surface: rgb(0x181825),
            surface_alt: rgb(0x313244),
            text: rgb(0xcdd6f4),
            muted: rgb(0x7f849c),
            accent: rgb(0x89b4fa),
            accent_alt: rgb(0xcba6f7),
            green: rgb(0xa6e3a1),
            yellow: rgb(0xf9e2af),
            red: rgb(0xf38ba8),
            border: rgb(0x45475a),
        },
        "dracula" => Theme {
            name: "dracula",
            label: "Dracula",
            bg: rgb(0x282a36),
            surface: rgb(0x21222c),
            surface_alt: rgb(0x44475a),
            text: rgb(0xf8f8f2),
            muted: rgb(0x6272a4),
            accent: rgb(0xbd93f9),
            accent_alt: rgb(0xff79c6),
            green: rgb(0x50fa7b),
            yellow: rgb(0xf1fa8c),
            red: rgb(0xff5555),
            border: rgb(0x44475a),
        },
        "nord" => Theme {
            name: "nord",
            label: "Nord",
            bg: rgb(0x2e3440),
            surface: rgb(0x272c36),
            surface_alt: rgb(0x3b4252),
            text: rgb(0xeceff4),
            muted: rgb(0x7b88a1),
            accent: rgb(0x88c0d0),
            accent_alt: rgb(0xb48ead),
            green: rgb(0xa3be8c),
            yellow: rgb(0xebcb8b),
            red: rgb(0xbf616a),
            border: rgb(0x4c566a),
        },
        "gruvbox-dark" => Theme {
            name: "gruvbox-dark",
            label: "Gruvbox Dark",
            bg: rgb(0x282828),
            surface: rgb(0x1d2021),
            surface_alt: rgb(0x3c3836),
            text: rgb(0xebdbb2),
            muted: rgb(0x928374),
            accent: rgb(0xfabd2f),
            accent_alt: rgb(0xd3869b),
            green: rgb(0xb8bb26),
            yellow: rgb(0xfabd2f),
            red: rgb(0xfb4934),
            border: rgb(0x504945),
        },
        "solarized-dark" => Theme {
            name: "solarized-dark",
            label: "Solarized Dark",
            bg: rgb(0x002b36),
            surface: rgb(0x00252e),
            surface_alt: rgb(0x073642),
            text: rgb(0xeee8d5),
            muted: rgb(0x657b83),
            accent: rgb(0x268bd2),
            accent_alt: rgb(0x2aa198),
            green: rgb(0x859900),
            yellow: rgb(0xb58900),
            red: rgb(0xdc322f),
            border: rgb(0x586e75),
        },
        "rose-pine" => Theme {
            name: "rose-pine",
            label: "Rosé Pine Moon",
            bg: rgb(0x232136),
            surface: rgb(0x1f1d2e),
            surface_alt: rgb(0x393552),
            text: rgb(0xe0def4),
            muted: rgb(0x817c9c),
            accent: rgb(0xc4a7e7),
            accent_alt: rgb(0xea9a97),
            green: rgb(0x9ccfd8),
            yellow: rgb(0xf6c177),
            red: rgb(0xeb6f92),
            border: rgb(0x44415a),
        },
        "one-dark" => Theme {
            name: "one-dark",
            label: "One Dark",
            bg: rgb(0x282c34),
            surface: rgb(0x21252b),
            surface_alt: rgb(0x3a3f4b),
            text: rgb(0xabb2bf),
            muted: rgb(0x6b727f),
            accent: rgb(0x61afef),
            accent_alt: rgb(0xc678dd),
            green: rgb(0x98c379),
            yellow: rgb(0xe5c07b),
            red: rgb(0xe06c75),
            border: rgb(0x3e4451),
        },
        "catppuccin-latte" => Theme {
            name: "catppuccin-latte",
            label: "Catppuccin Latte",
            bg: rgb(0xeff1f5),
            surface: rgb(0xe6e9ef),
            surface_alt: rgb(0xccd0da),
            text: rgb(0x4c4f69),
            muted: rgb(0x8c8fa1),
            accent: rgb(0x1e66f5),
            accent_alt: rgb(0x8839ef),
            green: rgb(0x40a02b),
            yellow: rgb(0xdf8e1d),
            red: rgb(0xd20f39),
            border: rgb(0xbcc0cc),
        },
        "solarized-light" => Theme {
            name: "solarized-light",
            label: "Solarized Light",
            bg: rgb(0xfdf6e3),
            surface: rgb(0xeee8d5),
            surface_alt: rgb(0xe4ddc8),
            text: rgb(0x586e75),
            muted: rgb(0x93a1a1),
            accent: rgb(0x268bd2),
            accent_alt: rgb(0xd33682),
            green: rgb(0x859900),
            yellow: rgb(0xb58900),
            red: rgb(0xdc322f),
            border: rgb(0xd8d2c0),
        },
        _ => Theme {
            name: "tokyo-night",
            label: "Tokyo Night",
            bg: rgb(0x1a1b26),
            surface: rgb(0x16161e),
            surface_alt: rgb(0x292e42),
            text: rgb(0xc0caf5),
            muted: rgb(0x565f89),
            accent: rgb(0x7aa2f7),
            accent_alt: rgb(0xbb9af7),
            green: rgb(0x9ece6a),
            yellow: rgb(0xe0af68),
            red: rgb(0xf7768e),
            border: rgb(0x2f3549),
        },
    }
}

impl Theme {
    pub fn base(self) -> Style {
        Style::default().fg(self.text).bg(self.bg)
    }

    pub fn selected(self) -> Style {
        Style::default()
            .fg(self.accent)
            .bg(self.surface_alt)
            .add_modifier(Modifier::BOLD)
    }

    pub fn title(self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn muted(self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn swatches(self) -> [Color; 5] {
        [
            self.accent,
            self.accent_alt,
            self.green,
            self.yellow,
            self.red,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_resolves_to_itself() {
        for (i, name) in THEME_NAMES.iter().enumerate() {
            let theme = theme_at(i);
            assert_eq!(theme.name, *name);
            assert_eq!(theme_by_name(name).name, *name);
            assert_eq!(theme_index(name), i);
        }
    }

    #[test]
    fn unknown_theme_falls_back() {
        assert_eq!(theme_by_name("no-such-theme").name, "tokyo-night");
        assert_eq!(theme_index("DRACULA"), 2);
    }

    #[test]
    fn themes_have_distinct_backgrounds() {
        let mut seen = std::collections::HashSet::new();
        for i in 0..THEME_NAMES.len() {
            assert!(seen.insert(theme_at(i).bg));
        }
    }
}
