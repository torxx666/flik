use ratatui::style::{Color, Modifier, Style};

#[allow(dead_code)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub frame_border: Style,
    pub frame_title: Style,
    
    // Top menu
    pub menu_bar_bg: Color,
    pub menu_bar_fg: Color,
    pub menu_hotkey: Style,
    pub menu_selected: Style,
    pub menu_dropdown_bg: Color,
    pub menu_dropdown_fg: Color,
    pub menu_dropdown_border: Style,
    
    // Bottom function keys
    pub fn_key_num: Style,
    pub fn_key_label: Style,
    pub fn_key_bg: Color,
    
    // Status bar
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
    pub status_accent: Style,
    pub status_badge: Style,
    
    // Log levels
    pub level_info: Style,
    pub level_warn: Style,
    pub level_error: Style,
    pub level_debug: Style,
    pub level_trace: Style,
    
    // HTTP Status codes
    pub status_2xx: Style,
    pub status_3xx: Style,
    pub status_4xx: Style,
    pub status_5xx: Style,

    // Gutter / Line Numbers
    pub line_number: Style,
    pub timestamp: Style,
    pub field_key: Style,
    pub json_key: Style,
    pub json_string: Style,
    pub json_number: Style,
    pub json_literal: Style,
    pub json_punctuation: Style,
    pub json_colon: Style,
    pub highlight_match: Style,
    pub cursor_line: Style,
    
    // Modal
    pub modal_bg: Color,
    pub modal_border: Style,
    pub modal_title: Style,
    pub modal_input_bg: Color,
    pub modal_selected: Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    DarkSlate,
    NordLight,
    SolarizedLight,
    WarmSepia,
}

impl ThemeMode {
    pub fn next(&self) -> Self {
        match self {
            ThemeMode::DarkSlate => ThemeMode::NordLight,
            ThemeMode::NordLight => ThemeMode::SolarizedLight,
            ThemeMode::SolarizedLight => ThemeMode::WarmSepia,
            ThemeMode::WarmSepia => ThemeMode::DarkSlate,
        }
    }

    pub fn to_theme(&self) -> Theme {
        match self {
            ThemeMode::DarkSlate => Theme::default(),
            ThemeMode::NordLight => Theme::nord_light(),
            ThemeMode::SolarizedLight => Theme::solarized_light(),
            ThemeMode::WarmSepia => Theme::warm_sepia(),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ThemeMode::DarkSlate => "Dark Slate",
            ThemeMode::NordLight => "Nord Polar Light",
            ThemeMode::SolarizedLight => "Solarized Sand",
            ThemeMode::WarmSepia => "Warm Sepia",
        }
    }
}

impl Theme {
    /// Default light theme: Nord Polar Light (mineral soft-gray/stone, dark slate text, zero yellow glare).
    pub fn light() -> Self {
        Self::nord_light()
    }

    /// Nord Polar Light / Catppuccin Latte: Cool mineral light stone (#ECEFF4),
    /// deep crisp slate (#2E3440), frosty ice blue / teal / coral accents. Modern, crisp, zero yellow glare.
    pub fn nord_light() -> Self {
        Self {
            bg: Color::Rgb(236, 239, 244),             // Nord Snow Storm (#ECEFF4)
            fg: Color::Rgb(46, 52, 64),                // Nord Polar Night (#2E3440)
            frame_border: Style::default().fg(Color::Rgb(160, 175, 195)),
            frame_title: Style::default().fg(Color::Rgb(94, 129, 172)).add_modifier(Modifier::BOLD),
            menu_bar_bg: Color::Rgb(216, 222, 233),   // Nord (#D8DEE9)
            menu_bar_fg: Color::Rgb(46, 52, 64),
            menu_hotkey: Style::default().fg(Color::Rgb(208, 135, 112)).add_modifier(Modifier::BOLD),
            menu_selected: Style::default()
                .bg(Color::Rgb(94, 129, 172))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            menu_dropdown_bg: Color::Rgb(236, 239, 244),
            menu_dropdown_fg: Color::Rgb(46, 52, 64),
            menu_dropdown_border: Style::default().fg(Color::Rgb(94, 129, 172)),
            fn_key_num: Style::default()
                .bg(Color::Rgb(94, 129, 172))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            fn_key_label: Style::default()
                .bg(Color::Rgb(216, 222, 233))
                .fg(Color::Rgb(46, 52, 64)),
            fn_key_bg: Color::Rgb(229, 233, 240),
            status_bar_bg: Color::Rgb(216, 222, 233),
            status_bar_fg: Color::Rgb(46, 52, 64),
            status_accent: Style::default().fg(Color::Rgb(94, 129, 172)).add_modifier(Modifier::BOLD),
            status_badge: Style::default()
                .bg(Color::Rgb(94, 129, 172))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            level_info: Style::default().fg(Color::Rgb(65, 140, 75)).add_modifier(Modifier::BOLD),
            level_warn: Style::default().fg(Color::Rgb(185, 100, 25)).add_modifier(Modifier::BOLD),
            level_error: Style::default().fg(Color::Rgb(191, 55, 65)).add_modifier(Modifier::BOLD),
            level_debug: Style::default().fg(Color::Rgb(35, 120, 165)),
            level_trace: Style::default().fg(Color::Rgb(115, 130, 150)),
            status_2xx: Style::default().fg(Color::Rgb(65, 140, 75)),
            status_3xx: Style::default().fg(Color::Rgb(35, 120, 165)),
            status_4xx: Style::default().fg(Color::Rgb(185, 100, 25)),
            status_5xx: Style::default().fg(Color::Rgb(191, 55, 65)).add_modifier(Modifier::BOLD),
            line_number: Style::default().fg(Color::Rgb(130, 142, 160)),
            timestamp: Style::default().fg(Color::Rgb(70, 110, 155)),
            field_key: Style::default().fg(Color::Rgb(94, 129, 172)),
            json_key: Style::default().fg(Color::Rgb(94, 129, 172)).add_modifier(Modifier::BOLD),
            json_string: Style::default().fg(Color::Rgb(65, 140, 75)),
            json_number: Style::default().fg(Color::Rgb(180, 100, 40)),
            json_literal: Style::default().fg(Color::Rgb(150, 85, 160)),
            json_punctuation: Style::default().fg(Color::Rgb(130, 142, 160)),
            json_colon: Style::default().fg(Color::Rgb(76, 86, 106)).add_modifier(Modifier::BOLD),
            highlight_match: Style::default()
                .bg(Color::Rgb(235, 195, 80))
                .fg(Color::Rgb(46, 52, 64))
                .add_modifier(Modifier::BOLD),
            cursor_line: Style::default().bg(Color::Rgb(220, 227, 238)),
            modal_bg: Color::Rgb(236, 239, 244),
            modal_border: Style::default().fg(Color::Rgb(94, 129, 172)),
            modal_title: Style::default().fg(Color::Rgb(94, 129, 172)).add_modifier(Modifier::BOLD),
            modal_input_bg: Color::Rgb(222, 227, 236),
            modal_selected: Style::default()
                .bg(Color::Rgb(94, 129, 172))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// Solarized Sand: Mathematically calibrated in CIELAB color space by Ethan Schoonover.
    /// Base3 (#FDF6E3) warm sand background with soft slate text and balanced chromatic accents.
    pub fn solarized_light() -> Self {
        Self {
            bg: Color::Rgb(253, 246, 227),             // Solarized base3 (#FDF6E3)
            fg: Color::Rgb(101, 123, 131),             // Solarized base00 (#657B83)
            frame_border: Style::default().fg(Color::Rgb(147, 161, 161)),
            frame_title: Style::default().fg(Color::Rgb(38, 139, 210)).add_modifier(Modifier::BOLD),
            menu_bar_bg: Color::Rgb(238, 232, 213),   // Solarized base2 (#EEE8D5)
            menu_bar_fg: Color::Rgb(88, 110, 117),
            menu_hotkey: Style::default().fg(Color::Rgb(203, 75, 22)).add_modifier(Modifier::BOLD),
            menu_selected: Style::default()
                .bg(Color::Rgb(38, 139, 210))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            menu_dropdown_bg: Color::Rgb(253, 246, 227),
            menu_dropdown_fg: Color::Rgb(88, 110, 117),
            menu_dropdown_border: Style::default().fg(Color::Rgb(38, 139, 210)),
            fn_key_num: Style::default()
                .bg(Color::Rgb(38, 139, 210))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            fn_key_label: Style::default()
                .bg(Color::Rgb(238, 232, 213))
                .fg(Color::Rgb(88, 110, 117)),
            fn_key_bg: Color::Rgb(245, 239, 220),
            status_bar_bg: Color::Rgb(238, 232, 213),
            status_bar_fg: Color::Rgb(88, 110, 117),
            status_accent: Style::default().fg(Color::Rgb(38, 139, 210)).add_modifier(Modifier::BOLD),
            status_badge: Style::default()
                .bg(Color::Rgb(38, 139, 210))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            level_info: Style::default().fg(Color::Rgb(133, 153, 0)).add_modifier(Modifier::BOLD),
            level_warn: Style::default().fg(Color::Rgb(181, 137, 0)).add_modifier(Modifier::BOLD),
            level_error: Style::default().fg(Color::Rgb(220, 50, 47)).add_modifier(Modifier::BOLD),
            level_debug: Style::default().fg(Color::Rgb(42, 161, 152)),
            level_trace: Style::default().fg(Color::Rgb(147, 161, 161)),
            status_2xx: Style::default().fg(Color::Rgb(133, 153, 0)),
            status_3xx: Style::default().fg(Color::Rgb(42, 161, 152)),
            status_4xx: Style::default().fg(Color::Rgb(181, 137, 0)),
            status_5xx: Style::default().fg(Color::Rgb(220, 50, 47)).add_modifier(Modifier::BOLD),
            line_number: Style::default().fg(Color::Rgb(147, 161, 161)),
            timestamp: Style::default().fg(Color::Rgb(38, 139, 210)),
            field_key: Style::default().fg(Color::Rgb(42, 161, 152)),
            json_key: Style::default().fg(Color::Rgb(38, 139, 210)).add_modifier(Modifier::BOLD),
            json_string: Style::default().fg(Color::Rgb(133, 153, 0)),
            json_number: Style::default().fg(Color::Rgb(211, 54, 130)),
            json_literal: Style::default().fg(Color::Rgb(108, 113, 196)),
            json_punctuation: Style::default().fg(Color::Rgb(147, 161, 161)),
            json_colon: Style::default().fg(Color::Rgb(88, 110, 117)).add_modifier(Modifier::BOLD),
            highlight_match: Style::default()
                .bg(Color::Rgb(181, 137, 0))
                .fg(Color::Rgb(253, 246, 227))
                .add_modifier(Modifier::BOLD),
            cursor_line: Style::default().bg(Color::Rgb(238, 232, 213)),
            modal_bg: Color::Rgb(253, 246, 227),
            modal_border: Style::default().fg(Color::Rgb(38, 139, 210)),
            modal_title: Style::default().fg(Color::Rgb(203, 75, 22)).add_modifier(Modifier::BOLD),
            modal_input_bg: Color::Rgb(238, 232, 213),
            modal_selected: Style::default()
                .bg(Color::Rgb(38, 139, 210))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// Warm Sepia eye-comfort light theme (parchment background, deep charcoal umber text).
    pub fn warm_sepia() -> Self {
        Self {
            bg: Color::Rgb(244, 238, 224),             // Warm Sepia Parchment (#F4EEE0)
            fg: Color::Rgb(58, 49, 44),                // Deep Charcoal Umber (#3A312C)
            frame_border: Style::default().fg(Color::Rgb(181, 163, 143)),
            frame_title: Style::default().fg(Color::Rgb(175, 58, 3)).add_modifier(Modifier::BOLD),
            menu_bar_bg: Color::Rgb(230, 220, 202),
            menu_bar_fg: Color::Rgb(58, 49, 44),
            menu_hotkey: Style::default().fg(Color::Rgb(175, 58, 3)).add_modifier(Modifier::BOLD),
            menu_selected: Style::default()
                .bg(Color::Rgb(175, 58, 3))
                .fg(Color::Rgb(254, 250, 242))
                .add_modifier(Modifier::BOLD),
            menu_dropdown_bg: Color::Rgb(246, 241, 230),
            menu_dropdown_fg: Color::Rgb(58, 49, 44),
            menu_dropdown_border: Style::default().fg(Color::Rgb(175, 58, 3)),
            fn_key_num: Style::default()
                .bg(Color::Rgb(175, 58, 3))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            fn_key_label: Style::default()
                .bg(Color::Rgb(230, 220, 202))
                .fg(Color::Rgb(58, 49, 44)),
            fn_key_bg: Color::Rgb(238, 230, 214),
            status_bar_bg: Color::Rgb(230, 220, 202),
            status_bar_fg: Color::Rgb(58, 49, 44),
            status_accent: Style::default().fg(Color::Rgb(175, 58, 3)).add_modifier(Modifier::BOLD),
            status_badge: Style::default()
                .bg(Color::Rgb(175, 58, 3))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            level_info: Style::default().fg(Color::Rgb(68, 115, 23)).add_modifier(Modifier::BOLD),
            level_warn: Style::default().fg(Color::Rgb(175, 95, 0)).add_modifier(Modifier::BOLD),
            level_error: Style::default().fg(Color::Rgb(186, 28, 28)).add_modifier(Modifier::BOLD),
            level_debug: Style::default().fg(Color::Rgb(20, 95, 128)),
            level_trace: Style::default().fg(Color::Rgb(138, 125, 114)),
            status_2xx: Style::default().fg(Color::Rgb(68, 115, 23)),
            status_3xx: Style::default().fg(Color::Rgb(20, 95, 128)),
            status_4xx: Style::default().fg(Color::Rgb(175, 95, 0)),
            status_5xx: Style::default().fg(Color::Rgb(186, 28, 28)).add_modifier(Modifier::BOLD),
            line_number: Style::default().fg(Color::Rgb(150, 137, 125)),
            timestamp: Style::default().fg(Color::Rgb(32, 94, 125)),
            field_key: Style::default().fg(Color::Rgb(140, 70, 20)),
            json_key: Style::default().fg(Color::Rgb(140, 70, 20)).add_modifier(Modifier::BOLD),
            json_string: Style::default().fg(Color::Rgb(68, 115, 23)),
            json_number: Style::default().fg(Color::Rgb(175, 95, 0)),
            json_literal: Style::default().fg(Color::Rgb(128, 52, 142)),
            json_punctuation: Style::default().fg(Color::Rgb(138, 125, 114)),
            json_colon: Style::default().fg(Color::Rgb(95, 80, 70)).add_modifier(Modifier::BOLD),
            highlight_match: Style::default()
                .bg(Color::Rgb(245, 205, 80))
                .fg(Color::Rgb(40, 30, 10))
                .add_modifier(Modifier::BOLD),
            cursor_line: Style::default().bg(Color::Rgb(235, 226, 210)),
            modal_bg: Color::Rgb(246, 241, 230),
            modal_border: Style::default().fg(Color::Rgb(175, 58, 3)),
            modal_title: Style::default().fg(Color::Rgb(175, 58, 3)).add_modifier(Modifier::BOLD),
            modal_input_bg: Color::Rgb(234, 225, 208),
            modal_selected: Style::default()
                .bg(Color::Rgb(175, 58, 3))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::Rgb(15, 17, 23),             // Sleek Dark Slate
            fg: Color::Rgb(220, 225, 235),          // Crisp light text
            frame_border: Style::default().fg(Color::Rgb(70, 85, 110)),
            frame_title: Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),

            // Top Menu
            menu_bar_bg: Color::Rgb(30, 36, 50),
            menu_bar_fg: Color::Rgb(200, 210, 225),
            menu_hotkey: Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            menu_selected: Style::default()
                .bg(Color::Rgb(45, 90, 160))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            menu_dropdown_bg: Color::Rgb(24, 28, 40),
            menu_dropdown_fg: Color::Rgb(210, 220, 235),
            menu_dropdown_border: Style::default().fg(Color::Rgb(80, 110, 150)),

            // Bottom Function Keys
            fn_key_num: Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            fn_key_label: Style::default()
                .bg(Color::Rgb(35, 42, 60))
                .fg(Color::Rgb(220, 230, 245)),
            fn_key_bg: Color::Rgb(25, 30, 42),

            // Status bar
            status_bar_bg: Color::Rgb(25, 32, 46),
            status_bar_fg: Color::Rgb(180, 195, 215),
            status_accent: Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            status_badge: Style::default()
                .bg(Color::Rgb(40, 60, 90))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),

            // Log levels
            level_info: Style::default()
                .fg(Color::Rgb(80, 220, 120))
                .add_modifier(Modifier::BOLD),
            level_warn: Style::default()
                .fg(Color::Rgb(255, 190, 60))
                .add_modifier(Modifier::BOLD),
            level_error: Style::default()
                .fg(Color::Rgb(255, 80, 80))
                .add_modifier(Modifier::BOLD),
            level_debug: Style::default()
                .fg(Color::Rgb(100, 180, 255)),
            level_trace: Style::default()
                .fg(Color::Rgb(160, 160, 175)),

            // HTTP Status
            status_2xx: Style::default().fg(Color::Rgb(80, 220, 120)),
            status_3xx: Style::default().fg(Color::Rgb(90, 200, 255)),
            status_4xx: Style::default().fg(Color::Rgb(255, 190, 60)),
            status_5xx: Style::default().fg(Color::Rgb(255, 80, 80)).add_modifier(Modifier::BOLD),

            // Gutter & Highlights
            line_number: Style::default().fg(Color::Rgb(90, 105, 130)),
            timestamp: Style::default().fg(Color::Rgb(110, 160, 210)),
            field_key: Style::default().fg(Color::Rgb(140, 175, 220)),
            json_key: Style::default().fg(Color::Rgb(110, 210, 255)).add_modifier(Modifier::BOLD),
            json_string: Style::default().fg(Color::Rgb(165, 235, 175)),
            json_number: Style::default().fg(Color::Rgb(255, 215, 100)),
            json_literal: Style::default().fg(Color::Rgb(215, 150, 255)),
            json_punctuation: Style::default().fg(Color::Rgb(100, 120, 150)),
            json_colon: Style::default().fg(Color::Rgb(130, 145, 175)).add_modifier(Modifier::BOLD),
            highlight_match: Style::default()
                .bg(Color::Rgb(220, 180, 30))
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            cursor_line: Style::default().bg(Color::Rgb(30, 38, 54)),

            // Modal
            modal_bg: Color::Rgb(22, 26, 38),
            modal_border: Style::default().fg(Color::Cyan),
            modal_title: Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            modal_input_bg: Color::Rgb(14, 17, 25),
            modal_selected: Style::default()
                .bg(Color::Rgb(45, 95, 170))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Theme;
    use ratatui::style::{Color, Style};

    #[test]
    fn light_theme_has_a_distinct_light_surface_and_selection_palette() {
        let dark = Theme::default();
        let light = Theme::light();

        assert_eq!(light.bg, Color::Rgb(236, 239, 244));
        assert_ne!(light.bg, dark.bg);
        assert_ne!(light.menu_bar_bg, dark.menu_bar_bg);
        assert_ne!(light.cursor_line, dark.cursor_line);
        assert_eq!(light.json_string, Style::default().fg(Color::Rgb(65, 140, 75)));
        assert_ne!(light.json_string, dark.json_string);
    }
}
