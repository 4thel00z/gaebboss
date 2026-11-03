//! Colors from the RAL Classic swatches German construction already speaks:
//! signal yellow for focus, traffic red and orange for findings, on the
//! terminal's own background. Terminals without truecolor get the nearest
//! xterm-256 entries.

use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// RAL 1003 signal yellow: focus, selection, the mode chip.
    pub accent: Color,
    /// Text on an accent background.
    pub on_accent: Color,
    /// RAL 3020 traffic red, lifted for dark backgrounds.
    pub error: Color,
    /// RAL 2009 traffic orange.
    pub warning: Color,
    /// RAL 5015 sky blue: notes and XML tags.
    pub info: Color,
    /// RAL 6018 yellow green: totals that add up.
    pub ok: Color,
    /// Labels, rules, secondary text.
    pub muted: Color,
    /// Unfocused borders and guides.
    pub border: Color,
    /// The selected row's band.
    pub selection: Color,
    /// XML attribute names.
    pub attribute: Color,
}

impl Theme {
    pub fn truecolor() -> Theme {
        Theme {
            accent: Color::Rgb(0xF9, 0xA8, 0x00),
            on_accent: Color::Rgb(0x1A, 0x1A, 0x1A),
            error: Color::Rgb(0xE8, 0x45, 0x3C),
            warning: Color::Rgb(0xF0, 0x7A, 0x2D),
            info: Color::Rgb(0x5B, 0x9B, 0xD5),
            ok: Color::Rgb(0x57, 0xA6, 0x39),
            muted: Color::Rgb(0x8A, 0x8D, 0x86),
            border: Color::Rgb(0x4A, 0x4D, 0x48),
            selection: Color::Rgb(0x2E, 0x32, 0x29),
            attribute: Color::Rgb(0xD7, 0xB5, 0x6D),
        }
    }

    pub fn indexed() -> Theme {
        Theme {
            accent: Color::Indexed(214),
            on_accent: Color::Indexed(234),
            error: Color::Indexed(203),
            warning: Color::Indexed(208),
            info: Color::Indexed(74),
            ok: Color::Indexed(71),
            muted: Color::Indexed(245),
            border: Color::Indexed(239),
            selection: Color::Indexed(236),
            attribute: Color::Indexed(179),
        }
    }

    /// Truecolor when `COLORTERM` says so, the 256-color palette otherwise.
    pub fn detect() -> Theme {
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        match matches!(colorterm.as_str(), "truecolor" | "24bit") {
            true => Theme::truecolor(),
            false => Theme::indexed(),
        }
    }

    pub fn fg(&self, color: Color) -> Style {
        Style::default().fg(color)
    }

    pub fn muted(&self) -> Style {
        self.fg(self.muted)
    }

    pub fn bold(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    /// A key in the hint bar.
    pub fn key(&self) -> Style {
        self.fg(self.accent).add_modifier(Modifier::BOLD)
    }

    /// A filled label, `text` on `color`.
    pub fn chip(&self, color: Color) -> Style {
        Style::default()
            .fg(self.on_accent)
            .bg(color)
            .add_modifier(Modifier::BOLD)
    }
}

impl Default for Theme {
    fn default() -> Theme {
        Theme::truecolor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_palettes_keep_accent_and_findings_apart() {
        for theme in [Theme::truecolor(), Theme::indexed()] {
            assert_ne!(theme.accent, theme.error);
            assert_ne!(theme.error, theme.warning);
            assert_ne!(theme.border, theme.selection);
        }
    }
}
