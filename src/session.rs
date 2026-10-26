//! Versioned, atomic session files; restoration is prepared before replacing the workspace.
use crate::{
    app::App,
    filter::{FilterKind, FilterRule},
    format::{LogFormat, ViewMode},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::Write,
    path::{Path, PathBuf},
};
#[derive(Serialize, Deserialize)]
pub struct SavedRule {
    pub pattern: String,
    pub kind: FilterKind,
    pub regex: bool,
    pub case_sensitive: bool,
    pub enabled: bool,
}
#[derive(Serialize, Deserialize)]
pub struct SavedPane {
    pub path: PathBuf,
    pub rules: Vec<SavedRule>,
    pub fields: Vec<String>,
    pub format: LogFormat,
    pub view: ViewMode,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub selected_line: usize,
    pub bookmarks: BTreeSet<usize>,
    pub follow: bool,
    pub following: bool,
    pub reverse: bool,
    pub indent: bool,
    pub light: bool,
    pub search: String,
}
#[derive(Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub panes: Vec<SavedPane>,
    pub active: usize,
    #[serde(default)]
    pub history: Vec<String>,
    #[serde(default)]
    pub redaction_keys: Vec<String>,
}
impl SavedPane {
    pub fn capture(app: &App) -> Result<Self, String> {
        Ok(Self {
            path: std::fs::canonicalize(&app.reader.path).map_err(|e| e.to_string())?,
            rules: app
                .filter
                .rules
                .iter()
                .map(|r| SavedRule {
                    pattern: r.pattern.clone(),
                    kind: r.kind,
                    regex: r.is_regex,
                    case_sensitive: r.is_case_sensitive,
                    enabled: r.enabled,
                })
                .collect(),
            fields: app.format.visible_fields.clone(),
            format: app.format.selected_format,
            view: app.format.view_mode,
            scroll_row: app.ui_state.scroll_row,
            scroll_col: app.ui_state.scroll_col,
            selected_line: app.ui_state.selected_line,
            bookmarks: app.ui_state.bookmarks.clone(),
            follow: app.reader.is_tail_enabled(),
            following: app.ui_state.tail_following,
            reverse: app.ui_state.reverse_order,
            indent: app.ui_state.auto_indent_json,
            light: app.light_theme_enabled,
            search: app.ui_state.quick_search_term.clone(),
        })
    }
    pub fn restore(&self) -> Result<App, String> {
        if self.rules.len() > 512 {
            return Err("Too many saved filters".into());
        }
        let mut app = App::new(self.path.clone()).map_err(|e| e.to_string())?;
        // Compile all expressions before launching a single filtering pass.
        let rules: Result<Vec<_>, _> = self
            .rules
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut rule =
                    FilterRule::new(i + 1, r.pattern.clone(), r.kind, r.regex, r.case_sensitive)?;
                rule.enabled = r.enabled;
                Ok::<_, String>(rule)
            })
            .collect();
        app.filter.restore_rules(rules?);
        app.format.selected_format = self.format;
        app.format.view_mode = self.view;
        app.format.visible_fields = self.fields.clone();
        app.filter.sync_field_filter(
            &app.format.visible_fields,
            &app.format.available_fields,
            app.format.is_json(),
            &app.reader,
        );
        app.ui_state.scroll_row = self.scroll_row;
        app.ui_state.scroll_col = self.scroll_col;
        app.ui_state.selected_line = self.selected_line;
        app.ui_state.bookmarks = self.bookmarks.clone();
        app.reader.set_tail_enabled(self.follow);
        app.ui_state.tail_following = self.following;
        app.ui_state.reverse_order = self.reverse;
        app.ui_state.auto_indent_json = self.indent;
        app.ui_state.quick_search_term = self.search.clone();
        if self.light {
            app.execute_menu_action(crate::ui::menu::MenuAction::ToggleTheme);
        }
        Ok(app)
    }
}
impl Session {
    pub fn read(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
            return Err("Session file exceeds 8 MiB".into());
        }
        let session: Self = serde_json::from_reader(file).map_err(|e| e.to_string())?;
        if session.version != 1
            || session.panes.is_empty()
            || session.panes.len() > 2
            || session.active >= session.panes.len()
        {
            return Err("Unsupported or invalid Flik session".into());
        }
        Ok(session)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if path.exists() {
            // Never overwrite logs, exports or arbitrary documents with a session.
            Self::read(path)?;
            for pane in &self.panes {
                let source = std::fs::metadata(&pane.path).map_err(|e| e.to_string())?;
                let target = std::fs::metadata(path).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if source.ino() == target.ino() && source.dev() == target.dev() {
                        return Err("Session destination is an open log file".into());
                    }
                }
            }
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(&mut temp, self).map_err(|e| e.to_string())?;
        temp.flush().map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
