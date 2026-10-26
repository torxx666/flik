use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Widget},
};

use crate::ui::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Investigate,
    ShowContext,
    TimeRange,
    MergeFiles,
    TraceFiles,
    SaveSession,
    LoadSession,
    ExportIncident,
    // File
    OpenFile,
    AddFile,
    ClosePane,
    SwitchPane,
    ToggleTail,
    Reload,
    Quit,

    // Filter
    AddIncludeFilter,
    AddExcludeFilter,
    ManageFilters,
    ClearFilters,

    // Format
    AutoDetectFormat,
    ToggleViewMode,
    SelectFields,
    SelectFieldRange,
    InspectFieldValues,
    ForceJson,
    ForceApache,
    ForceApacheError,
    ForceNginxError,
    ForceK8s,
    ForceSyslog,
    ForceRaw,

    // View
    ToggleLineNumbers,
    ToggleColoration,
    ToggleTheme,
    ToggleReverseOrder,
    ToggleAutoIndentJson,
    ToggleMouseCapture,
    CopyCurrentLine,
    JumpToTop,
    JumpToBottom,
    Search,

    // Analytics & Tools (Killer Features)
    ShowStats,
    ShowHistogram,
    ShowPatterns,
    ToggleBookmark,
    JumpNextBookmark,
    DiffLines,
    CorrelateTrace,

    // Export
    ExportFiltered,

    // Help
    ShowHelp,
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub action: MenuAction,
    pub description: &'static str,
    pub is_separator: bool,
}

impl MenuItem {
    pub const fn item(
        label: &'static str,
        shortcut: &'static str,
        action: MenuAction,
        description: &'static str,
    ) -> Self {
        Self {
            label,
            shortcut,
            action,
            description,
            is_separator: false,
        }
    }

    pub const fn sep() -> Self {
        Self {
            label: "",
            shortcut: "",
            action: MenuAction::Quit,
            description: "",
            is_separator: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MenuCategory {
    pub name: &'static str,
    pub hotkey: char,
    pub items: Vec<MenuItem>,
}

pub struct MenuBar {
    pub is_open: bool,
    pub selected_category: usize,
    pub selected_item: usize,
    pub categories: Vec<MenuCategory>,
}

impl MenuBar {
    pub fn new() -> Self {
        let categories = vec![
            MenuCategory {
                name: "Investigate",
                hotkey: 'I',
                items: vec![
                    MenuItem::item(
                        "Commands / history",
                        "Ctrl+K",
                        MenuAction::Investigate,
                        "Open command palette, query history and actions",
                    ),
                    MenuItem::item(
                        "Context around selected match",
                        "Ctrl+G",
                        MenuAction::ShowContext,
                        "Show surrounding lines before/after selected match in raw source",
                    ),
                    MenuItem::item(
                        "Time range / jump / around",
                        "Ctrl+T",
                        MenuAction::TimeRange,
                        "Jump to timestamp or filter incidents by time range",
                    ),
                    MenuItem::sep(),
                    MenuItem::item(
                        "Merged chronological results",
                        "Ctrl+L",
                        MenuAction::MergeFiles,
                        "Merge multiple log files into single ordered timeline",
                    ),
                    MenuItem::item(
                        "Trace across files",
                        "Ctrl+R",
                        MenuAction::TraceFiles,
                        "Correlate Request ID / Trace ID across all open panes",
                    ),
                    MenuItem::sep(),
                    MenuItem::item(
                        "Save session / view",
                        "Ctrl+P",
                        MenuAction::SaveSession,
                        "Save current workspace session, views and filters to disk",
                    ),
                    MenuItem::item(
                        "Restore session / view",
                        "Ctrl+K load",
                        MenuAction::LoadSession,
                        "Restore a previously saved investigation session",
                    ),
                    MenuItem::item(
                        "Export redacted incident",
                        "Ctrl+E",
                        MenuAction::ExportIncident,
                        "Export incident logs with automatic secret and PII redaction",
                    ),
                ],
            },
            MenuCategory {
                name: "File",
                hotkey: 'F',
                items: vec![
                    MenuItem::item(
                        "Open in Active Pane",
                        "Ctrl+O",
                        MenuAction::OpenFile,
                        "Open a log file or live stream in active pane",
                    ),
                    MenuItem::item(
                        "Add / Replace Other Pane",
                        "Ctrl+N",
                        MenuAction::AddFile,
                        "Open second file side-by-side in split pane view",
                    ),
                    MenuItem::item(
                        "Switch Pane",
                        "Ctrl+] / F12",
                        MenuAction::SwitchPane,
                        "Switch focus between split panes",
                    ),
                    MenuItem::item(
                        "Close Active Pane",
                        "Ctrl+W",
                        MenuAction::ClosePane,
                        "Close active pane and expand remaining pane",
                    ),
                    MenuItem::sep(),
                    MenuItem::item(
                        "Follow (tail -F)",
                        "5 / t",
                        MenuAction::ToggleTail,
                        "Toggle real-time log following with live append tracking",
                    ),
                    MenuItem::item(
                        "Reload File",
                        "r",
                        MenuAction::Reload,
                        "Re-index and reload file contents from disk",
                    ),
                    MenuItem::sep(),
                    MenuItem::item(
                        "Save / Export Extraction",
                        "Ctrl+S",
                        MenuAction::ExportFiltered,
                        "Export filtered or visible lines to Raw, JSON, or CSV",
                    ),
                    MenuItem::item(
                        "Quit Flik",
                        "QQ / F10",
                        MenuAction::Quit,
                        "Exit Flik (press 'q' twice or Shift+Q / F10)",
                    ),
                ],
            },
            MenuCategory {
                name: "Filter",
                hotkey: 'L',
                items: vec![
                    MenuItem::item(
                        "Add Include Filter (+)",
                        "3 / +",
                        MenuAction::AddIncludeFilter,
                        "Show only lines matching text, regex, or expression",
                    ),
                    MenuItem::item(
                        "Add Exclude Filter (-)",
                        "8 / -",
                        MenuAction::AddExcludeFilter,
                        "Hide lines matching pattern or expression",
                    ),
                    MenuItem::sep(),
                    MenuItem::item(
                        "Manage Filter Stack",
                        "Ctrl+F",
                        MenuAction::ManageFilters,
                        "View, toggle, edit or remove active filter rules",
                    ),
                    MenuItem::item(
                        "Clear All Filters",
                        "x",
                        MenuAction::ClearFilters,
                        "Remove all active filters and restore full log view",
                    ),
                ],
            },
            MenuCategory {
                name: "Format",
                hotkey: 'O',
                items: vec![
                    MenuItem::item(
                        "Auto-Detect Format",
                        "2 / o",
                        MenuAction::AutoDetectFormat,
                        "Sample log lines and auto-configure column structure",
                    ),
                    MenuItem::item(
                        "Switch View (Line / Table)",
                        "4 / Tab",
                        MenuAction::ToggleViewMode,
                        "Toggle between compact line and structured column table",
                    ),
                    MenuItem::item(
                        "Select Columns / Fields",
                        "6 / f",
                        MenuAction::SelectFields,
                        "Choose which structured fields to display as table columns",
                    ),
                    MenuItem::item(
                        "Quick Column Range (e.g. 1-4)",
                        "R",
                        MenuAction::SelectFieldRange,
                        "Quickly filter column range by index (e.g. 1-4 or 0,2,5)",
                    ),
                    MenuItem::item(
                        "Inspect Field Values (Facets)",
                        "v",
                        MenuAction::InspectFieldValues,
                        "View unique value distribution and counts for active field",
                    ),
                    MenuItem::sep(),
                    MenuItem::item(
                        "Force JSON Format",
                        "",
                        MenuAction::ForceJson,
                        "Parse lines as JSON objects with key extraction",
                    ),
                    MenuItem::item(
                        "Force Apache / Nginx Combined",
                        "",
                        MenuAction::ForceApache,
                        "Parse Nginx / Apache standard combined access logs",
                    ),
                    MenuItem::item(
                        "Force Apache Error Log",
                        "",
                        MenuAction::ForceApacheError,
                        "Parse Apache error logs with log levels",
                    ),
                    MenuItem::item(
                        "Force Nginx Error Log",
                        "",
                        MenuAction::ForceNginxError,
                        "Parse Nginx error logs with levels, pid#tid, and client info",
                    ),
                    MenuItem::item(
                        "Force Kubernetes Format",
                        "",
                        MenuAction::ForceK8s,
                        "Parse container logs with timestamp and stream tags",
                    ),
                    MenuItem::item(
                        "Force Syslog Format",
                        "",
                        MenuAction::ForceSyslog,
                        "Parse RFC syslog headers and facility messages",
                    ),
                    MenuItem::item(
                        "Force Raw Text",
                        "",
                        MenuAction::ForceRaw,
                        "Display plain unparsed raw text lines",
                    ),
                ],
            },
            MenuCategory {
                name: "View",
                hotkey: 'V',
                items: vec![
                    MenuItem::item(
                        "Search In View",
                        "7 / /",
                        MenuAction::Search,
                        "Highlight and jump between matches in current view",
                    ),
                    MenuItem::item(
                        "Copy Line to Clipboard",
                        "y",
                        MenuAction::CopyCurrentLine,
                        "Yank current line to system clipboard (OSC 52 / pbcopy)",
                    ),
                    MenuItem::item(
                        "Toggle Mouse Interaction",
                        "M",
                        MenuAction::ToggleMouseCapture,
                        "Toggle mouse clicks, row selection, and scrolling",
                    ),
                    MenuItem::sep(),
                    MenuItem::item(
                        "Toggle Line Numbers",
                        "n",
                        MenuAction::ToggleLineNumbers,
                        "Show or hide original log file line numbers",
                    ),
                    MenuItem::item(
                        "Toggle Coloration",
                        "c",
                        MenuAction::ToggleColoration,
                        "Toggle ANSI syntax highlighting and level badges",
                    ),
                    MenuItem::item(
                        "Switch Graphic Theme",
                        "T",
                        MenuAction::ToggleTheme,
                        "Toggle between Dark Slate and Warm Sepia themes",
                    ),
                    MenuItem::item(
                        "Newest Logs First",
                        "I",
                        MenuAction::ToggleReverseOrder,
                        "Show latest incoming logs at top of screen",
                    ),
                    MenuItem::item(
                        "Toggle JSON Auto-Indent",
                        "w",
                        MenuAction::ToggleAutoIndentJson,
                        "Format and indent embedded JSON payload strings",
                    ),
                    MenuItem::sep(),
                    MenuItem::item(
                        "Top of File",
                        "g / Home",
                        MenuAction::JumpToTop,
                        "Scroll to the beginning of the log file",
                    ),
                    MenuItem::item(
                        "End of File",
                        "G / End",
                        MenuAction::JumpToBottom,
                        "Scroll to the latest line and follow",
                    ),
                    MenuItem::item(
                        "Toggle Bookmark",
                        "B / Space",
                        MenuAction::ToggleBookmark,
                        "Pin or unpin a bookmark flag (*) on current line",
                    ),
                    MenuItem::item(
                        "Jump to Next Bookmark",
                        "'",
                        MenuAction::JumpNextBookmark,
                        "Jump cursor to next bookmarked line (*)",
                    ),
                ],
            },
            MenuCategory {
                name: "Analytics",
                hotkey: 'A',
                items: vec![
                    MenuItem::item(
                        "Statistics & Top 10 Dashboard",
                        "S",
                        MenuAction::ShowStats,
                        "Metrics breakdown: HTTP codes, levels, IPs, latencies",
                    ),
                    MenuItem::item(
                        "Time Histogram & Error Spikes",
                        "H",
                        MenuAction::ShowHistogram,
                        "Hourly distribution chart to pinpoint incident spikes",
                    ),
                    MenuItem::item(
                        "Log Pattern Clustering (Anomalies)",
                        "P",
                        MenuAction::ShowPatterns,
                        "Group logs by template to uncover rare anomalies (< 2%)",
                    ),
                    MenuItem::sep(),
                    MenuItem::item(
                        "Compare 2 Lines (Diff)",
                        "D",
                        MenuAction::DiffLines,
                        "Character-by-character visual diff between two lines",
                    ),
                    MenuItem::item(
                        "Correlate Trace / Request ID",
                        "C / Enter",
                        MenuAction::CorrelateTrace,
                        "Extract identifiers and filter related log events",
                    ),
                ],
            },
            MenuCategory {
                name: "Help",
                hotkey: 'H',
                items: vec![MenuItem::item(
                    "Shortcuts & Investigation Guide",
                    "1 / ?",
                    MenuAction::ShowHelp,
                    "Complete categorized guide, shortcuts, and smart expressions",
                )],
            },
        ];

        Self {
            is_open: false,
            selected_category: 0,
            selected_item: 0,
            categories,
        }
    }

    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
        if self.is_open {
            self.selected_item = 0;
            if self.categories[self.selected_category]
                .items
                .get(0)
                .map_or(false, |i| i.is_separator)
            {
                self.next_item();
            }
        }
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn next_category(&mut self) {
        if self.selected_category + 1 < self.categories.len() {
            self.selected_category += 1;
        } else {
            self.selected_category = 0;
        }
        self.selected_item = 0;
        if self.categories[self.selected_category]
            .items
            .get(0)
            .map_or(false, |i| i.is_separator)
        {
            self.next_item();
        }
    }

    pub fn prev_category(&mut self) {
        if self.selected_category > 0 {
            self.selected_category -= 1;
        } else {
            self.selected_category = self.categories.len() - 1;
        }
        self.selected_item = 0;
        if self.categories[self.selected_category]
            .items
            .get(0)
            .map_or(false, |i| i.is_separator)
        {
            self.next_item();
        }
    }

    pub fn next_item(&mut self) {
        let items = &self.categories[self.selected_category].items;
        if items.is_empty() {
            return;
        }
        let mut idx = self.selected_item;
        for _ in 0..items.len() {
            idx = (idx + 1) % items.len();
            if !items[idx].is_separator {
                self.selected_item = idx;
                return;
            }
        }
    }

    pub fn prev_item(&mut self) {
        let items = &self.categories[self.selected_category].items;
        if items.is_empty() {
            return;
        }
        let mut idx = self.selected_item;
        for _ in 0..items.len() {
            idx = if idx == 0 { items.len() - 1 } else { idx - 1 };
            if !items[idx].is_separator {
                self.selected_item = idx;
                return;
            }
        }
    }

    pub fn select_current(&mut self) -> Option<MenuAction> {
        if !self.is_open {
            return None;
        }
        let item = &self.categories[self.selected_category].items[self.selected_item];
        if item.is_separator {
            return None;
        }
        let action = item.action;
        self.close();
        Some(action)
    }

    pub fn render_bar(
        &self,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        reader: &crate::reader::LogReader,
    ) {
        // Clear background
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, area.top())) {
                cell.set_style(Style::default().bg(theme.menu_bar_bg).fg(theme.menu_bar_fg));
            }
        }

        // Title icon: FLIK
        let title = "  FLIK ";
        buf.set_string(
            area.left(),
            area.top(),
            title,
            Style::default()
                .bg(theme.menu_bar_bg)
                .fg(ratatui::style::Color::Cyan)
                .add_modifier(ratatui::style::Modifier::BOLD),
        );

        let mut offset = area.left() + (title.len() as u16);

        for (idx, cat) in self.categories.iter().enumerate() {
            let is_active = self.is_open && idx == self.selected_category;
            let cat_str = format!("  {}  ", cat.name);
            let style = if is_active {
                theme.menu_selected
            } else {
                Style::default().bg(theme.menu_bar_bg).fg(theme.menu_bar_fg)
            };

            buf.set_string(offset, area.top(), &cat_str, style);

            // Highlight hotkey letter
            if !is_active {
                if let Some(pos) = cat.name.find(cat.hotkey) {
                    if let Some(cell) = buf.cell_mut((offset + 2 + (pos as u16), area.top())) {
                        cell.set_style(theme.menu_hotkey);
                    }
                }
            }

            offset += cat_str.len() as u16;
        }

        // Render Live Activity "Loupiote" on the top right
        let is_growing = reader.is_actively_growing();
        let is_tail = reader.is_tail_enabled();
        let incoming = reader.incoming_lines_count();

        let now_m = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let pulse = (now_m / 450) % 2 == 0;

        let (loupiote_str, loupiote_style) = if is_growing {
            let dot = if pulse { "🟢" } else { "●" };
            if is_tail {
                (
                    format!(" [ {} LIVE (TAILING) ] ", dot),
                    Style::default()
                        .bg(ratatui::style::Color::Rgb(0, 110, 40))
                        .fg(ratatui::style::Color::White)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                )
            } else {
                let msg = if incoming > 0 {
                    format!(" [ {} LIVE: +{} NEW (5:Follow) ] ", dot, incoming)
                } else {
                    format!(" [ {} LIVE STREAM ] ", dot)
                };
                (
                    msg,
                    Style::default()
                        .bg(ratatui::style::Color::Rgb(160, 80, 0))
                        .fg(ratatui::style::Color::Rgb(255, 240, 100))
                        .add_modifier(ratatui::style::Modifier::BOLD),
                )
            }
        } else if is_tail {
            (
                " [ 🟢 TAIL: WAITING ] ".to_string(),
                Style::default()
                    .bg(theme.menu_bar_bg)
                    .fg(ratatui::style::Color::Cyan)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )
        } else {
            (
                " [ TAIL: OFF (5/t) ] ".to_string(),
                Style::default()
                    .bg(theme.menu_bar_bg)
                    .fg(ratatui::style::Color::DarkGray),
            )
        };

        let loupiote_x = area.right().saturating_sub(loupiote_str.len() as u16 + 1);
        if loupiote_x > offset {
            buf.set_string(loupiote_x, area.top(), &loupiote_str, loupiote_style);
        }
    }

    pub fn contextual_label(
        &self,
        item: &MenuItem,
        reader: &crate::reader::LogReader,
        filter_engine: &crate::filter::FilterEngine,
        format_mgr: &crate::format::FormatManager,
        ui_state: &crate::ui::UiState,
        theme: &Theme,
    ) -> String {
        match item.action {
            MenuAction::ToggleTail => {
                if reader.is_tail_enabled() {
                    "[✓] Follow (tail -F)".to_string()
                } else {
                    "[ ] Follow (tail -F)".to_string()
                }
            }
            MenuAction::ToggleLineNumbers => {
                if ui_state.show_line_numbers {
                    "[✓] Line Numbers".to_string()
                } else {
                    "[ ] Line Numbers".to_string()
                }
            }
            MenuAction::ToggleColoration => {
                if ui_state.enable_coloration {
                    "[✓] Syntax Coloration".to_string()
                } else {
                    "[ ] Syntax Coloration".to_string()
                }
            }
            MenuAction::ToggleTheme => {
                let name = if theme.bg == ratatui::style::Color::Rgb(236, 239, 244) {
                    "Nord Polar Light"
                } else if theme.bg == ratatui::style::Color::Rgb(253, 246, 227) {
                    "Solarized Sand"
                } else if theme.bg == ratatui::style::Color::Rgb(244, 238, 224) {
                    "Warm Sepia"
                } else {
                    "Dark Slate"
                };
                format!("Theme: {} (T: cycle)", name)
            }
            MenuAction::ToggleReverseOrder => {
                if ui_state.reverse_order {
                    "[✓] Newest Logs First".to_string()
                } else {
                    "[ ] Newest Logs First".to_string()
                }
            }
            MenuAction::ToggleAutoIndentJson => {
                if ui_state.auto_indent_json {
                    "[✓] Auto-Indent JSON Messages".to_string()
                } else {
                    "[ ] Auto-Indent JSON Messages".to_string()
                }
            }
            MenuAction::ToggleMouseCapture => {
                "[✓] Mouse Interaction (Click/Scroll)".to_string()
            }
            MenuAction::ToggleViewMode => {
                if matches!(format_mgr.view_mode, crate::format::ViewMode::ColumnsTable) {
                    "Switch View: [●] Columns Table".to_string()
                } else {
                    "Switch View: [●] Compact Lines".to_string()
                }
            }
            MenuAction::AutoDetectFormat => {
                format!("Auto-Detect Format ({:?})", format_mgr.detected_format)
            }
            MenuAction::ForceJson => {
                if matches!(format_mgr.detected_format, crate::format::LogFormat::Json) {
                    "[●] Force JSON Format".to_string()
                } else {
                    "[ ] Force JSON Format".to_string()
                }
            }
            MenuAction::ForceApache => {
                if matches!(format_mgr.detected_format, crate::format::LogFormat::ApacheCombined) {
                    "[●] Force Apache / Nginx Combined".to_string()
                } else {
                    "[ ] Force Apache / Nginx Combined".to_string()
                }
            }
            MenuAction::ForceApacheError => {
                if matches!(format_mgr.detected_format, crate::format::LogFormat::ApacheError) {
                    "[●] Force Apache Error Log".to_string()
                } else {
                    "[ ] Force Apache Error Log".to_string()
                }
            }
            MenuAction::ForceNginxError => {
                if matches!(format_mgr.detected_format, crate::format::LogFormat::NginxError) {
                    "[●] Force Nginx Error Log".to_string()
                } else {
                    "[ ] Force Nginx Error Log".to_string()
                }
            }
            MenuAction::ForceK8s => {
                if matches!(format_mgr.detected_format, crate::format::LogFormat::Kubernetes) {
                    "[●] Force Kubernetes Format".to_string()
                } else {
                    "[ ] Force Kubernetes Format".to_string()
                }
            }
            MenuAction::ForceSyslog => {
                if matches!(format_mgr.detected_format, crate::format::LogFormat::Syslog) {
                    "[●] Force Syslog Format".to_string()
                } else {
                    "[ ] Force Syslog Format".to_string()
                }
            }
            MenuAction::ForceRaw => {
                if matches!(format_mgr.detected_format, crate::format::LogFormat::Raw) {
                    "[●] Force Raw Text".to_string()
                } else {
                    "[ ] Force Raw Text".to_string()
                }
            }
            MenuAction::ManageFilters => {
                let count = filter_engine.rules.len();
                if count == 0 {
                    "Manage Filter Stack (None active)".to_string()
                } else {
                    format!("Manage Filter Stack ({} active)", count)
                }
            }
            MenuAction::ClearFilters => {
                let count = filter_engine.rules.len();
                if count == 0 {
                    "Clear All Filters (Empty)".to_string()
                } else {
                    format!("Clear All Filters ({} rules)", count)
                }
            }
            _ => item.label.to_string(),
        }
    }

    pub fn render_dropdown(
        &self,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        reader: &crate::reader::LogReader,
        filter_engine: &crate::filter::FilterEngine,
        format_mgr: &crate::format::FormatManager,
        ui_state: &crate::ui::UiState,
    ) {
        if !self.is_open {
            return;
        }

        // Calculate offset for current category
        let mut x_offset = area.left() + 7; // after title
        for idx in 0..self.selected_category {
            let cat = &self.categories[idx];
            x_offset += (cat.name.len() as u16) + 4;
        }

        let cat = &self.categories[self.selected_category];
        let max_label_len = cat
            .items
            .iter()
            .map(|i| {
                if i.is_separator {
                    10
                } else {
                    self.contextual_label(i, reader, filter_engine, format_mgr, ui_state, theme).len()
                }
            })
            .max()
            .unwrap_or(18);

        let max_sc_len = cat
            .items
            .iter()
            .map(|i| i.shortcut.len())
            .max()
            .unwrap_or(0);

        let menu_width = ((max_label_len + max_sc_len + 8) as u16).max(36);
        let items_len = cat.items.len() as u16;
        let menu_height = (items_len + 4).min(area.height.saturating_sub(1));

        let x = x_offset.min(area.right().saturating_sub(menu_width));
        let y = area.top() + 1;

        let dropdown_area = Rect {
            x,
            y,
            width: menu_width.min(area.width),
            height: menu_height,
        };

        Clear.render(dropdown_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.menu_dropdown_border)
            .style(
                Style::default()
                    .bg(theme.menu_dropdown_bg)
                    .fg(theme.menu_dropdown_fg),
            );

        let inner = block.inner(dropdown_area);
        block.render(dropdown_area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let has_footer = inner.height >= items_len + 2;
        let items_visible = if has_footer {
            items_len.min(inner.height.saturating_sub(2))
        } else {
            items_len.min(inner.height)
        };

        for idx in 0..(items_visible as usize) {
            let item = &cat.items[idx];
            let row_y = inner.top() + (idx as u16);
            if row_y >= inner.bottom() {
                break;
            }

            if item.is_separator {
                let sep_len = (inner.width as usize).saturating_sub(2);
                let sep = format!(" ├{}┤", "─".repeat(sep_len.saturating_sub(2)));
                buf.set_string(inner.left(), row_y, &sep, Style::default().fg(Color::Rgb(120, 130, 145)));
            } else {
                let is_sel = idx == self.selected_item;
                let label = self.contextual_label(item, reader, filter_engine, format_mgr, ui_state, theme);
                let sc = item.shortcut;
                let pad = (inner.width as usize).saturating_sub(label.len() + sc.len() + 3);
                let spaces = " ".repeat(pad);
                let line = format!(" {}{}{} ", label, spaces, sc);
                let style = if is_sel {
                    theme.menu_selected
                } else {
                    Style::default()
                        .bg(theme.menu_dropdown_bg)
                        .fg(theme.menu_dropdown_fg)
                };
                buf.set_string(inner.left(), row_y, &line, style);
            }
        }

        // Contextual description footer
        if has_footer {
            let sep_y = inner.top() + items_len;
            if sep_y < inner.bottom() {
                let sep_line = "─".repeat(inner.width as usize);
                buf.set_string(inner.left(), sep_y, &sep_line, Style::default().fg(Color::Rgb(120, 130, 145)));
            }

            let hint_y = sep_y + 1;
            if hint_y < inner.bottom() {
                let desc = cat.items.get(self.selected_item).map(|i| i.description).unwrap_or("");
                let hint_text = format!(" ℹ {}", desc);
                let hint_style = Style::default().fg(Color::Yellow);
                let truncated = crate::ui::truncate_to_width(&hint_text, inner.width as usize);
                buf.set_string(inner.left(), hint_y, &truncated, hint_style);
            }
        }
    }
}
