use ratatui::style::{Color, Modifier, Style};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Palette {
    pub fg: Color,
    pub bg: Color,
    pub dim: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_fg: Color,
    pub added: Color,
    pub removed: Color,
    pub warning: Color,
    pub syn_keyword: Color,
    pub syn_string: Color,
    pub syn_comment: Color,
    pub syn_type: Color,
    pub syn_function: Color,
    pub syn_constant: Color,
    pub syn_variable: Color,
    pub syn_operator: Color,
}

impl Palette {
    pub fn plain() -> Self {
        Self {
            fg: Color::White,
            bg: Color::Reset,
            dim: Color::DarkGray,
            border: Color::White,
            accent: Color::White,
            accent_fg: Color::Black,
            added: Color::Green,
            removed: Color::Red,
            warning: Color::Yellow,
            syn_keyword: Color::Magenta,
            syn_string: Color::Green,
            syn_comment: Color::DarkGray,
            syn_type: Color::Cyan,
            syn_function: Color::Blue,
            syn_constant: Color::Yellow,
            syn_variable: Color::White,
            syn_operator: Color::Yellow,
        }
    }

    pub fn catppuccin() -> Self {
        Self {
            fg: Color::Rgb(205, 214, 244),
            bg: Color::Rgb(30, 30, 46),
            dim: Color::Rgb(88, 91, 112),
            border: Color::Rgb(69, 71, 90),
            accent: Color::Rgb(203, 166, 247),
            accent_fg: Color::Rgb(30, 30, 46),
            added: Color::Rgb(166, 227, 161),
            removed: Color::Rgb(243, 139, 168),
            warning: Color::Rgb(249, 226, 175),
            syn_keyword: Color::Rgb(203, 166, 247),
            syn_string: Color::Rgb(166, 227, 161),
            syn_comment: Color::Rgb(88, 91, 112),
            syn_type: Color::Rgb(137, 220, 235),
            syn_function: Color::Rgb(137, 180, 250),
            syn_constant: Color::Rgb(250, 179, 135),
            syn_variable: Color::Rgb(205, 214, 244),
            syn_operator: Color::Rgb(148, 226, 213),
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            fg: Color::Rgb(192, 202, 245),
            bg: Color::Rgb(26, 27, 38),
            dim: Color::Rgb(68, 75, 106),
            border: Color::Rgb(41, 46, 66),
            accent: Color::Rgb(122, 162, 247),
            accent_fg: Color::Rgb(26, 27, 38),
            added: Color::Rgb(158, 206, 106),
            removed: Color::Rgb(247, 118, 142),
            warning: Color::Rgb(224, 175, 104),
            syn_keyword: Color::Rgb(187, 154, 247),
            syn_string: Color::Rgb(158, 206, 106),
            syn_comment: Color::Rgb(68, 75, 106),
            syn_type: Color::Rgb(122, 162, 247),
            syn_function: Color::Rgb(122, 162, 247),
            syn_constant: Color::Rgb(255, 158, 100),
            syn_variable: Color::Rgb(192, 202, 245),
            syn_operator: Color::Rgb(137, 221, 255),
        }
    }

    pub fn nord() -> Self {
        Self {
            fg: Color::Rgb(216, 222, 233),
            bg: Color::Rgb(46, 52, 64),
            dim: Color::Rgb(76, 86, 106),
            border: Color::Rgb(59, 66, 82),
            accent: Color::Rgb(136, 192, 208),
            accent_fg: Color::Rgb(46, 52, 64),
            added: Color::Rgb(163, 190, 140),
            removed: Color::Rgb(191, 97, 106),
            warning: Color::Rgb(235, 203, 139),
            syn_keyword: Color::Rgb(180, 142, 173),
            syn_string: Color::Rgb(163, 190, 140),
            syn_comment: Color::Rgb(76, 86, 106),
            syn_type: Color::Rgb(136, 192, 208),
            syn_function: Color::Rgb(136, 192, 208),
            syn_constant: Color::Rgb(208, 135, 112),
            syn_variable: Color::Rgb(216, 222, 233),
            syn_operator: Color::Rgb(143, 188, 187),
        }
    }

    pub fn gruvbox() -> Self {
        Self {
            fg: Color::Rgb(235, 219, 178),
            bg: Color::Rgb(40, 40, 40),
            dim: Color::Rgb(146, 131, 116),
            border: Color::Rgb(60, 56, 54),
            accent: Color::Rgb(250, 189, 47),
            accent_fg: Color::Rgb(40, 40, 40),
            added: Color::Rgb(184, 187, 38),
            removed: Color::Rgb(251, 73, 52),
            warning: Color::Rgb(250, 189, 47),
            syn_keyword: Color::Rgb(211, 134, 155),
            syn_string: Color::Rgb(184, 187, 38),
            syn_comment: Color::Rgb(146, 131, 116),
            syn_type: Color::Rgb(142, 192, 124),
            syn_function: Color::Rgb(142, 192, 124),
            syn_constant: Color::Rgb(254, 128, 25),
            syn_variable: Color::Rgb(235, 219, 178),
            syn_operator: Color::Rgb(131, 165, 152),
        }
    }

    pub fn one_light() -> Self {
        Self {
            fg: Color::Rgb(56, 58, 66),
            bg: Color::Rgb(250, 250, 250),
            dim: Color::Rgb(160, 161, 167),
            border: Color::Rgb(220, 221, 225),
            accent: Color::Rgb(64, 120, 242),
            accent_fg: Color::Rgb(250, 250, 250),
            added: Color::Rgb(80, 161, 79),
            removed: Color::Rgb(228, 86, 73),
            warning: Color::Rgb(193, 132, 1),
            syn_keyword: Color::Rgb(166, 38, 164),
            syn_string: Color::Rgb(80, 161, 79),
            syn_comment: Color::Rgb(160, 161, 167),
            syn_type: Color::Rgb(64, 120, 242),
            syn_function: Color::Rgb(64, 120, 242),
            syn_constant: Color::Rgb(152, 104, 1),
            syn_variable: Color::Rgb(56, 58, 66),
            syn_operator: Color::Rgb(1, 132, 188),
        }
    }

    pub fn highlight_style(&self) -> Style {
        Style::new().fg(self.accent_fg).bg(self.accent)
    }

    pub fn to_highlight_map(&self) -> HashMap<String, Style> {
        let mut m = HashMap::new();
        m.insert(
            "keyword".into(),
            Style::new()
                .fg(self.syn_keyword)
                .add_modifier(Modifier::BOLD),
        );
        m.insert("function".into(), Style::new().fg(self.syn_function));
        m.insert("function.builtin".into(), Style::new().fg(self.syn_type));
        m.insert("string".into(), Style::new().fg(self.syn_string));
        m.insert("string.special".into(), Style::new().fg(self.syn_type));
        m.insert("comment".into(), Style::new().fg(self.syn_comment));
        m.insert("type".into(), Style::new().fg(self.syn_type));
        m.insert("type.builtin".into(), Style::new().fg(self.syn_type));
        m.insert("constant".into(), Style::new().fg(self.syn_constant));
        m.insert("variable".into(), Style::new().fg(self.syn_variable));
        m.insert("variable.builtin".into(), Style::new().fg(self.syn_type));
        m.insert(
            "variable.parameter".into(),
            Style::new().fg(self.syn_variable),
        );
        m.insert("operator".into(), Style::new().fg(self.syn_operator));
        m.insert("punctuation".into(), Style::new().fg(self.dim));
        m.insert("punctuation.bracket".into(), Style::new().fg(self.dim));
        m.insert("punctuation.delimiter".into(), Style::new().fg(self.dim));
        m.insert("property".into(), Style::new().fg(self.syn_variable));
        m.insert("attribute".into(), Style::new().fg(self.syn_constant));
        m.insert("tag".into(), Style::new().fg(self.syn_type));
        m.insert(
            "text.title".into(),
            Style::new()
                .fg(self.syn_keyword)
                .add_modifier(Modifier::BOLD),
        );
        m.insert("text.literal".into(), Style::new().fg(self.syn_string));
        m.insert("text.emphasis".into(), Style::new().fg(self.accent));
        m.insert(
            "text.strong".into(),
            Style::new().fg(self.accent).add_modifier(Modifier::BOLD),
        );
        m.insert(
            "text.uri".into(),
            Style::new()
                .fg(self.syn_type)
                .add_modifier(Modifier::UNDERLINED),
        );
        m.insert("text.reference".into(), Style::new().fg(self.syn_type));
        m.insert("punctuation.special".into(), Style::new().fg(self.dim));
        m.insert("string.escape".into(), Style::new().fg(self.syn_constant));
        m
    }
}

pub type ThemeFactory = fn() -> Palette;

pub static THEMES: &[(&str, ThemeFactory)] = &[
    ("plain", Palette::plain),
    ("catppuccin", Palette::catppuccin),
    ("tokyo-night", Palette::tokyo_night),
    ("nord", Palette::nord),
    ("gruvbox", Palette::gruvbox),
    ("one-light", Palette::one_light),
];

pub fn find_theme(name: &str) -> Option<ThemeFactory> {
    THEMES.iter().find(|(n, _)| *n == name).map(|(_, f)| *f)
}

pub fn default_theme_name() -> &'static str {
    "plain"
}

pub fn resolve_palette(name: Option<&str>) -> Palette {
    name.and_then(find_theme).unwrap_or(Palette::plain)()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_themes_have_non_default_colors() {
        for (name, factory) in THEMES {
            let p = factory();
            assert_ne!(p.fg, Color::Reset, "{name}: fg must be set");
            assert_ne!(p.accent, Color::Reset, "{name}: accent must be set");
        }
    }

    #[test]
    fn test_plain_theme_matches_expected() {
        let p = Palette::plain();
        assert_eq!(p.fg, Color::White);
        assert_eq!(p.dim, Color::DarkGray);
        assert_eq!(p.accent, Color::White);
        assert_eq!(p.accent_fg, Color::Black);
    }

    #[test]
    fn test_find_theme_case_sensitive() {
        assert!(find_theme("catppuccin").is_some());
        assert!(find_theme("nonexistent").is_none());
    }

    #[test]
    fn test_resolve_palette_falls_back_to_plain() {
        let p = resolve_palette(Some("bogus"));
        assert_eq!(p.fg, Color::White);
    }

    #[test]
    fn test_resolve_palette_loads_named_theme() {
        let p = resolve_palette(Some("nord"));
        assert_eq!(p.fg, Color::Rgb(216, 222, 233));
    }

    #[test]
    fn test_highlight_map_is_non_empty() {
        let p = Palette::plain();
        let map = p.to_highlight_map();
        assert!(!map.is_empty());
        assert!(map.contains_key("keyword"));
        assert!(map.contains_key("comment"));
        assert!(map.contains_key("string"));
    }

    #[test]
    fn test_highlight_style_is_fg_on_accent() {
        let p = Palette::plain();
        let s = p.highlight_style();
        assert_eq!(s.fg, Some(Color::Black));
        assert_eq!(s.bg, Some(Color::White));
    }
}
