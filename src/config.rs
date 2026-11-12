//! Global configuration and custom format loading (~/.config/flik/config.toml & formats/*.toml)

use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct FlikConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub alerts: AlertsConfig,
    #[serde(default)]
    pub formats: Vec<CustomFormatConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct GeneralConfig {
    pub mouse_capture: Option<bool>,
    pub auto_indent_json: Option<bool>,
    pub reverse_order: Option<bool>,
    pub show_line_numbers: Option<bool>,
    pub enable_coloration: Option<bool>,
    pub theme: Option<String>,
    pub default_view: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AlertsConfig {
    pub enabled: Option<bool>,
    pub threshold: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomFormatConfig {
    pub name: String,
    pub pattern: String,
}

impl FlikConfig {
    /// Return the platform-specific standard config directory:
    /// - Windows: `%APPDATA%\flik`
    /// - Unix / macOS: `$XDG_CONFIG_HOME/flik` or `~/.config/flik`
    pub fn config_dir() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                return Some(PathBuf::from(appdata).join("flik"));
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
                if !xdg.trim().is_empty() {
                    return Some(PathBuf::from(xdg).join("flik"));
                }
            }
            if let Ok(home) = std::env::var("HOME") {
                return Some(PathBuf::from(home).join(".config").join("flik"));
            }
        }

        None
    }

    /// Load configuration from default paths if present.
    pub fn load() -> Self {
        if let Some(dir) = Self::config_dir() {
            Self::load_from_dir(&dir)
        } else {
            Self::default()
        }
    }

    /// Load configuration from a specific directory.
    pub fn load_from_dir(dir: &Path) -> Self {
        let mut config = Self::default();

        let config_file = dir.join("config.toml");
        if config_file.is_file() {
            if let Ok(content) = fs::read_to_string(&config_file) {
                if let Ok(parsed) = toml::from_str::<FlikConfig>(&content) {
                    config = parsed;
                }
            }
        }

        // Also scan formats/*.toml
        let formats_dir = dir.join("formats");
        if formats_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(formats_dir) {
                let mut files: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("toml"))
                    .collect();
                files.sort();

                for file in files {
                    if let Ok(content) = fs::read_to_string(&file) {
                        if let Ok(single) = toml::from_str::<CustomFormatConfig>(&content) {
                            config.formats.push(single);
                        } else if let Ok(wrapper) = toml::from_str::<FormatsWrapper>(&content) {
                            config.formats.extend(wrapper.format);
                        }
                    }
                }
            }
        }

        config
    }

    /// Apply loaded configuration to an App instance
    pub fn apply(&self, app: &mut crate::app::App) {
        if let Some(mc) = self.general.mouse_capture {
            app.mouse_capture_enabled = mc;
        }
        if let Some(ai) = self.general.auto_indent_json {
            app.ui_state.auto_indent_json = ai;
        }
        if let Some(ro) = self.general.reverse_order {
            app.ui_state.reverse_order = ro;
        }
        if let Some(sln) = self.general.show_line_numbers {
            app.ui_state.show_line_numbers = sln;
        }
        if let Some(ec) = self.general.enable_coloration {
            app.ui_state.enable_coloration = ec;
        }
        if let Some(ref theme_str) = self.general.theme {
            if let Some(mode) = crate::ui::theme::ThemeMode::from_str_name(theme_str) {
                app.theme_mode = mode;
                app.theme = mode.to_theme();
                app.light_theme_enabled = mode != crate::ui::theme::ThemeMode::DarkSlate;
            }
        }
        if let Some(ref dv) = self.general.default_view {
            match dv.to_lowercase().as_str() {
                "table" | "columnstable" => app.format.view_mode = crate::format::ViewMode::ColumnsTable,
                "long_split" | "longsplit" | "split3" => app.format.view_mode = crate::format::ViewMode::LongSplit,
                _ => app.format.view_mode = crate::format::ViewMode::CompactLine,
            }
        }
        if let Some(thresh) = self.alerts.threshold {
            app.ui_state.alert_threshold = thresh;
        }
    }

    /// Automatically persist updated general preferences to config.toml
    pub fn save_general_preference<F>(update_fn: F)
    where
        F: FnOnce(&mut GeneralConfig),
    {
        if let Some(dir) = Self::config_dir() {
            let _ = fs::create_dir_all(&dir);
            let mut config = Self::load_from_dir(&dir);
            update_fn(&mut config.general);
            let config_file = dir.join("config.toml");
            if let Ok(toml_str) = toml::to_string_pretty(&config) {
                let _ = fs::write(config_file, toml_str);
            }
        }
    }

    /// Find a custom format by name (case-insensitive)
    pub fn find_format(&self, name: &str) -> Option<&CustomFormatConfig> {
        self.formats.iter().find(|f| f.name.eq_ignore_ascii_case(name))
    }
}

#[derive(Deserialize)]
struct FormatsWrapper {
    #[serde(default)]
    format: Vec<CustomFormatConfig>,
}

