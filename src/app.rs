use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::Backend;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

use crate::export::{ExportEngine, ExportFormat, ExportScope};
use crate::filter::{FilterEngine, FilterKind};
use crate::format::{FormatManager, LogFormat};
use crate::reader::LogReader;
use crate::ui::menu::{MenuAction, MenuBar};
use crate::ui::modals::ActiveModal;
use crate::ui::theme::Theme;
use crate::ui::{FlikUi, UiState, source_display_index};

pub struct App {
    completion: Option<(Vec<String>, usize)>,
    indexer_handle: Option<std::thread::JoinHandle<()>>,
    pub(crate) pending_workspace_action: Option<MenuAction>,
    pub reader: LogReader,
    pub filter: FilterEngine,
    pub format: FormatManager,
    pub theme: Theme,
    pub theme_mode: crate::ui::theme::ThemeMode,
    pub light_theme_enabled: bool,
    pub menu: MenuBar,
    pub modal: ActiveModal,
    pub ui_state: UiState,
    pub should_quit: bool,
    pub last_q_press: Option<Instant>,
    pub mouse_capture_enabled: bool,
    pub last_click: Option<(Instant, u16, u16)>,
}

fn copy_text_to_clipboard(text: &str) {
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        if let Ok(mut child) = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }

    fn b64_encode(bytes: &[u8]) -> String {
        const CHARSET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = if chunk.len() > 1 {
                chunk[1] as usize
            } else {
                0
            };
            let b2 = if chunk.len() > 2 {
                chunk[2] as usize
            } else {
                0
            };
            let triple = (b0 << 16) | (b1 << 8) | b2;
            out.push(CHARSET[(triple >> 18) & 0x3F] as char);
            out.push(CHARSET[(triple >> 12) & 0x3F] as char);
            if chunk.len() > 1 {
                out.push(CHARSET[(triple >> 6) & 0x3F] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(CHARSET[triple & 0x3F] as char);
            } else {
                out.push('=');
            }
        }
        out
    }

    let b64 = b64_encode(text.as_bytes());
    let osc52 = format!("\x1b]52;c;{}\x07", b64);
    let _ = std::io::Write::write_all(&mut std::io::stdout(), osc52.as_bytes());
}

fn extract_word_at_pos(line: &str, pos: usize) -> Option<String> {
    if line.is_empty() {
        return None;
    }
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    if chars.is_empty() {
        return None;
    }

    let target_idx = chars
        .iter()
        .position(|&(byte_idx, _)| byte_idx >= pos)
        .unwrap_or(chars.len().saturating_sub(1));

    let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-' || c == '.';

    let mut word_char_idx = None;
    if is_word_char(chars[target_idx].1) {
        word_char_idx = Some(target_idx);
    } else if target_idx > 0 && is_word_char(chars[target_idx - 1].1) {
        word_char_idx = Some(target_idx - 1);
    } else if target_idx + 1 < chars.len() && is_word_char(chars[target_idx + 1].1) {
        word_char_idx = Some(target_idx + 1);
    }

    let center = word_char_idx?;

    let mut start = center;
    while start > 0 && is_word_char(chars[start - 1].1) {
        start -= 1;
    }

    let mut end = center;
    while end + 1 < chars.len() && is_word_char(chars[end + 1].1) {
        end += 1;
    }

    let start_byte = chars[start].0;
    let end_byte = if end + 1 < chars.len() {
        chars[end + 1].0
    } else {
        line.len()
    };

    let word = line[start_byte..end_byte]
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches(':')
        .trim_matches(',')
        .trim();

    if !word.is_empty() {
        Some(word.to_string())
    } else {
        None
    }
}

fn clicked_line_offset(mouse_row: u16) -> Option<usize> {
    usize::from(mouse_row).checked_sub(2)
}

fn clicked_text_offset(mouse_column: u16, gutter_width: usize, scroll_col: usize) -> usize {
    usize::from(mouse_column).saturating_sub(gutter_width + 1) + scroll_col
}

fn byte_offset_for_display_column(text: &str, target_column: usize) -> usize {
    let mut column = 0;
    for (byte_offset, character) in text.char_indices() {
        if column >= target_column {
            return byte_offset;
        }
        column += UnicodeWidthChar::width(character).unwrap_or(0);
        if column > target_column {
            return byte_offset;
        }
    }
    text.len()
}

fn mouse_text_for_line(line: &str, format: &FormatManager, auto_indent_json: bool) -> String {
    if format.active_format() != LogFormat::Kubernetes {
        return line.to_string();
    }

    let record = format.parse_line(line);
    let has_field = |field: &str| format.visible_fields.iter().any(|visible| visible == field);
    let mut rendered = String::new();
    if has_field("timestamp") {
        if let Some(timestamp) = &record.timestamp {
            rendered.push_str(timestamp);
            rendered.push(' ');
        }
    }
    if has_field("stream") {
        if let Some(stream) = record.get_field_value("stream") {
            rendered.push_str(&format!("[{}] ", stream.to_uppercase()));
        }
    }
    if has_field("flag") {
        if let Some(flag) = record.get_field_value("flag") {
            rendered.push_str(flag);
            rendered.push(' ');
        }
    }
    for field in &format.visible_fields {
        if matches!(field.as_str(), "timestamp" | "stream" | "flag" | "message") {
            continue;
        }
        if let Some(value) = record.get_field_value(field) {
            rendered.push_str(&format!("{}: {} ", field, value));
        }
    }
    let message_is_json = serde_json::from_str::<serde_json::Value>(&record.message).is_ok();
    if has_field("message") && !(auto_indent_json && message_is_json) {
        rendered.push_str(&record.message);
    }
    rendered
}

impl App {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        let (reader, handle) = LogReader::open(&path)?;
        reader.set_tail_enabled(true);
        let filter = FilterEngine::new();
        let mut format = FormatManager::new();
        let theme = Theme::default();
        let menu = MenuBar::new();
        let modal = ActiveModal::None;
        let ui_state = UiState::new();

        // Sample first 100 lines to detect format immediately
        let initial_sample = reader.read_range(0, 100);
        let sample_lines: Vec<String> = initial_sample.into_iter().map(|(_, l)| l).collect();
        format.detect_and_index_fields(&sample_lines);
        let mut filter = filter;
        filter.sync_field_filter(
            &format.visible_fields,
            &format.available_fields,
            format.is_json(),
            &reader,
        );
        let mut ui_state = ui_state;
        ui_state.tail_following = true;
        if reader.is_actively_growing() {
            ui_state.status_toast = Some((
                "🟢 Active live stream detected! Following in real-time."
                    .to_string(),
                Instant::now(),
            ));
        }

        Ok(Self {
            completion: None,
            indexer_handle: Some(handle),
            pending_workspace_action: None,
            reader,
            filter,
            format,
            theme,
            theme_mode: crate::ui::theme::ThemeMode::DarkSlate,
            light_theme_enabled: false,
            menu,
            modal,
            ui_state,
            should_quit: false,
            last_q_press: None,
            mouse_capture_enabled: true,
            last_click: None,
        })
    }

    /// Resets the viewport after adding, removing, or modifying filters.
    /// If follow mode (tail -F) is enabled, it retains tail mode and positions
    /// the view at the tail of the filtered stream so it stays in tail!
    pub fn reset_view_after_filter_change(&mut self) {
        if self.reader.is_tail_enabled() {
            self.ui_state.tail_following = true;
            let total = self.filter.filtered_count(&self.reader);
            if total > 0 {
                if self.ui_state.reverse_order {
                    self.ui_state.selected_line = 0;
                    self.ui_state.scroll_row = 0;
                } else {
                    self.ui_state.selected_line = total - 1;
                    self.ui_state.scroll_row = total.saturating_sub(20);
                }
            } else {
                self.ui_state.selected_line = 0;
                self.ui_state.scroll_row = 0;
            }
        } else {
            self.ui_state.tail_following = false;
            self.ui_state.selected_line = 0;
            self.ui_state.scroll_row = 0;
        }
    }

    fn toggle_json_auto_indent(&mut self) {
        self.ui_state.auto_indent_json = !self.ui_state.auto_indent_json;
        let message = if self.ui_state.auto_indent_json {
            "JSON auto-indent enabled for all visible log lines"
        } else {
            "JSON auto-indent disabled"
        };
        self.ui_state.status_toast = Some((message.into(), Instant::now()));
    }

    pub(crate) fn original_line_index_for_view(&self, display_index: usize) -> Option<usize> {
        let total = self.filter.filtered_count(&self.reader);
        let source_index = source_display_index(display_index, total, self.ui_state.reverse_order);
        self.filter
            .get_original_line_index(source_index, &self.reader)
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> std::io::Result<()> {
        while !self.should_quit {
            self.tick(terminal.size()?.height);

            terminal.draw(|f| {
                FlikUi::render(
                    f.area(),
                    f.buffer_mut(),
                    &self.theme,
                    &self.reader,
                    &self.filter,
                    &self.format,
                    &self.menu,
                    &self.modal,
                    &self.ui_state,
                );
            })?;

            if event::poll(Duration::from_millis(50))? {
                let size = terminal.size()?;
                match event::read()? {
                    Event::Key(key) => self.handle_key(key, size.height as usize),
                    Event::Mouse(mouse) => {
                        self.handle_mouse(mouse, size.width as usize, size.height as usize)
                    }
                    _ => {}
                }
            }
        }

        self.reader.stop();
        Ok(())
    }

    pub(crate) fn tick(&mut self, height: u16) {
        // Check if quit warning has expired (after 1.5s)
        if let Some(t) = self.last_q_press {
            if t.elapsed() > Duration::from_millis(1500) {
                self.ui_state.quit_warning = false;
                self.last_q_press = None;
            }
        }

        // Check if status toast has expired (after 2.5s)
        if let Some((_, t)) = self.ui_state.status_toast {
            if t.elapsed() > Duration::from_millis(2500) {
                self.ui_state.status_toast = None;
            }
        }

        // Process incoming lines for active filters in real-time
        self.filter.process_incoming_lines(&self.reader);

        // Auto-follow if tail mode is active
        if self.reader.is_tail_enabled() {
            let total = self.filter.filtered_count(&self.reader);
            if total > 0 {
                let term_height = height.saturating_sub(4) as usize;
                let near_live_edge = if self.ui_state.reverse_order {
                    self.ui_state.selected_line <= 5 || self.ui_state.scroll_row <= 2
                } else {
                    self.ui_state.selected_line + 5 >= total
                        || self.ui_state.scroll_row + term_height + 2 >= total
                };
                if self.ui_state.tail_following || near_live_edge {
                    if self.ui_state.reverse_order {
                        self.ui_state.selected_line = 0;
                        self.ui_state.scroll_row = 0;
                    } else {
                        self.ui_state.selected_line = total - 1;
                        self.ui_state.scroll_row = total.saturating_sub(term_height);
                    }
                }
            }
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent, term_height: usize) {
        // If a modal is open, modal gets priority
        if self.modal != ActiveModal::None {
            self.handle_modal_key(key);
            return;
        }

        // If top menu is open, menu navigation gets priority
        if self.menu.is_open {
            self.handle_menu_key(key);
            return;
        }

        // Handle double-q (QQ) quit mechanism
        if key.code == KeyCode::Char('q') {
            let now = Instant::now();
            if let Some(prev) = self.last_q_press {
                if now.duration_since(prev) <= Duration::from_millis(1500) {
                    self.should_quit = true;
                    return;
                }
            }
            self.last_q_press = Some(now);
            self.ui_state.quit_warning = true;
            return;
        } else if key.code == KeyCode::Char('Q') || key.code == KeyCode::F(10) {
            // Capital Q or F10 immediately quits
            self.should_quit = true;
            return;
        } else {
            // Any other key resets the double-q timer
            self.last_q_press = None;
            self.ui_state.quit_warning = false;
        }

        // Global function keys (F1-F10) & Mac direct number/letter keys
        match key.code {
            KeyCode::F(1) | KeyCode::Char('?') => {
                self.modal = ActiveModal::Help { tab: 0 };
                return;
            }
            KeyCode::F(2) => {
                self.execute_menu_action(MenuAction::AutoDetectFormat);
                return;
            }
            KeyCode::F(3) => {
                self.modal = ActiveModal::AddFilter {
                    pattern: String::new(),
                    kind: FilterKind::Include,
                    is_regex: false,
                    is_case_sensitive: false,
                };
                return;
            }
            KeyCode::F(4) => {
                self.format.toggle_view_mode();
                return;
            }
            KeyCode::F(5) => {
                self.execute_menu_action(MenuAction::ToggleTail);
                return;
            }
            KeyCode::F(6) => {
                self.modal = ActiveModal::FieldSelector { selected_index: 0 };
                return;
            }
            KeyCode::F(7) => {
                self.modal = ActiveModal::Search {
                    query: self.ui_state.quick_search_term.clone(),
                };
                return;
            }
            KeyCode::F(8) => {
                self.modal = ActiveModal::AddFilter {
                    pattern: String::new(),
                    kind: FilterKind::Exclude,
                    is_regex: false,
                    is_case_sensitive: false,
                };
                return;
            }
            KeyCode::F(9) | KeyCode::Esc => {
                if self.ui_state.diff_pinned_line.is_some() {
                    self.ui_state.diff_pinned_line = None;
                    self.ui_state.status_toast =
                        Some(("Cancelled diff selection".into(), Instant::now()));
                    return;
                }
                // ESC or F9 or 'm' toggles top menu
                self.menu.toggle();
                return;
            }
            _ => {}
        }

        // Key combos with Ctrl
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('f') => {
                    self.modal = ActiveModal::FilterManager { selected_index: 0 };
                    return;
                }
                KeyCode::Char('s') => {
                    let default_path = format!("{}.extracted.log", self.reader.path.display());
                    self.modal = ActiveModal::Export {
                        output_path: default_path,
                        selected_format_idx: 0,
                        selected_scope_idx: 0,
                        focused_field: 0,
                    };
                    return;
                }
                KeyCode::Char('b') => {
                    // Ctrl+B: Page Up
                    let page_size = term_height.saturating_sub(4).max(1);
                    self.ui_state.selected_line =
                        self.ui_state.selected_line.saturating_sub(page_size);
                    self.ui_state.scroll_row = self.ui_state.scroll_row.saturating_sub(page_size);
                    return;
                }
                KeyCode::Char('c') => {
                    self.should_quit = true;
                    return;
                }
                _ => {}
            }
        }

        let total_lines = self.filter.filtered_count(&self.reader);
        let page_size = term_height.saturating_sub(4).max(1);

        // Standard navigation & Mac-friendly direct keys
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.ui_state.tail_following = false;
                if self.ui_state.selected_line > 0 {
                    self.ui_state.selected_line -= 1;
                    if self.ui_state.selected_line < self.ui_state.scroll_row {
                        self.ui_state.scroll_row = self.ui_state.selected_line;
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if total_lines > 0 && self.ui_state.selected_line + 1 < total_lines {
                    self.ui_state.selected_line += 1;
                    if self.ui_state.selected_line >= self.ui_state.scroll_row + page_size {
                        self.ui_state.scroll_row =
                            self.ui_state.selected_line.saturating_sub(page_size - 1);
                    }
                    if self.ui_state.selected_line + 1 >= total_lines {
                        self.ui_state.tail_following = true;
                    }
                }
            }
            // Page Down: Space, f, PageDown
            KeyCode::PageDown | KeyCode::Char(' ') | KeyCode::Char('f') => {
                if total_lines > 0 {
                    self.ui_state.selected_line =
                        (self.ui_state.selected_line + page_size).min(total_lines - 1);
                    self.ui_state.scroll_row = (self.ui_state.scroll_row + page_size)
                        .min(total_lines.saturating_sub(page_size));
                    if self.ui_state.selected_line + 1 >= total_lines {
                        self.ui_state.tail_following = true;
                    }
                }
            }
            // Page Up: b, PageUp
            KeyCode::PageUp | KeyCode::Char('b') => {
                self.ui_state.tail_following = false;
                self.ui_state.selected_line = self.ui_state.selected_line.saturating_sub(page_size);
                self.ui_state.scroll_row = self.ui_state.scroll_row.saturating_sub(page_size);
            }
            // Start of file: g, Home
            KeyCode::Home | KeyCode::Char('g') => {
                self.ui_state.tail_following = false;
                self.ui_state.selected_line = 0;
                self.ui_state.scroll_row = 0;
            }
            // End of file: G, End
            KeyCode::End | KeyCode::Char('G') => {
                self.ui_state.tail_following = true;
                if total_lines > 0 {
                    self.ui_state.selected_line = total_lines - 1;
                    self.ui_state.scroll_row = total_lines.saturating_sub(page_size);
                    self.reader.reset_incoming_lines();
                }
            }
            KeyCode::Left => {
                self.ui_state.scroll_col = self.ui_state.scroll_col.saturating_sub(4);
            }
            KeyCode::Right => {
                self.ui_state.scroll_col += 4;
            }
            // Direct 1-9 shortcuts
            KeyCode::Char('1') | KeyCode::Char('h') => {
                self.modal = ActiveModal::Help { tab: 0 };
            }
            KeyCode::Char('2') | KeyCode::Char('o') => {
                self.execute_menu_action(MenuAction::AutoDetectFormat);
            }
            KeyCode::Char('3') | KeyCode::Char('+') | KeyCode::Char('i') => {
                self.modal = ActiveModal::AddFilter {
                    pattern: String::new(),
                    kind: FilterKind::Include,
                    is_regex: false,
                    is_case_sensitive: false,
                };
            }
            KeyCode::Tab | KeyCode::Char('4') => {
                self.format.toggle_view_mode();
            }
            KeyCode::Char('5') | KeyCode::Char('t') => {
                self.execute_menu_action(MenuAction::ToggleTail);
            }
            KeyCode::Char('6') => {
                self.modal = ActiveModal::FieldSelector { selected_index: 0 };
            }
            KeyCode::Char('7') | KeyCode::Char('/') => {
                self.modal = ActiveModal::Search {
                    query: self.ui_state.quick_search_term.clone(),
                };
            }
            KeyCode::Char('8') | KeyCode::Char('-') | KeyCode::Char('e') => {
                self.modal = ActiveModal::AddFilter {
                    pattern: String::new(),
                    kind: FilterKind::Exclude,
                    is_regex: false,
                    is_case_sensitive: false,
                };
            }
            KeyCode::Char('9') | KeyCode::Char('m') => {
                self.menu.toggle();
            }
            KeyCode::Char('x') => {
                if let Some(popped) = self.filter.pop_last_rule(&self.reader) {
                    if self
                        .ui_state
                        .quick_search_term
                        .eq_ignore_ascii_case(&popped.pattern)
                    {
                        self.ui_state.quick_search_term.clear();
                    }
                    self.reset_view_after_filter_change();
                    let tail_msg = if self.reader.is_tail_enabled() {
                        " [Tail ON 🟢]"
                    } else {
                        ""
                    };
                    self.ui_state.status_toast = Some((
                        format!("Removed last filter: [{}]{}", popped.pattern, tail_msg),
                        Instant::now(),
                    ));
                } else if !self.ui_state.quick_search_term.is_empty() {
                    self.ui_state.quick_search_term.clear();
                    self.ui_state.status_toast =
                        Some(("Cleared search highlight".into(), Instant::now()));
                } else if self.filter.has_active_field_filter() {
                    self.format.select_all_fields();
                    self.filter.sync_field_filter(
                        &self.format.visible_fields,
                        &self.format.available_fields,
                        self.format.is_json(),
                        &self.reader,
                    );
                    self.reset_view_after_filter_change();
                    self.ui_state.status_toast =
                        Some(("Restored all fields".into(), Instant::now()));
                } else {
                    self.ui_state.status_toast =
                        Some(("No active filters to remove".into(), Instant::now()));
                }
            }
            KeyCode::Char('X') => {
                self.filter.clear_rules(&self.reader);
                self.ui_state.quick_search_term.clear();
                self.format.select_all_fields();
                self.filter.sync_field_filter(
                    &self.format.visible_fields,
                    &self.format.available_fields,
                    self.format.is_json(),
                    &self.reader,
                );
                self.reset_view_after_filter_change();
                let tail_msg = if self.reader.is_tail_enabled() {
                    " [Tail ON 🟢]"
                } else {
                    ""
                };
                self.ui_state.status_toast = Some((
                    format!("Cleared all filters & restored all fields{}", tail_msg),
                    Instant::now(),
                ));
            }
            KeyCode::Char('v') => {
                self.modal = ActiveModal::InspectFieldValues {
                    field_name: String::new(),
                    results: None,
                    selected_index: 0,
                    scroll_offset: 0,
                };
            }
            KeyCode::Char('y') => {
                self.execute_menu_action(MenuAction::CopyCurrentLine);
            }
            KeyCode::Char('M') => {
                self.execute_menu_action(MenuAction::ToggleMouseCapture);
            }
            KeyCode::Char('T') => {
                self.execute_menu_action(MenuAction::ToggleTheme);
            }
            KeyCode::Char('I') => {
                self.execute_menu_action(MenuAction::ToggleReverseOrder);
            }
            KeyCode::Char('c') => {
                self.ui_state.enable_coloration = !self.ui_state.enable_coloration;
            }
            KeyCode::Char('n') => {
                self.ui_state.show_line_numbers = !self.ui_state.show_line_numbers;
            }
            KeyCode::Char('w') => self.toggle_json_auto_indent(),
            KeyCode::Char('r') => {
                self.execute_menu_action(MenuAction::Reload);
            }
            KeyCode::Char('R') => {
                self.modal = ActiveModal::SelectFieldRange {
                    input: String::new(),
                };
            }
            KeyCode::Char('S') | KeyCode::Char('s') => {
                self.execute_menu_action(MenuAction::ShowStats);
            }
            KeyCode::Char('H') => {
                self.execute_menu_action(MenuAction::ShowHistogram);
            }
            KeyCode::Char('P') | KeyCode::Char('p') => {
                self.execute_menu_action(MenuAction::ShowPatterns);
            }
            KeyCode::Char('B') => {
                self.execute_menu_action(MenuAction::ToggleBookmark);
            }
            KeyCode::Char('\'') => {
                self.execute_menu_action(MenuAction::JumpNextBookmark);
            }
            KeyCode::Char('D') | KeyCode::Char('d') => {
                self.execute_menu_action(MenuAction::DiffLines);
            }
            KeyCode::Char('C') => {
                self.execute_menu_action(MenuAction::CorrelateTrace);
            }
            _ => {}
        }
    }

    fn handle_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::F(9) | KeyCode::Char('m') => {
                self.menu.close();
            }
            KeyCode::Left => {
                self.menu.prev_category();
            }
            KeyCode::Right => {
                self.menu.next_category();
            }
            KeyCode::Up => {
                self.menu.prev_item();
            }
            KeyCode::Down => {
                self.menu.next_item();
            }
            KeyCode::Enter => {
                if let Some(action) = self.menu.select_current() {
                    self.execute_menu_action(action);
                }
            }
            _ => {}
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) {
        let complete = matches!(self.modal, ActiveModal::Search { .. }) && key.code == KeyCode::Tab
            || key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(' ');
        if complete {
            let input = match &self.modal {
                ActiveModal::Search { query } => Some(query.clone()),
                ActiveModal::AddFilter { pattern, is_regex: false, .. } => Some(pattern.clone()),
                _ => None,
            };
            if let Some(input) = input {
                let (values, index) = if let Some((values, index)) = self.completion.take() {
                    let next = (index + 1) % values.len();
                    (values, next)
                } else { (crate::completion::candidates(&input, &self.reader, &self.format), 0) };
                if !values.is_empty() {
                    match &mut self.modal {
                        ActiveModal::Search { query } => *query = values[index].clone(),
                        ActiveModal::AddFilter { pattern, .. } => *pattern = values[index].clone(),
                        _ => {}
                    }
                    self.ui_state.completion_hint = Some(format!("{}/{} · {} · Tab / Ctrl+Space: next (sampled values)",
                        index + 1, values.len(), values.iter().take(5).cloned().collect::<Vec<_>>().join(" | ")));
                    self.completion = if values.len() == 1 && values[index].ends_with(':') {
                        None
                    } else { Some((values, index)) };
                } else {
                    self.ui_state.completion_hint = Some("No completion in the sampled log lines".into());
                }
                return;
            }
        }
        self.completion = None;
        self.ui_state.completion_hint = None;
        match &mut self.modal {
            ActiveModal::Report { lines, scroll, horizontal, .. } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.modal = ActiveModal::None,
                KeyCode::Down | KeyCode::Char('j') => *scroll = (*scroll + 1).min(lines.len().saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                KeyCode::PageDown | KeyCode::Char(' ') => *scroll = (*scroll + 15).min(lines.len().saturating_sub(1)),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(15),
                KeyCode::Left => *horizontal = horizontal.saturating_sub(8),
                KeyCode::Right => *horizontal = (*horizontal + 8).min(1000),
                KeyCode::Home => { *scroll = 0; *horizontal = 0; },
                KeyCode::End => *scroll = lines.len().saturating_sub(1),
                _ => {}
            },
            ActiveModal::AddFilter {
                pattern,
                kind,
                is_regex,
                is_case_sensitive,
            } => match key.code {
                KeyCode::Esc => {
                    self.modal = ActiveModal::None;
                }
                KeyCode::Tab => {
                    *kind = match *kind {
                        FilterKind::Include => FilterKind::Exclude,
                        FilterKind::Exclude => FilterKind::Include,
                    };
                }
                KeyCode::F(2) => {
                    *is_regex = !*is_regex;
                }
                KeyCode::F(3) => {
                    *is_case_sensitive = !*is_case_sensitive;
                }
                KeyCode::Enter => {
                    let pat = pattern.trim().to_string();
                    if !pat.is_empty() {
                        let k = *kind;
                        let r = *is_regex;
                        let c = *is_case_sensitive;
                        let pat_clone = pat.clone();
                        match self.filter.add_rule(pat, k, r, c, &self.reader) {
                            Ok(_) => {
                                self.modal = ActiveModal::None;
                                self.reset_view_after_filter_change();
                                if self.reader.is_tail_enabled() {
                                    self.ui_state.status_toast = Some((
                                        format!(
                                            "Filter added: [{}] - Following tail 🟢",
                                            pat_clone
                                        ),
                                        Instant::now(),
                                    ));
                                }
                            }
                            Err(e) => {
                                self.modal = ActiveModal::Message {
                                    title: "Filter Error".into(),
                                    body: e,
                                    is_error: true,
                                };
                            }
                        }
                    } else {
                        self.modal = ActiveModal::None;
                    }
                }
                KeyCode::Backspace => {
                    pattern.pop();
                }
                KeyCode::Char(c) => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'r' {
                        *is_regex = !*is_regex;
                    } else if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                        *is_case_sensitive = !*is_case_sensitive;
                    } else {
                        pattern.push(c);
                    }
                }
                _ => {}
            },
            ActiveModal::FilterManager { selected_index } => {
                let count = self.filter.rules.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => {
                        self.modal = ActiveModal::None;
                    }
                    KeyCode::Up => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if count > 0 && *selected_index + 1 < count {
                            *selected_index += 1;
                        }
                    }
                    KeyCode::Char(' ') => {
                        if *selected_index < count {
                            let rule_id = self.filter.rules[*selected_index].id;
                            self.filter.toggle_rule(rule_id, &self.reader);
                            self.reset_view_after_filter_change();
                        }
                    }
                    KeyCode::Char('d') | KeyCode::Delete => {
                        if *selected_index < count {
                            let rule_id = self.filter.rules[*selected_index].id;
                            self.filter.remove_rule(rule_id, &self.reader);
                            if *selected_index > 0 && *selected_index >= self.filter.rules.len() {
                                *selected_index = self.filter.rules.len().saturating_sub(1);
                            }
                            self.reset_view_after_filter_change();
                        }
                    }
                    KeyCode::Char('+') | KeyCode::Char('i') => {
                        self.modal = ActiveModal::AddFilter {
                            pattern: String::new(),
                            kind: FilterKind::Include,
                            is_regex: false,
                            is_case_sensitive: false,
                        };
                    }
                    KeyCode::Char('-') | KeyCode::Char('e') => {
                        self.modal = ActiveModal::AddFilter {
                            pattern: String::new(),
                            kind: FilterKind::Exclude,
                            is_regex: false,
                            is_case_sensitive: false,
                        };
                    }
                    KeyCode::Char('x') => {
                        self.filter.clear_rules(&self.reader);
                        *selected_index = 0;
                        self.reset_view_after_filter_change();
                    }
                    _ => {}
                }
            }
            ActiveModal::FieldSelector { selected_index } => {
                let count = self.format.available_fields.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.filter.sync_field_filter(
                            &self.format.visible_fields,
                            &self.format.available_fields,
                            self.format.is_json(),
                            &self.reader,
                        );
                        self.modal = ActiveModal::None;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if count > 0 && *selected_index + 1 < count {
                            *selected_index += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if *selected_index < count {
                            let field_name = self.format.available_fields[*selected_index].clone();
                            if !self.format.visible_fields.contains(&field_name) {
                                self.format.visible_fields.push(field_name.clone());
                            }
                            self.filter.sync_field_filter(
                                &self.format.visible_fields,
                                &self.format.available_fields,
                                self.format.is_json(),
                                &self.reader,
                            );
                            self.ui_state.status_toast = Some((
                                format!(
                                    "Field '{}' visible! (Tab: Table view, 6: Fields)",
                                    field_name
                                ),
                                Instant::now(),
                            ));
                        }
                        self.modal = ActiveModal::None;
                    }
                    KeyCode::Char(' ') => {
                        if *selected_index < count {
                            let field_name = self.format.available_fields[*selected_index].clone();
                            self.format.toggle_field_visibility(&field_name);
                            self.filter.sync_field_filter(
                                &self.format.visible_fields,
                                &self.format.available_fields,
                                self.format.is_json(),
                                &self.reader,
                            );
                        }
                    }
                    KeyCode::Char('s') => {
                        if *selected_index < count {
                            let field_name = self.format.available_fields[*selected_index].clone();
                            self.format.solo_field(&field_name);
                            self.filter.sync_field_filter(
                                &self.format.visible_fields,
                                &self.format.available_fields,
                                self.format.is_json(),
                                &self.reader,
                            );
                            self.reset_view_after_filter_change();
                            self.ui_state.status_toast = Some((
                                format!("Solo field '{}' activated! (x to reset)", field_name),
                                Instant::now(),
                            ));
                            self.modal = ActiveModal::None;
                        }
                    }
                    KeyCode::Char('f') => {
                        if *selected_index < count {
                            let field_name = self.format.available_fields[*selected_index].clone();
                            let _ = self.filter.add_rule(
                                field_name.clone(),
                                FilterKind::Include,
                                false,
                                false,
                                &self.reader,
                            );
                            self.reset_view_after_filter_change();
                            self.ui_state.status_toast = Some((
                                format!(
                                    "Filtered view to field '{}'! (x to clear filter)",
                                    field_name
                                ),
                                Instant::now(),
                            ));
                            self.modal = ActiveModal::None;
                        }
                    }
                    KeyCode::Char('r') => {
                        self.modal = ActiveModal::SelectFieldRange {
                            input: String::new(),
                        };
                    }
                    KeyCode::Char(c @ '1'..='9') => {
                        let digit = c.to_digit(10).unwrap() as usize;
                        let field_name = format!("field_{}", digit);
                        self.format.toggle_field_visibility(&field_name);
                        self.filter.sync_field_filter(
                            &self.format.visible_fields,
                            &self.format.available_fields,
                            self.format.is_json(),
                            &self.reader,
                        );
                        self.ui_state.status_toast =
                            Some((format!("Toggled {}", field_name), Instant::now()));
                    }
                    KeyCode::Char('a') => {
                        self.format.select_all_fields();
                        self.filter.sync_field_filter(
                            &self.format.visible_fields,
                            &self.format.available_fields,
                            self.format.is_json(),
                            &self.reader,
                        );
                    }
                    KeyCode::Char('c') => {
                        self.format.clear_all_fields();
                        self.filter.sync_field_filter(
                            &self.format.visible_fields,
                            &self.format.available_fields,
                            self.format.is_json(),
                            &self.reader,
                        );
                    }
                    KeyCode::Tab | KeyCode::Char('t') => {
                        self.format.toggle_view_mode();
                    }
                    _ => {}
                }
            }
            ActiveModal::SelectFieldRange { input } => match key.code {
                KeyCode::Esc => {
                    self.modal = ActiveModal::None;
                }
                KeyCode::Enter => {
                    let selected = crate::format::parse_field_selection_query(
                        input,
                        &self.format.available_fields,
                    );
                    if !selected.is_empty() {
                        self.format.visible_fields = selected;
                        self.format.view_mode = crate::format::ViewMode::ColumnsTable;
                        self.filter.sync_field_filter(
                            &self.format.visible_fields,
                            &self.format.available_fields,
                            self.format.is_json(),
                            &self.reader,
                        );
                        self.reset_view_after_filter_change();
                        self.ui_state.status_toast = Some((
                            format!(
                                "Columns active: {} (Tab: toggle Table/Compact)",
                                self.format.visible_fields.join(", ")
                            ),
                            Instant::now(),
                        ));
                    }
                    self.modal = ActiveModal::None;
                }
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => {
                    input.push(c);
                }
                _ => {}
            },
            ActiveModal::Export {
                output_path,
                selected_format_idx,
                selected_scope_idx,
                focused_field,
            } => match key.code {
                KeyCode::Esc => {
                    self.modal = ActiveModal::None;
                }
                KeyCode::Tab => {
                    *focused_field = (*focused_field + 1) % 3;
                }
                KeyCode::F(2) => {
                    *selected_scope_idx = (*selected_scope_idx + 1) % 2;
                }
                KeyCode::F(3) => {
                    *selected_format_idx = (*selected_format_idx + 1) % 3;
                }
                KeyCode::Enter => {
                    let path = output_path.trim().to_string();
                    let fmt = match *selected_format_idx {
                        1 => ExportFormat::JsonLines,
                        2 => ExportFormat::Csv,
                        _ => ExportFormat::Raw,
                    };
                    let scope = match *selected_scope_idx {
                        1 => {
                            let start = self.ui_state.scroll_row;
                            let end = start + 50;
                            ExportScope::Range(start, end)
                        }
                        _ => ExportScope::AllFiltered,
                    };

                    match ExportEngine::export_to_file(
                        &path,
                        &self.reader,
                        &self.filter,
                        &self.format,
                        scope,
                        fmt,
                    ) {
                        Ok(n) => {
                            self.modal = ActiveModal::Message {
                                title: "Export Succeeded".into(),
                                body: format!("{} lines exported successfully to:\n{}", n, path),
                                is_error: false,
                            };
                        }
                        Err(e) => {
                            self.modal = ActiveModal::Message {
                                title: "Export Error".into(),
                                body: e,
                                is_error: true,
                            };
                        }
                    }
                }
                KeyCode::Backspace => {
                    if *focused_field == 0 {
                        output_path.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if *focused_field == 0 {
                        output_path.push(c);
                    } else if *focused_field == 1 {
                        if c == ' ' || c == '2' {
                            *selected_scope_idx = (*selected_scope_idx + 1) % 2;
                        }
                    } else if *focused_field == 2 {
                        if c == ' ' || c == '3' {
                            *selected_format_idx = (*selected_format_idx + 1) % 3;
                        }
                    }
                }
                _ => {}
            },
            ActiveModal::Search { query } => match key.code {
                KeyCode::Esc => {
                    self.modal = ActiveModal::None;
                }
                KeyCode::Enter => {
                    let clean = query.trim().to_string();
                    // The first slash opened this dialog; a second slash enables context.
                    let smart_find = clean.starts_with('/');
                    let term = if smart_find { clean[1..].trim() } else { &clean };
                    self.ui_state.quick_search_term = term.to_string();
                    if !term.is_empty() {
                        let clean = if smart_find { format!("//{term}") } else { clean };

                        if let Err(error) = self.filter.add_rule(
                            clean.clone(),
                            FilterKind::Include,
                            false,
                            false,
                            &self.reader,
                        ) {
                            self.modal = ActiveModal::Message { title: "Search error".into(), body: error, is_error: true };
                            return;
                        }
                        self.reset_view_after_filter_change();
                        if self.reader.is_tail_enabled() {
                            self.ui_state.status_toast = Some((
                                format!("Search filter: [{}] - Following tail 🟢", clean),
                                Instant::now(),
                            ));
                        }
                    }
                    self.modal = ActiveModal::None;
                }
                KeyCode::Backspace => {
                    query.pop();
                }
                KeyCode::Char(c) => {
                    query.push(c);
                }
                _ => {}
            },
            ActiveModal::InspectFieldValues {
                field_name,
                results,
                selected_index,
                scroll_offset,
            } => match results {
                None => {
                    let query = field_name.trim().to_lowercase();
                    let mut filtered: Vec<String> = self
                        .format
                        .available_fields
                        .iter()
                        .filter(|f| query.is_empty() || f.to_lowercase().contains(&query))
                        .cloned()
                        .collect();
                    if !query.is_empty() && !filtered.iter().any(|f| f.eq_ignore_ascii_case(&query))
                    {
                        filtered.insert(0, field_name.trim().to_string());
                    }
                    if filtered.is_empty() {
                        filtered = vec![
                            "account_id".into(),
                            "transaction_id".into(),
                            "status".into(),
                            "level".into(),
                            "merchant".into(),
                        ];
                    }
                    let filtered_count = filtered.len();

                    match key.code {
                        KeyCode::Esc => {
                            self.modal = ActiveModal::None;
                        }
                        KeyCode::Up | KeyCode::Char('k') if field_name.is_empty() => {
                            if *selected_index > 0 {
                                *selected_index -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') if field_name.is_empty() => {
                            if filtered_count > 0 && *selected_index + 1 < filtered_count {
                                *selected_index += 1;
                            }
                        }
                        KeyCode::Up => {
                            if *selected_index > 0 {
                                *selected_index -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if filtered_count > 0 && *selected_index + 1 < filtered_count {
                                *selected_index += 1;
                            }
                        }
                        KeyCode::Backspace => {
                            field_name.pop();
                            *selected_index = 0;
                        }
                        KeyCode::Char(c) => {
                            field_name.push(c);
                            *selected_index = 0;
                        }
                        KeyCode::Enter => {
                            let f_name = if !filtered.is_empty() && *selected_index < filtered.len()
                            {
                                filtered[*selected_index].clone()
                            } else if !field_name.trim().is_empty() {
                                field_name.trim().to_string()
                            } else if !self.format.available_fields.is_empty() {
                                self.format.available_fields[0].clone()
                            } else {
                                String::new()
                            };

                            if !f_name.is_empty() {
                                let count = self.reader.line_count().min(25_000);
                                let sample = self.reader.read_range(0, count);
                                let lines: Vec<String> =
                                    sample.into_iter().map(|(_, l)| l).collect();
                                let res = self.format.inspect_field(&f_name, &lines);
                                *results = Some(res);
                                *selected_index = 0;
                                *scroll_offset = 0;
                            }
                        }
                        _ => {}
                    }
                }
                Some(res) => match key.code {
                    KeyCode::Esc => {
                        self.modal = ActiveModal::None;
                    }
                    KeyCode::Backspace | KeyCode::Char('r') => {
                        *results = None;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                            if *selected_index < *scroll_offset {
                                *scroll_offset = *selected_index;
                            }
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if !res.values.is_empty() && *selected_index + 1 < res.values.len() {
                            *selected_index += 1;
                            if *selected_index >= *scroll_offset + 12 {
                                *scroll_offset += 1;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if !res.values.is_empty() && *selected_index < res.values.len() {
                            let selected_val = res.values[*selected_index].value.clone();
                            let _ = self.filter.add_rule(
                                selected_val,
                                FilterKind::Include,
                                false,
                                false,
                                &self.reader,
                            );
                            self.reset_view_after_filter_change();
                        }
                        self.modal = ActiveModal::None;
                    }
                    _ => {}
                },
            },
            ActiveModal::StatsDashboard {
                stats,
                selected_tab,
                selected_item,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ActiveModal::None;
                }
                KeyCode::Tab | KeyCode::Right => {
                    *selected_tab = (*selected_tab + 1) % 5;
                    *selected_item = 0;
                }
                KeyCode::BackTab | KeyCode::Left => {
                    *selected_tab = (*selected_tab + 4) % 5;
                    *selected_item = 0;
                }
                KeyCode::Char('1') => {
                    *selected_tab = 0;
                    *selected_item = 0;
                }
                KeyCode::Char('2') => {
                    *selected_tab = 1;
                    *selected_item = 0;
                }
                KeyCode::Char('3') => {
                    *selected_tab = 2;
                    *selected_item = 0;
                }
                KeyCode::Char('4') => {
                    *selected_tab = 3;
                    *selected_item = 0;
                }
                KeyCode::Char('5') => {
                    *selected_tab = 4;
                    *selected_item = 0;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected_item > 0 {
                        *selected_item -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max_len = match *selected_tab {
                        0 => if stats.status_total == 0 { stats.level_counts.len() } else { stats.status_counts.len() },
                        1 => stats.top_ips.len(),
                        2 => stats.top_endpoints.len(),
                        3 => stats.top_errors.len(),
                        _ => 0,
                    };
                    if max_len > 0 && *selected_item + 1 < max_len {
                        *selected_item += 1;
                    }
                }
                KeyCode::Enter => {
                    let filter_val = match *selected_tab {
                        0 if stats.status_total == 0 => stats.level_counts.keys()
                            .nth(*selected_item).map(|level| format!("level:{level}")),
                        0 => {
                            let codes: Vec<u16> = stats.status_counts.keys().copied().collect();
                            codes.get(*selected_item).map(|c| c.to_string())
                        }
                        1 => stats.top_ips.get(*selected_item).map(|(ip, _)| ip.clone()),
                        2 => stats
                            .top_endpoints
                            .get(*selected_item)
                            .map(|(ep, _)| ep.clone()),
                        3 => stats
                            .top_errors
                            .get(*selected_item)
                            .map(|(err, _)| err.clone()),
                        _ => None,
                    };
                    if let Some(val) = filter_val {
                        let _ = self.filter.add_rule(
                            val.clone(),
                            FilterKind::Include,
                            false,
                            false,
                            &self.reader,
                        );
                        self.reset_view_after_filter_change();
                        self.ui_state.status_toast = Some((
                            format!("Filtered by: '{}' (x to clear)", val),
                            Instant::now(),
                        ));
                    }
                    self.modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::TimelineHistogram {
                histogram,
                selected_bucket,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ActiveModal::None;
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    if *selected_bucket > 0 {
                        *selected_bucket -= 1;
                    }
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if !histogram.buckets.is_empty()
                        && *selected_bucket + 1 < histogram.buckets.len()
                    {
                        *selected_bucket += 1;
                    }
                }
                KeyCode::Enter => {
                    if let Some(b) = histogram.buckets.get(*selected_bucket) {
                        if b.total_count == 0 {
                            return;
                        }
                        let target_orig = b.first_line_idx;
                        let total = self.filter.filtered_count(&self.reader);
                        let mut target_row = None;
                        for r in 0..total {
                            let source_row =
                                source_display_index(r, total, self.ui_state.reverse_order);
                            if let Some(orig) = self
                                .filter
                                .get_original_line_index(source_row, &self.reader)
                            {
                                if orig >= target_orig {
                                    target_row = Some(r);
                                    break;
                                }
                            }
                        }
                        let dest = target_row.unwrap_or(0);
                        self.ui_state.selected_line = dest;
                        self.ui_state.scroll_row = dest;
                        self.ui_state.status_toast = Some((
                            format!(
                                "Jumped to timeline bucket [{}] at line #{}",
                                b.label,
                                target_orig + 1
                            ),
                            Instant::now(),
                        ));
                    }
                    self.modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::PatternClustering {
                clusters,
                selected_index,
                scroll_offset,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ActiveModal::None;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected_index > 0 {
                        *selected_index -= 1;
                        if *selected_index < *scroll_offset {
                            *scroll_offset = *selected_index;
                        }
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !clusters.is_empty() && *selected_index + 1 < clusters.len() {
                        *selected_index += 1;
                        if *selected_index >= *scroll_offset + 10 {
                            *scroll_offset += 1;
                        }
                    }
                }
                KeyCode::Enter => {
                    if let Some(c) = clusters.get(*selected_index) {
                        let search_term = if !c.pattern_template.is_empty() {
                            let tokens: Vec<&str> = c
                                .pattern_template
                                .split(|ch: char| {
                                    ch.is_whitespace() || ch == '"' || ch == '[' || ch == ']'
                                })
                                .filter(|t| !t.is_empty() && !t.starts_with('<') && t.len() >= 3)
                                .collect();
                            if let Some(best) = tokens.iter().max_by_key(|t| t.len()) {
                                best.to_string()
                            } else {
                                c.sample_line
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or(&c.sample_line)
                                    .to_string()
                            }
                        } else {
                            c.sample_line.clone()
                        };

                        let _ = self.filter.add_rule(
                            search_term.clone(),
                            FilterKind::Include,
                            false,
                            false,
                            &self.reader,
                        );
                        self.reset_view_after_filter_change();
                        self.ui_state.status_toast = Some((
                            format!("Filtered pattern cluster: '{}' (x to clear)", search_term),
                            Instant::now(),
                        ));
                    }
                    self.modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::LineDiff { .. } => match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('d') => {
                    self.modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::TraceCorrelation {
                line_idx: _,
                identifiers,
                selected_index,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ActiveModal::None;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected_index > 0 {
                        *selected_index -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !identifiers.is_empty() && *selected_index + 1 < identifiers.len() {
                        *selected_index += 1;
                    }
                }
                KeyCode::Enter => {
                    let target_pair = identifiers.get(*selected_index).cloned();
                    if let Some((key_name, val)) = target_pair {
                        let _ = self.filter.add_rule(
                            val.clone(),
                            FilterKind::Include,
                            false,
                            false,
                            &self.reader,
                        );
                        self.reset_view_after_filter_change();
                        self.ui_state.status_toast = Some((
                            format!("Correlated view for {}: '{}' (x to clear)", key_name, val),
                            Instant::now(),
                        ));
                    }
                    self.modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::Help { tab } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.modal = ActiveModal::None;
                }
                KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                    *tab = (*tab + 1) % 5;
                }
                KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                    *tab = if *tab == 0 { 4 } else { *tab - 1 };
                }
                KeyCode::Char('1') => *tab = 0,
                KeyCode::Char('2') => *tab = 1,
                KeyCode::Char('3') => *tab = 2,
                KeyCode::Char('4') => *tab = 3,
                KeyCode::Char('5') => *tab = 4,
                KeyCode::Enter => {
                    self.modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::Message { .. } => match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.modal = ActiveModal::None;
                }
                _ => {}
            },
            ActiveModal::None => {}
        }
    }

    pub(crate) fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        term_width: usize,
        term_height: usize,
    ) {
        match mouse.kind {
            MouseEventKind::ScrollDown => {
                let total = self.filter.filtered_count(&self.reader);
                if self.ui_state.selected_line + 3 < total {
                    self.ui_state.selected_line += 3;
                    self.ui_state.scroll_row += 3;
                } else if total > 0 {
                    self.ui_state.selected_line = total - 1;
                }
                if total > 0 && self.ui_state.selected_line + 1 >= total {
                    self.ui_state.tail_following = true;
                }
            }
            MouseEventKind::ScrollUp => {
                self.ui_state.tail_following = false;
                self.ui_state.selected_line = self.ui_state.selected_line.saturating_sub(3);
                self.ui_state.scroll_row = self.ui_state.scroll_row.saturating_sub(3);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Top menu row click
                if mouse.row == 0 {
                    self.menu.toggle();
                    return;
                }

                // Bottom function keys row click (Trackpad / Mouse accessible!)
                if (mouse.row as usize) >= term_height.saturating_sub(1) {
                    let col = mouse.column as usize;
                    let btn_width = (term_width / 10).max(1);
                    let btn_idx = (col / btn_width).min(9);

                    match btn_idx {
                        0 => self.modal = ActiveModal::Help { tab: 0 },
                        1 => self.execute_menu_action(MenuAction::AutoDetectFormat),
                        2 => {
                            self.modal = ActiveModal::AddFilter {
                                pattern: String::new(),
                                kind: FilterKind::Include,
                                is_regex: false,
                                is_case_sensitive: false,
                            }
                        }
                        3 => {
                            self.format.toggle_view_mode();
                        }
                        4 => {
                            self.execute_menu_action(MenuAction::ToggleTail);
                        }
                        5 => {
                            self.modal = ActiveModal::FieldSelector { selected_index: 0 };
                        }
                        6 => {
                            self.modal = ActiveModal::Search {
                                query: self.ui_state.quick_search_term.clone(),
                            };
                        }
                        7 => {
                            self.modal = ActiveModal::AddFilter {
                                pattern: String::new(),
                                kind: FilterKind::Exclude,
                                is_regex: false,
                                is_case_sensitive: false,
                            }
                        }
                        8 => self.menu.toggle(),
                        9 => self.should_quit = true,
                        _ => {}
                    }
                    return;
                }

                // Content area click (viewing mode): select line or double-click to copy word
                if self.modal == ActiveModal::None {
                    let total = self.filter.filtered_count(&self.reader);
                    if total > 0 {
                        let Some(line_offset) = clicked_line_offset(mouse.row) else {
                            self.last_click = None;
                            return;
                        };
                        let clicked_line_idx = self.ui_state.scroll_row + line_offset;
                        if clicked_line_idx < total {
                            let now = Instant::now();
                            let is_double_click =
                                if let Some((last_time, last_col, last_row)) = self.last_click {
                                    now.duration_since(last_time).as_millis() < 450
                                        && last_row == mouse.row
                                        && (last_col as i32 - mouse.column as i32).abs() <= 3
                                } else {
                                    false
                                };

                            if is_double_click {
                                if let Some(orig_idx) =
                                    self.original_line_index_for_view(clicked_line_idx)
                                {
                                    if let Some(raw_line) = self.reader.read_line(orig_idx) {
                                        let max_line_num_digits = total.to_string().len().max(4);
                                        let gutter_width = if self.ui_state.show_line_numbers {
                                            max_line_num_digits + 5
                                        } else {
                                            0
                                        };
                                        let char_offset = clicked_text_offset(
                                            mouse.column,
                                            gutter_width,
                                            self.ui_state.scroll_col,
                                        );
                                        let displayed_line = mouse_text_for_line(
                                            &raw_line,
                                            &self.format,
                                            self.ui_state.auto_indent_json,
                                        );
                                        let char_offset = byte_offset_for_display_column(
                                            &displayed_line,
                                            char_offset,
                                        );

                                        if let Some(word) =
                                            extract_word_at_pos(&displayed_line, char_offset)
                                        {
                                            copy_text_to_clipboard(&word);
                                            self.ui_state.quick_search_term = word.clone();
                                            self.ui_state.status_toast = Some((
                                                format!("[ Copied '{}' to clipboard! ]", word),
                                                Instant::now(),
                                            ));
                                        }
                                    }
                                }
                                self.ui_state.selected_line = clicked_line_idx;
                                if clicked_line_idx + 1 >= total {
                                    self.ui_state.tail_following = true;
                                } else {
                                    self.ui_state.tail_following = false;
                                }
                                self.last_click = None;
                            } else {
                                self.ui_state.selected_line = clicked_line_idx;
                                if clicked_line_idx + 1 >= total {
                                    self.ui_state.tail_following = true;
                                } else {
                                    self.ui_state.tail_following = false;
                                }
                                self.last_click = Some((now, mouse.column, mouse.row));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn execute_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::Investigate
            | MenuAction::ShowContext
            | MenuAction::TimeRange
            | MenuAction::MergeFiles
            | MenuAction::TraceFiles
            | MenuAction::SaveSession
            | MenuAction::LoadSession
            | MenuAction::ExportIncident
            | MenuAction::OpenFile
            | MenuAction::AddFile
            | MenuAction::ClosePane
            | MenuAction::SwitchPane => {
                self.pending_workspace_action = Some(action);
            }
            MenuAction::ToggleTail => {
                let enabled = self.reader.toggle_tail();
                self.ui_state.tail_following = enabled;
                if enabled {
                    let total = self.filter.filtered_count(&self.reader);
                    if total > 0 {
                        if self.ui_state.reverse_order {
                            self.ui_state.selected_line = 0;
                            self.ui_state.scroll_row = 0;
                        } else {
                            self.ui_state.selected_line = total - 1;
                            self.ui_state.scroll_row = total.saturating_sub(20);
                        }
                    }
                    self.reader.reset_incoming_lines();
                    self.ui_state.status_toast = Some((
                        "🟢 LIVE Follow Mode ON (tail -F) - Auto-scrolling".to_string(),
                        Instant::now(),
                    ));
                } else {
                    self.ui_state.status_toast =
                        Some(("Follow Mode OFF (Static View)".to_string(), Instant::now()));
                }
            }
            MenuAction::Reload => {
                if let Ok((new_reader, handle)) = LogReader::open(&self.reader.path) {
                    self.reader.stop();
                    if let Some(old) = self.indexer_handle.take() {
                        let _ = old.join();
                    }
                    self.indexer_handle = Some(handle);
                    self.reader = new_reader;
                    self.filter.trigger_refilter(&self.reader);
                }
            }
            MenuAction::Quit => {
                self.should_quit = true;
            }
            MenuAction::AddIncludeFilter => {
                self.modal = ActiveModal::AddFilter {
                    pattern: String::new(),
                    kind: FilterKind::Include,
                    is_regex: false,
                    is_case_sensitive: false,
                };
            }
            MenuAction::AddExcludeFilter => {
                self.modal = ActiveModal::AddFilter {
                    pattern: String::new(),
                    kind: FilterKind::Exclude,
                    is_regex: false,
                    is_case_sensitive: false,
                };
            }
            MenuAction::ManageFilters => {
                self.modal = ActiveModal::FilterManager { selected_index: 0 };
            }
            MenuAction::ClearFilters => {
                self.filter.clear_rules(&self.reader);
                self.reset_view_after_filter_change();
            }
            MenuAction::AutoDetectFormat => {
                let sample = self.reader.read_range(0, 50);
                let sample_lines: Vec<String> = sample.into_iter().map(|(_, l)| l).collect();
                self.format.selected_format = LogFormat::Auto;
                self.format.detect_and_index_fields(&sample_lines);
                self.modal = ActiveModal::Message {
                    title: "Format Detected".into(),
                    body: format!(
                        "Detected format: {}\n{} fields discovered.",
                        self.format.detected_format.name(),
                        self.format.available_fields.len()
                    ),
                    is_error: false,
                };
            }
            MenuAction::ToggleViewMode => {
                self.format.toggle_view_mode();
            }
            MenuAction::SelectFields => {
                self.modal = ActiveModal::FieldSelector { selected_index: 0 };
            }
            MenuAction::SelectFieldRange => {
                self.modal = ActiveModal::SelectFieldRange {
                    input: String::new(),
                };
            }
            MenuAction::ForceJson => {
                self.format.selected_format = LogFormat::Json;
                let sample_lines: Vec<String> = self
                    .reader
                    .read_range(0, 100)
                    .into_iter()
                    .map(|(_, l)| l)
                    .collect();
                self.format.detect_and_index_fields(&sample_lines);
                self.filter.sync_field_filter(
                    &self.format.visible_fields,
                    &self.format.available_fields,
                    self.format.is_json(),
                    &self.reader,
                );
                self.reset_view_after_filter_change();
                self.ui_state.status_toast = Some(("Forced format: JSON".into(), Instant::now()));
            }
            MenuAction::ForceApache => {
                self.format.selected_format = LogFormat::ApacheCombined;
                let sample_lines: Vec<String> = self
                    .reader
                    .read_range(0, 100)
                    .into_iter()
                    .map(|(_, l)| l)
                    .collect();
                self.format.detect_and_index_fields(&sample_lines);
                self.filter.sync_field_filter(
                    &self.format.visible_fields,
                    &self.format.available_fields,
                    self.format.is_json(),
                    &self.reader,
                );
                self.reset_view_after_filter_change();
                self.ui_state.status_toast = Some((
                    "Forced format: Apache Combined/Common".into(),
                    Instant::now(),
                ));
            }
            MenuAction::ForceApacheError => {
                self.format.selected_format = LogFormat::ApacheError;
                let sample_lines: Vec<String> = self
                    .reader
                    .read_range(0, 100)
                    .into_iter()
                    .map(|(_, l)| l)
                    .collect();
                self.format.detect_and_index_fields(&sample_lines);
                self.filter.sync_field_filter(
                    &self.format.visible_fields,
                    &self.format.available_fields,
                    self.format.is_json(),
                    &self.reader,
                );
                self.reset_view_after_filter_change();
                self.ui_state.status_toast =
                    Some(("Forced format: Apache Error Log".into(), Instant::now()));
            }
            MenuAction::ForceNginxError => {
                self.format.selected_format = LogFormat::NginxError;
                let sample_lines: Vec<String> = self
                    .reader
                    .read_range(0, 100)
                    .into_iter()
                    .map(|(_, l)| l)
                    .collect();
                self.format.detect_and_index_fields(&sample_lines);
                self.filter.sync_field_filter(
                    &self.format.visible_fields,
                    &self.format.available_fields,
                    self.format.is_json(),
                    &self.reader,
                );
                self.reset_view_after_filter_change();
                self.ui_state.status_toast =
                    Some(("Forced format: Nginx Error Log".into(), Instant::now()));
            }
            MenuAction::ForceK8s => {
                self.format.selected_format = LogFormat::Kubernetes;
            }
            MenuAction::ForceSyslog => {
                self.format.selected_format = LogFormat::Syslog;
            }
            MenuAction::ForceRaw => {
                self.format.selected_format = LogFormat::Raw;
            }
            MenuAction::ToggleLineNumbers => {
                self.ui_state.show_line_numbers = !self.ui_state.show_line_numbers;
            }
            MenuAction::ToggleColoration => {
                self.ui_state.enable_coloration = !self.ui_state.enable_coloration;
            }
            MenuAction::ToggleTheme => {
                self.theme_mode = self.theme_mode.next();
                self.theme = self.theme_mode.to_theme();
                self.light_theme_enabled = self.theme_mode != crate::ui::theme::ThemeMode::DarkSlate;
                self.ui_state.status_toast =
                    Some((format!("{} theme enabled", self.theme_mode.name()), Instant::now()));
            }
            MenuAction::ToggleReverseOrder => {
                self.ui_state.reverse_order = !self.ui_state.reverse_order;
                self.ui_state.selected_line = 0;
                self.ui_state.scroll_row = 0;
                self.ui_state.tail_following = self.reader.is_tail_enabled();
                let message = if self.ui_state.reverse_order {
                    "Newest log lines are now shown first"
                } else {
                    "Oldest log lines are now shown first"
                };
                self.ui_state.status_toast = Some((message.into(), Instant::now()));
            }
            MenuAction::ToggleAutoIndentJson => self.toggle_json_auto_indent(),
            MenuAction::JumpToTop => {
                self.ui_state.tail_following = false;
                self.ui_state.selected_line = 0;
                self.ui_state.scroll_row = 0;
            }
            MenuAction::JumpToBottom => {
                self.ui_state.tail_following = true;
                let total = self.filter.filtered_count(&self.reader);
                if total > 0 {
                    self.ui_state.selected_line = total - 1;
                    self.ui_state.scroll_row = total.saturating_sub(20);
                }
            }
            MenuAction::Search => {
                self.modal = ActiveModal::Search {
                    query: self.ui_state.quick_search_term.clone(),
                };
            }
            MenuAction::ExportFiltered => {
                let default_path = format!("{}.extracted.log", self.reader.path.display());
                self.modal = ActiveModal::Export {
                    output_path: default_path,
                    selected_format_idx: 0,
                    selected_scope_idx: 0,
                    focused_field: 0,
                };
            }
            MenuAction::ShowHelp => {
                self.modal = ActiveModal::Help { tab: 0 };
            }
            MenuAction::InspectFieldValues => {
                self.modal = ActiveModal::InspectFieldValues {
                    field_name: String::new(),
                    results: None,
                    selected_index: 0,
                    scroll_offset: 0,
                };
            }
            MenuAction::CopyCurrentLine => {
                let total = self.filter.filtered_count(&self.reader);
                if total > 0 {
                    if let Some(actual_idx) =
                        self.original_line_index_for_view(self.ui_state.selected_line)
                    {
                        let line = self.reader.read_line(actual_idx).unwrap_or_default();
                        copy_text_to_clipboard(&line);
                        self.ui_state.status_toast = Some((
                            "Copied line to clipboard (pbcopy / OSC 52)!".to_string(),
                            Instant::now(),
                        ));
                    }
                }
            }
            MenuAction::ToggleMouseCapture => {
                self.mouse_capture_enabled = !self.mouse_capture_enabled;
                if self.mouse_capture_enabled {
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::event::EnableMouseCapture
                    );
                    self.ui_state.status_toast = Some((
                        "Mouse Capture: ON (TUI scrolling enabled)".to_string(),
                        Instant::now(),
                    ));
                } else {
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::event::DisableMouseCapture
                    );
                    self.ui_state.status_toast = Some((
                        "Mouse Capture: OFF (Native Mac text selection enabled!)".to_string(),
                        Instant::now(),
                    ));
                }
            }
            MenuAction::ShowStats => {
                let stats = crate::analytics::StatsSummary::compute(&self.reader, 5000);
                self.modal = ActiveModal::StatsDashboard {
                    stats,
                    selected_tab: 0,
                    selected_item: 0,
                };
            }
            MenuAction::ShowHistogram => {
                self.pending_workspace_action = Some(MenuAction::ShowHistogram);
            }
            MenuAction::ShowPatterns => {
                let clusters = crate::analytics::PatternClusterer::cluster(&self.reader, 3000);
                self.modal = ActiveModal::PatternClustering {
                    clusters,
                    selected_index: 0,
                    scroll_offset: 0,
                };
            }
            MenuAction::ToggleBookmark => {
                let total = self.filter.filtered_count(&self.reader);
                if total > 0 {
                    if let Some(orig_idx) =
                        self.original_line_index_for_view(self.ui_state.selected_line)
                    {
                        if self.ui_state.bookmarks.contains(&orig_idx) {
                            self.ui_state.bookmarks.remove(&orig_idx);
                            self.ui_state.status_toast = Some((
                                format!("Bookmark removed from line #{}", orig_idx + 1),
                                Instant::now(),
                            ));
                        } else {
                            self.ui_state.bookmarks.insert(orig_idx);
                            self.ui_state.status_toast = Some((
                                format!("★ Bookmarked line #{} (' to jump next)", orig_idx + 1),
                                Instant::now(),
                            ));
                        }
                    }
                }
            }
            MenuAction::JumpNextBookmark => {
                if self.ui_state.bookmarks.is_empty() {
                    self.ui_state.status_toast = Some((
                        "No bookmarks set. Press 'B' to bookmark lines.".to_string(),
                        Instant::now(),
                    ));
                } else {
                    let cur_orig = self
                        .original_line_index_for_view(self.ui_state.selected_line)
                        .unwrap_or(0);
                    let next_orig = self
                        .ui_state
                        .bookmarks
                        .range((cur_orig + 1)..)
                        .next()
                        .or_else(|| self.ui_state.bookmarks.iter().next())
                        .copied();
                    if let Some(target) = next_orig {
                        let total = self.filter.filtered_count(&self.reader);
                        let mut found_row = None;
                        for r in 0..total {
                            if self.original_line_index_for_view(r) == Some(target) {
                                found_row = Some(r);
                                break;
                            }
                        }
                        if let Some(r) = found_row {
                            self.ui_state.selected_line = r;
                            self.ui_state.scroll_row = r.saturating_sub(5);
                            self.ui_state.status_toast = Some((
                                format!("Jumped to bookmark ★ on line #{}", target + 1),
                                Instant::now(),
                            ));
                        } else {
                            self.ui_state.status_toast = Some((
                                format!(
                                    "Bookmark ★ on line #{} is hidden by active filters!",
                                    target + 1
                                ),
                                Instant::now(),
                            ));
                        }
                    }
                }
            }
            MenuAction::DiffLines => {
                let total = self.filter.filtered_count(&self.reader);
                if total > 0 {
                    if let Some(cur_orig) =
                        self.original_line_index_for_view(self.ui_state.selected_line)
                    {
                        match self.ui_state.diff_pinned_line {
                            None => {
                                self.ui_state.diff_pinned_line = Some(cur_orig);
                                self.ui_state.status_toast = Some((
                                    format!(
                                        "📌 Line #{} pinned for diff! Select another line and press 'D' again.",
                                        cur_orig + 1
                                    ),
                                    Instant::now(),
                                ));
                            }
                            Some(pinned_idx) => {
                                if pinned_idx == cur_orig {
                                    self.ui_state.status_toast = Some((
                                        format!(
                                            "Current line is already pinned (#{}). Select a different line or press Esc.",
                                            pinned_idx + 1
                                        ),
                                        Instant::now(),
                                    ));
                                } else {
                                    let line1 =
                                        self.reader.read_line(pinned_idx).unwrap_or_default();
                                    let line2 = self.reader.read_line(cur_orig).unwrap_or_default();
                                    self.ui_state.diff_pinned_line = None;
                                    self.modal = ActiveModal::LineDiff {
                                        line1_idx: pinned_idx,
                                        line1_text: line1,
                                        line2_idx: cur_orig,
                                        line2_text: line2,
                                    };
                                }
                            }
                        }
                    }
                }
            }
            MenuAction::CorrelateTrace => {
                let total = self.filter.filtered_count(&self.reader);
                if total > 0 {
                    if let Some(cur_orig) =
                        self.original_line_index_for_view(self.ui_state.selected_line)
                    {
                        let line = self.reader.read_line(cur_orig).unwrap_or_default();
                        let ids = crate::analytics::TraceCorrelator::extract_identifiers(&line);
                        if ids.is_empty() {
                            self.ui_state.status_toast = Some((
                                format!(
                                    "No trace IDs or identifiers detected on line #{}",
                                    cur_orig + 1
                                ),
                                Instant::now(),
                            ));
                        } else {
                            self.modal = ActiveModal::TraceCorrelation {
                                line_idx: cur_orig,
                                identifiers: ids,
                                selected_index: 0,
                            };
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        byte_offset_for_display_column, clicked_line_offset, clicked_text_offset,
        extract_word_at_pos, mouse_text_for_line,
    };
    use crate::format::{FormatManager, LogFormat};

    #[test]
    fn double_slash_search_creates_a_context_filter() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "cause\nerror\n").unwrap();
        let mut app = super::App::new(file.path().to_path_buf()).unwrap();
        for code in [KeyCode::Char('/'), KeyCode::Char('/'), KeyCode::Char('e'),
            KeyCode::Char('r'), KeyCode::Char('r'), KeyCode::Char('o'),
            KeyCode::Char('r'), KeyCode::Enter] {
            app.handle_key(KeyEvent::new(code, KeyModifiers::NONE), 30);
        }
        assert_eq!(app.filter.rules[0].pattern, "//error");
        assert_eq!(app.ui_state.quick_search_term, "error");
    }

    #[test]
    fn opening_a_file_follows_the_end_after_indexing() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let lines: String = (0..5000).map(|i| format!("INFO event {i}\n")).collect();
        std::fs::write(file.path(), lines).unwrap();
        let mut app = super::App::new(file.path().to_path_buf()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.reader.line_count() < 5000 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        app.tick(30);
        assert!(app.reader.is_tail_enabled());
        assert!(app.ui_state.tail_following);
        assert_eq!(app.ui_state.selected_line, 4999);
        assert!(app.ui_state.scroll_row > 0);
    }

    #[test]
    fn mouse_click_maps_first_log_row_after_menu_and_border() {
        assert_eq!(clicked_line_offset(1), None);
        assert_eq!(clicked_line_offset(2), Some(0));
        assert_eq!(clicked_line_offset(3), Some(1));
    }

    #[test]
    fn mouse_word_offset_accounts_for_border_gutter_and_horizontal_scroll() {
        assert_eq!(clicked_text_offset(10, 9, 0), 0);
        assert_eq!(clicked_text_offset(14, 9, 2), 6);

        let line = "2026-10-20T10:00:00Z stdout F Started processing job";
        let word_start = line.find("processing").unwrap();
        let click_column = (9 + 1 + word_start) as u16;
        let source_offset = clicked_text_offset(click_column, 9, 0);
        assert_eq!(
            extract_word_at_pos(line, source_offset).as_deref(),
            Some("processing")
        );
    }

    #[test]
    fn double_click_uses_the_rendered_kubernetes_text() {
        let line =
            r#"2026-10-20T10:00:00Z stdout F {"service":"auth-service","msg":"JWT token expired"}"#;
        let mut format = FormatManager::new();
        format.detect_and_index_fields(&[line.to_string()]);
        assert_eq!(format.active_format(), LogFormat::Kubernetes);

        let displayed = mouse_text_for_line(line, &format, false);
        assert!(displayed.contains("[STDOUT] F"));
        let word_column = displayed.find("JWT").unwrap();
        let terminal_column = (1 + 9 + 1 + word_column) as u16;
        let visible_column = clicked_text_offset(terminal_column, 9, 0);
        let byte_offset = byte_offset_for_display_column(&displayed, visible_column);
        assert_eq!(
            extract_word_at_pos(&displayed, byte_offset).as_deref(),
            Some("JWT")
        );
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.filter
            .cancel_signal
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.reader.stop();
        if let Some(handle) = self.indexer_handle.take() {
            let _ = handle.join();
        }
    }
}
