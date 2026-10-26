pub mod menu;
pub mod modals;
pub mod theme;

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::filter::{FilterEngine, FilterKind};
use crate::format::{FormatManager, LogFormat, ViewMode};
use crate::reader::LogReader;
use crate::ui::menu::MenuBar;
use crate::ui::modals::{ActiveModal, ModalRenderer};
use crate::ui::theme::Theme;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Truncates a string to fit within `max_width` display columns in terminal.
/// Guaranteed to never panic on UTF-8 character boundaries.
pub fn truncate_to_width(s: &str, max_width: usize) -> &str {
    let mut current_width = 0;
    let mut byte_end = 0;
    for (idx, ch) in s.char_indices() {
        let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_w > max_width {
            return &s[..byte_end];
        }
        current_width += ch_w;
        byte_end = idx + ch.len_utf8();
    }
    s
}

/// Skips `skip_cols` display columns from the start of a string.
/// Guaranteed to never panic on UTF-8 character boundaries.
pub fn skip_columns(s: &str, skip_cols: usize) -> &str {
    if skip_cols == 0 {
        return s;
    }
    let mut current_col = 0;
    for (idx, ch) in s.char_indices() {
        if current_col >= skip_cols {
            return &s[idx..];
        }
        let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
        current_col += ch_w;
    }
    ""
}

/// Slices a string by display columns: skips `skip_cols`, then takes at most `max_width`.
/// Guaranteed to never panic on UTF-8 character boundaries.
pub fn slice_by_columns(s: &str, skip_cols: usize, max_width: usize) -> &str {
    let skipped = skip_columns(s, skip_cols);
    truncate_to_width(skipped, max_width)
}

pub struct UiState {
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub selected_line: usize,
    pub show_line_numbers: bool,
    pub enable_coloration: bool,
    pub quick_search_term: String,
    pub completion_hint: Option<String>,
    pub auto_indent_json: bool,
    pub reverse_order: bool,
    pub quit_warning: bool,
    pub status_toast: Option<(String, std::time::Instant)>,
    pub bookmarks: std::collections::BTreeSet<usize>,
    pub diff_pinned_line: Option<usize>,
    pub tail_following: bool,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            scroll_row: 0,
            scroll_col: 0,
            selected_line: 0,
            show_line_numbers: true,
            enable_coloration: true,
            quick_search_term: String::new(),
            completion_hint: None,
            auto_indent_json: false,
            reverse_order: false,
            quit_warning: false,
            status_toast: None,
            bookmarks: std::collections::BTreeSet::new(),
            diff_pinned_line: None,
            tail_following: false,
        }
    }
}

pub fn source_display_index(display_index: usize, total: usize, reverse_order: bool) -> usize {
    if reverse_order {
        total.saturating_sub(display_index.saturating_add(1))
    } else {
        display_index
    }
}

pub struct FlikUi;

impl FlikUi {
    pub fn render(
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        reader: &LogReader,
        filter_engine: &FilterEngine,
        format_mgr: &FormatManager,
        menu_bar: &MenuBar,
        modal: &ActiveModal,
        ui_state: &UiState,
    ) {
        let has_active_filters = filter_engine.has_active_filters();
        let has_custom_fields = format_mgr.visible_fields.len() < format_mgr.available_fields.len()
            && !format_mgr.visible_fields.is_empty();
        let has_search = !ui_state.quick_search_term.is_empty();

        let show_filter_bar = (has_active_filters || has_custom_fields || has_search) && area.height >= 8;

        let chunks = if show_filter_bar {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Top menu
                    Constraint::Min(4),    // Main content
                    Constraint::Length(1), // Active Filter bar
                    Constraint::Length(1), // Status bar
                    Constraint::Length(1), // Function keys
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Top menu
                    Constraint::Min(4),    // Main content
                    Constraint::Length(1), // Status bar
                    Constraint::Length(1), // Function keys
                ])
                .split(area)
        };

        if show_filter_bar {
            menu_bar.render_bar(chunks[0], buf, theme, reader);
            Self::render_main_viewport(chunks[1], buf, theme, reader, filter_engine, format_mgr, ui_state);
            Self::render_filter_bar(chunks[2], buf, theme, filter_engine, format_mgr, ui_state);
            Self::render_status_bar(chunks[3], buf, theme, reader, filter_engine, format_mgr, ui_state);
            Self::render_function_keys(chunks[4], buf, theme, reader, format_mgr);
        } else {
            menu_bar.render_bar(chunks[0], buf, theme, reader);
            Self::render_main_viewport(chunks[1], buf, theme, reader, filter_engine, format_mgr, ui_state);
            Self::render_status_bar(chunks[2], buf, theme, reader, filter_engine, format_mgr, ui_state);
            Self::render_function_keys(chunks[3], buf, theme, reader, format_mgr);
        }

        // Render open dropdown menu if active
        menu_bar.render_dropdown(
            chunks[1],
            buf,
            theme,
            reader,
            filter_engine,
            format_mgr,
            ui_state,
        );

        // Render modal popup if active
        ModalRenderer::render(modal, area, buf, theme, filter_engine, format_mgr);
        if matches!(modal, ActiveModal::Search { .. } | ActiveModal::AddFilter { .. }) {
            if let Some(hint) = &ui_state.completion_hint {
                if area.height > 2 {
                    Paragraph::new(hint.as_str()).style(Style::default().bg(theme.modal_bg).fg(Color::Yellow))
                        .render(Rect { y: area.bottom() - 2, height: 1, ..area }, buf);
                }
            }
        }
    }

    fn render_main_viewport(
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        reader: &LogReader,
        filter_engine: &FilterEngine,
        format_mgr: &FormatManager,
        ui_state: &UiState,
    ) {
        let total_lines = filter_engine.filtered_count(reader);
        let live_badge = if reader.is_actively_growing() {
            if reader.is_tail_enabled() {
                " [ 🟢 LIVE STREAM (TAILING) ]"
            } else {
                " [ 🟢 LIVE (+NEW) ]"
            }
        } else if reader.is_tail_enabled() {
            " [ 🟢 TAIL: ON ]"
        } else {
            ""
        };

        let order_badge = if ui_state.reverse_order { " [NEWEST FIRST]" } else { "" };
        let title = if filter_engine.has_active_filters() {
            format!(
                " flik: {} ({:.2} MB) - {} / {} lines [FILTERED]{}{} ",
                reader.path.file_name().unwrap_or_default().to_string_lossy(),
                (reader.file_size.load(std::sync::atomic::Ordering::Relaxed) as f64) / (1024.0 * 1024.0),
                total_lines,
                reader.line_count(),
                live_badge,
                order_badge
            )
        } else {
            format!(
                " flik: {} ({:.2} MB) - {} lines{}{} ",
                reader.path.file_name().unwrap_or_default().to_string_lossy(),
                (reader.file_size.load(std::sync::atomic::Ordering::Relaxed) as f64) / (1024.0 * 1024.0),
                total_lines,
                live_badge,
                order_badge
            )
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.frame_border)
            .title(title)
            .title_style(theme.frame_title)
            .style(Style::default().bg(theme.bg).fg(theme.fg));

        let inner = block.inner(area);
        block.render(area, buf);

        if total_lines == 0 {
            let msg = if filter_engine.has_active_filters() {
                "No lines match the active filters.\nPress 'x' to clear all filters or 'Ctrl+F' to manage."
            } else {
                "Empty file or loading in progress..."
            };
            Paragraph::new(msg)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray))
                .render(inner, buf);
            return;
        }

        let visible_height = inner.height as usize;
        let start_row = ui_state.scroll_row;
        let end_row = (start_row + visible_height).min(total_lines);

        // Read the visible page in one file operation where possible.
        let display_indices: Vec<usize> = (start_row..end_row)
            .map(|row| source_display_index(row, total_lines, ui_state.reverse_order))
            .collect();
        let display_lines: Vec<(usize, usize, String)> = if filter_engine.has_active_filters() {
            let rows: Vec<(usize, usize)> = display_indices
                .iter()
                .enumerate()
                .filter_map(|row| {
                    filter_engine
                        .get_original_line_index(*row.1, reader)
                        .map(|original_idx| (start_row + row.0, original_idx))
                })
                .collect();
            let original_indices: Vec<usize> = rows.iter().map(|(_, idx)| *idx).collect();
            reader
                .read_lines(&original_indices)
                .into_iter()
                .zip(rows)
                .filter_map(|(line, (row, original_idx))| {
                    line.map(|line| (row, original_idx, line))
                })
                .collect()
        } else if ui_state.reverse_order {
            reader
                .read_lines(&display_indices)
                .into_iter()
                .zip(start_row..end_row)
                .filter_map(|(line, display_idx)| {
                    line.map(|line| (display_idx, source_display_index(display_idx, total_lines, true), line))
                })
                .collect()
        } else {
            reader
                .read_range(start_row, end_row.saturating_sub(start_row))
                .into_iter()
                .map(|(row, line)| (row, row, line))
                .collect()
        };

        match format_mgr.view_mode {
            ViewMode::CompactLine => {
                Self::render_compact_lines(
                    inner,
                    buf,
                    theme,
                    &display_lines,
                    format_mgr,
                    filter_engine,
                    ui_state,
                    total_lines,
                );
            }
            ViewMode::ColumnsTable => {
                Self::render_table_view(
                    inner,
                    buf,
                    theme,
                    &display_lines,
                    format_mgr,
                    filter_engine,
                    ui_state,
                );
            }
        }
    }

    fn render_compact_lines(
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        lines: &[(usize, usize, String)],
        format_mgr: &FormatManager,
        filter_engine: &FilterEngine,
        ui_state: &UiState,
        total_lines: usize,
    ) {
        let max_line_num_digits = total_lines.to_string().len().max(4);

        let mut display_row = 0usize;
        for (display_idx, orig_line_idx, raw_line) in lines {
            let y = area.top().saturating_add(display_row as u16);
            if y >= area.bottom() {
                break;
            }

            let is_selected = *display_idx == ui_state.selected_line;
            let line_bg = if is_selected {
                theme.cursor_line
            } else {
                Style::default().bg(theme.bg)
            };

            // Fill line background
            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(line_bg);
                }
            }

            let mut x_pos = area.left();

            // 1. Line number gutter (with bookmark star)
            if ui_state.show_line_numbers {
                let is_bookmarked = ui_state.bookmarks.contains(&orig_line_idx);
                let mark = if is_bookmarked { "★ " } else if !filter_engine.line_matches(raw_line) { "· " } else { "  " };
                let num_str = format!("{}{:>width$} │ ", mark, orig_line_idx + 1, width = max_line_num_digits);
                let mark_style = if is_bookmarked {
                    Style::default().fg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD)
                } else {
                    theme.line_number
                };
                buf.set_string(x_pos, y, &num_str, mark_style.patch(line_bg));
                x_pos += num_str.len() as u16;
            }

            let fmt = format_mgr.active_format();
            let record = format_mgr.parse_line(raw_line);
            let has_field = |f: &str| format_mgr.visible_fields.iter().any(|v| v.eq_ignore_ascii_case(f));
            let pretty_json = if fmt == LogFormat::Kubernetes
                && ui_state.auto_indent_json
                && has_field("message")
            {
                serde_json::from_str::<serde_json::Value>(&record.message)
                    .ok()
                    .and_then(|value| serde_json::to_string_pretty(&value).ok())
            } else {
                None
            };
            if !ui_state.enable_coloration {
                let remaining_width = area.right().saturating_sub(x_pos) as usize;
                if remaining_width > 0 {
                    let text = if pretty_json.is_some() && fmt == LogFormat::Kubernetes {
                        &raw_line[..raw_line.len().saturating_sub(record.message.len())]
                    } else {
                        raw_line
                    };
                    let to_draw = slice_by_columns(text, ui_state.scroll_col, remaining_width);
                    buf.set_string(x_pos, y, to_draw, Style::default().fg(theme.fg).patch(line_bg));
                }
            } else {
                if fmt == LogFormat::Json || raw_line.trim().starts_with('{') {
                    let is_custom_fields = format_mgr.visible_fields.len() < format_mgr.available_fields.len()
                        && !format_mgr.visible_fields.is_empty();

                    let text_opt = if is_custom_fields {
                        let mut parts = Vec::new();
                        for vf in &format_mgr.visible_fields {
                            if let Some(val) = record.get_field_value(vf) {
                                let trimmed_val = val.trim();
                                if !trimmed_val.is_empty() {
                                    if trimmed_val.starts_with('{') || trimmed_val.starts_with('[') || trimmed_val.parse::<f64>().is_ok() || trimmed_val == "true" || trimmed_val == "false" || trimmed_val == "null" {
                                        parts.push(format!("\"{}\": {}", vf, trimmed_val));
                                    } else {
                                        parts.push(format!("\"{}\": \"{}\"", vf, trimmed_val));
                                    }
                                }
                            }
                        }
                        if parts.is_empty() {
                            None
                        } else {
                            Some(format!("{{ {} }}", parts.join(", ")))
                        }
                    } else {
                        Some(raw_line.trim().to_string())
                    };

                    if let Some(text) = text_opt {
                        let scrolled = skip_columns(&text, ui_state.scroll_col);
                        Self::render_colorized_json(buf, x_pos, y, area.right(), scrolled, line_bg, theme);
                    }
                } else {
                    // Structured non-JSON format: build styled segments
                    let mut segments: Vec<(String, Style)> = Vec::new();

                    match fmt {
                        LogFormat::ApacheCombined | LogFormat::ApacheCommon => {
                            if has_field("ip") {
                                if let Some(ip) = record.get_field_value("ip") {
                                    if !ip.is_empty() {
                                        segments.push((format!("{} ", ip), Style::default().fg(Color::Rgb(100, 210, 255))));
                                    }
                                }
                            }
                            if has_field("user") {
                                if let Some(user) = record.get_field_value("user") {
                                    if !user.is_empty() && user != "-" {
                                        segments.push((format!("{} ", user), Style::default().fg(Color::Rgb(140, 180, 200))));
                                    }
                                }
                            }
                            if has_field("timestamp") {
                                if let Some(ref ts) = record.timestamp {
                                    segments.push((format!("[{}] ", ts), theme.timestamp));
                                }
                            }
                            if let Some(ref lvl) = record.level {
                                let badge_style = match lvl.to_uppercase().as_str() {
                                    "INFO" => theme.level_info,
                                    "WARN" | "WARNING" => theme.level_warn,
                                    "ERROR" | "ERR" | "FATAL" | "CRITICAL" => theme.level_error,
                                    _ => theme.level_trace,
                                };
                                segments.push((format!("[{:<5}] ", lvl), badge_style));
                            }
                            if has_field("status") {
                                if let Some(st) = record.status {
                                    let status_style = match st {
                                        200..=299 => theme.status_2xx,
                                        300..=399 => theme.status_3xx,
                                        400..=499 => theme.status_4xx,
                                        500..=599 => theme.status_5xx,
                                        _ => Style::default().fg(Color::White),
                                    };
                                    segments.push((format!("[{}] ", st), status_style));
                                }
                            }
                            if has_field("request") {
                                if let Some(req) = record.get_field_value("request") {
                                    if !req.is_empty() {
                                        segments.push((format!("\"{}\" ", req), Style::default().fg(Color::Rgb(220, 245, 255)).add_modifier(Modifier::BOLD)));
                                    }
                                }
                            }
                            if has_field("size") {
                                if let Some(size) = record.get_field_value("size") {
                                    if !size.is_empty() {
                                        segments.push((format!("{} ", size), Style::default().fg(Color::Rgb(255, 175, 215))));
                                    }
                                }
                            }
                            if has_field("referer") {
                                if let Some(referer) = record.get_field_value("referer") {
                                    if !referer.is_empty() {
                                        segments.push((format!("\"{}\" ", referer), Style::default().fg(Color::Rgb(140, 160, 180))));
                                    }
                                }
                            }
                            if has_field("user_agent") {
                                if let Some(ua) = record.get_field_value("user_agent") {
                                    if !ua.is_empty() {
                                        segments.push((format!("\"{}\" ", ua), Style::default().fg(Color::Rgb(185, 195, 210))));
                                    }
                                }
                            }
                        }
                        LogFormat::Kubernetes => {
                            if has_field("timestamp") {
                                if let Some(ref ts) = record.timestamp {
                                    segments.push((format!("{} ", ts), theme.timestamp));
                                }
                            }
                            if has_field("stream") {
                                if let Some(stream) = record.get_field_value("stream") {
                                    let st_style = if stream.eq_ignore_ascii_case("stderr") { theme.level_error } else { theme.level_info };
                                    segments.push((format!("[{}] ", stream.to_uppercase()), st_style));
                                }
                            }
                            if has_field("flag") {
                                if let Some(flag) = record.get_field_value("flag") {
                                    segments.push((format!("{} ", flag), Style::default().fg(Color::DarkGray)));
                                }
                            }
                            for field in &format_mgr.visible_fields {
                                if matches!(field.as_str(), "timestamp" | "stream" | "flag" | "message") {
                                    continue;
                                }
                                if let Some(value) = record.get_field_value(field) {
                                    segments.push((
                                        format!("{}: {} ", field, value),
                                        Style::default().fg(Color::Rgb(100, 220, 255)),
                                    ));
                                }
                            }
                            if has_field("message") && pretty_json.is_none() {
                                segments.push((record.message.clone(), Style::default().fg(theme.fg)));
                            }
                        }
                        LogFormat::Syslog => {
                            if has_field("timestamp") {
                                if let Some(ref ts) = record.timestamp {
                                    segments.push((format!("{} ", ts), theme.timestamp));
                                }
                            }
                            if has_field("host") {
                                if let Some(host) = record.get_field_value("host") {
                                    segments.push((format!("{} ", host), Style::default().fg(Color::Rgb(100, 200, 255))));
                                }
                            }
                            if has_field("service") {
                                if let Some(service) = record.get_field_value("service") {
                                    let pid_str = record.get_field_value("pid").unwrap_or("");
                                    if !pid_str.is_empty() {
                                        segments.push((format!("{}[{}]: ", service, pid_str), Style::default().fg(Color::Rgb(255, 200, 100))));
                                    } else {
                                        segments.push((format!("{}: ", service), Style::default().fg(Color::Rgb(255, 200, 100))));
                                    }
                                }
                            }
                            if let Some(ref lvl) = record.level {
                                let lvl_style = match lvl.as_str() {
                                    "ERROR" => theme.level_error,
                                    "WARN" => theme.level_warn,
                                    _ => theme.level_info,
                                };
                                segments.push((format!("[{}] ", lvl), lvl_style));
                            }
                            if has_field("message") {
                                segments.push((record.message.clone(), Style::default().fg(theme.fg)));
                            }
                        }
                        LogFormat::ApacheError => {
                            let is_positional_active = format_mgr.visible_fields.iter().any(|f| {
                                f.starts_with("field_") || f.starts_with("col_") || f.starts_with('$')
                            });
                            if is_positional_active {
                                for field in &format_mgr.visible_fields {
                                    if let Some(value) = record.get_field_value(field) {
                                        segments.push((
                                            format!("{}: {} │ ", field, value),
                                            Style::default().fg(Color::Rgb(100, 220, 255)),
                                        ));
                                    }
                                }
                            } else {
                                if has_field("timestamp") {
                                    if let Some(ref ts) = record.timestamp {
                                        segments.push((format!("[{}] ", ts), theme.timestamp));
                                    }
                                }
                                if has_field("module") {
                                    if let Some(module) = record.get_field_value("module") {
                                        if !module.is_empty() {
                                            segments.push((format!("[{}] ", module), Style::default().fg(Color::Rgb(200, 160, 255))));
                                        }
                                    }
                                }
                                if let Some(ref lvl) = record.level {
                                    let lvl_style = match lvl.as_str() {
                                        "ERROR" => theme.level_error,
                                        "WARN" => theme.level_warn,
                                        "NOTICE" => theme.level_info,
                                        _ => theme.level_info,
                                    };
                                    segments.push((format!("[{}] ", lvl), lvl_style));
                                }
                                if has_field("pid") {
                                    if let Some(pid) = record.get_field_value("pid") {
                                        if !pid.is_empty() {
                                            segments.push((format!("[pid {}] ", pid), Style::default().fg(Color::Rgb(255, 200, 100))));
                                        }
                                    }
                                }
                                if has_field("client") {
                                    if let Some(client) = record.get_field_value("client") {
                                        if !client.is_empty() {
                                            segments.push((format!("[client {}] ", client), Style::default().fg(Color::Rgb(100, 210, 255))));
                                        }
                                    }
                                }
                                if has_field("message") {
                                    segments.push((record.message.clone(), Style::default().fg(theme.fg)));
                                }
                            }
                        }
                        LogFormat::NginxError => {
                            let is_positional_active = format_mgr.visible_fields.iter().any(|f| {
                                f.starts_with("field_") || f.starts_with("col_") || f.starts_with('$')
                            });
                            if is_positional_active {
                                for field in &format_mgr.visible_fields {
                                    if let Some(value) = record.get_field_value(field) {
                                        segments.push((
                                            format!("{}: {} │ ", field, value),
                                            Style::default().fg(Color::Rgb(100, 220, 255)),
                                        ));
                                    }
                                }
                            } else {
                                if has_field("timestamp") {
                                    if let Some(ref ts) = record.timestamp {
                                        segments.push((format!("[{}] ", ts), theme.timestamp));
                                    }
                                }
                                if let Some(ref lvl) = record.level {
                                    let lvl_style = match lvl.to_uppercase().as_str() {
                                        "ERROR" | "CRIT" | "ALERT" | "EMERG" => theme.level_error,
                                        "WARN" | "WARNING" => theme.level_warn,
                                        "NOTICE" | "INFO" => theme.level_info,
                                        _ => theme.level_trace,
                                    };
                                    segments.push((format!("[{:<5}] ", lvl), lvl_style));
                                }
                                if has_field("pid") {
                                    if let Some(pid) = record.get_field_value("pid") {
                                        if !pid.is_empty() {
                                            let tid = record.get_field_value("tid").unwrap_or("");
                                            if !tid.is_empty() {
                                                segments.push((format!("[{}:{}] ", pid, tid), Style::default().fg(Color::Rgb(255, 200, 100))));
                                            } else {
                                                segments.push((format!("[pid {}] ", pid), Style::default().fg(Color::Rgb(255, 200, 100))));
                                            }
                                        }
                                    }
                                }
                                if has_field("client") {
                                    if let Some(client) = record.get_field_value("client") {
                                        if !client.is_empty() {
                                            segments.push((format!("[client {}] ", client), Style::default().fg(Color::Rgb(100, 210, 255))));
                                        }
                                    }
                                }
                                if has_field("server") {
                                    if let Some(server) = record.get_field_value("server") {
                                        if !server.is_empty() {
                                            segments.push((format!("[server {}] ", server), Style::default().fg(Color::Rgb(140, 180, 200))));
                                        }
                                    }
                                }
                                if has_field("message") {
                                    segments.push((record.message.clone(), Style::default().fg(theme.fg)));
                                }
                            }
                        }
                        _ => {
                            let has_custom_fields = format_mgr.visible_fields != vec!["message".to_string()]
                                && !format_mgr.visible_fields.is_empty();

                            if has_custom_fields {
                                for vf in &format_mgr.visible_fields {
                                    if let Some(val) = record.get_field_value(vf) {
                                        segments.push((format!("{}: {} │ ", vf, val), Style::default().fg(Color::Rgb(100, 220, 255))));
                                    }
                                }
                            } else {
                                if let Some(ref lvl) = record.level {
                                    let lvl_style = match lvl.as_str() {
                                        "ERROR" => theme.level_error,
                                        "WARN" => theme.level_warn,
                                        _ => theme.level_info,
                                    };
                                    segments.push((format!("[{}] ", lvl), lvl_style));
                                }
                                segments.push((raw_line.clone(), Style::default().fg(theme.fg)));
                            }
                        }
                    }

                    // Render segments with horizontal scroll
                    let mut skipped = 0usize;
                    let mut current_x = x_pos;

                    for (seg_text, seg_style) in segments {
                        if current_x >= area.right() {
                            break;
                        }
                        let seg_width = UnicodeWidthStr::width(seg_text.as_str());
                        if skipped + seg_width <= ui_state.scroll_col {
                            skipped += seg_width;
                            continue;
                        }

                        let skip_in_seg = ui_state.scroll_col.saturating_sub(skipped);
                        skipped += seg_width;

                        let max_avail = (area.right().saturating_sub(current_x)) as usize;
                        let to_draw = slice_by_columns(&seg_text, skip_in_seg, max_avail);
                        if !to_draw.is_empty() {
                            if fmt == LogFormat::Kubernetes
                                && seg_text == record.message
                                && record.fields.len() > 4
                            {
                                Self::render_colorized_json(
                                    buf,
                                    current_x,
                                    y,
                                    area.right(),
                                    to_draw,
                                    line_bg,
                                    theme,
                                );
                            } else {
                                buf.set_string(current_x, y, to_draw, seg_style.patch(line_bg));
                            }
                            current_x += UnicodeWidthStr::width(to_draw) as u16;
                        }
                    }
                }
            }

            // Highlighting: matches on rendered line
            let mut line_chars = Vec::new();
            for x in x_pos..area.right() {
                if let Some(cell) = buf.cell((x, y)) {
                    line_chars.push(cell.symbol().chars().next().unwrap_or(' '));
                }
            }
            let line_text: String = line_chars.into_iter().collect();
            let line_lower = line_text.to_lowercase();

            // 1. Highlight quick search
            if !ui_state.quick_search_term.is_empty() {
                let query = ui_state.quick_search_term.to_lowercase();
                let mut start = 0;
                while let Some(pos) = line_lower[start..].find(&query) {
                    let m_start = start + pos;
                    let m_end = m_start + query.len();
                    for hx in (x_pos as usize + m_start)..(x_pos as usize + m_end) {
                        if (hx as u16) < area.right() {
                            if let Some(cell) = buf.cell_mut((hx as u16, y)) {
                                cell.set_style(theme.highlight_match);
                            }
                        }
                    }
                    start = m_end;
                }
            }

            // 2. Highlight active include filters
            for rule in &filter_engine.rules {
                if rule.enabled && rule.kind == FilterKind::Include && !rule.pattern.is_empty() {
                    let pattern_raw = crate::filter::smart_find_parts(&rule.pattern).ok().flatten()
                        .map_or(rule.pattern.as_str(), |(_, _, term)| term).to_lowercase();
                    let query = if let Some((_, val)) = pattern_raw.split_once(':') {
                        val.trim()
                    } else if let Some((_, val)) = pattern_raw.split_once('>') {
                        val.trim().trim_start_matches('=').trim()
                    } else if let Some((_, val)) = pattern_raw.split_once('<') {
                        val.trim().trim_start_matches('=').trim()
                    } else {
                        pattern_raw.as_str()
                    };

                    if !query.is_empty() && query.len() >= 2 {
                        let filter_highlight = Style::default()
                            .bg(Color::Rgb(120, 80, 20))
                            .fg(Color::Rgb(255, 235, 120))
                            .add_modifier(Modifier::BOLD);

                        let mut start = 0;
                        while let Some(pos) = line_lower[start..].find(query) {
                            let m_start = start + pos;
                            let m_end = m_start + query.len();
                            for hx in (x_pos as usize + m_start)..(x_pos as usize + m_end) {
                                if (hx as u16) < area.right() {
                                    if let Some(cell) = buf.cell_mut((hx as u16, y)) {
                                        cell.set_style(filter_highlight);
                                    }
                                }
                            }
                            start = m_end;
                        }
                    }
                }
            }

            if !filter_engine.line_matches(raw_line) {
                for x in area.left()..area.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) { cell.set_fg(Color::DarkGray); }
                }
            }
            display_row += 1;
            if let Some(json) = pretty_json {
                let gutter_width = if ui_state.show_line_numbers {
                    max_line_num_digits + 5
                } else {
                    0
                };
                let json_x = area
                    .left()
                    .saturating_add(gutter_width as u16)
                    .saturating_add(2);
                for json_line in json.lines() {
                    let json_y = area.top().saturating_add(display_row as u16);
                    if json_y >= area.bottom() {
                        break;
                    }
                    for x in area.left()..area.right() {
                        if let Some(cell) = buf.cell_mut((x, json_y)) {
                            cell.set_style(Style::default().bg(theme.bg));
                        }
                    }
                    if ui_state.enable_coloration {
                        Self::render_colorized_json(
                            buf,
                            json_x,
                            json_y,
                            area.right(),
                            json_line,
                            Style::default().bg(theme.bg),
                            theme,
                        );
                    } else {
                        let remaining = area.right().saturating_sub(json_x) as usize;
                        buf.set_string(
                            json_x,
                            json_y,
                            truncate_to_width(json_line, remaining),
                            Style::default().fg(theme.fg),
                        );
                    }
                    display_row += 1;
                }
            }
        }
    }

    /// Colorizes JSON keys, values, numbers, and syntax punctuation
    pub fn render_colorized_json(
        buf: &mut Buffer,
        mut x: u16,
        y: u16,
        max_x: u16,
        text: &str,
        line_bg: Style,
        theme: &Theme,
    ) {
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let n = chars.len();

        while i < n && x < max_x {
            match chars[i] {
                '"' => {
                    // String literal: determine if it's a JSON key (followed by :)
                    let start = i;
                    i += 1;
                    while i < n && chars[i] != '"' {
                        if chars[i] == '\\' && i + 1 < n {
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    if i < n {
                        i += 1;
                    }
                    let token: String = chars[start..i].iter().collect();

                    // Look ahead for colon (indicating KEY)
                    let mut is_key = false;
                    let mut lookahead = i;
                    while lookahead < n && chars[lookahead].is_whitespace() {
                        lookahead += 1;
                    }
                    if lookahead < n && chars[lookahead] == ':' {
                        is_key = true;
                    }

                    let style = if is_key {
                        theme.json_key
                    } else {
                        theme.json_string
                    };

                    let to_draw = truncate_to_width(&token, (max_x.saturating_sub(x)) as usize);
                    buf.set_string(x, y, to_draw, style.patch(line_bg));
                    x += UnicodeWidthStr::width(to_draw) as u16;
                }
                '{' | '}' | '[' | ']' | ',' => {
                    buf.set_string(x, y, &chars[i].to_string(), theme.json_punctuation.patch(line_bg));
                    x += 1;
                    i += 1;
                }
                ':' => {
                    buf.set_string(x, y, ":", theme.json_colon.patch(line_bg));
                    x += 1;
                    i += 1;
                }
                c if c.is_ascii_digit() || c == '-' => {
                    let start = i;
                    while i < n && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'e' || chars[i] == 'E' || chars[i] == '-') {
                        i += 1;
                    }
                    let token: String = chars[start..i].iter().collect();
                    let style = theme.json_number;
                    let to_draw = truncate_to_width(&token, (max_x.saturating_sub(x)) as usize);
                    buf.set_string(x, y, to_draw, style.patch(line_bg));
                    x += UnicodeWidthStr::width(to_draw) as u16;
                }
                c if c.is_ascii_alphabetic() => {
                    let start = i;
                    while i < n && chars[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let token: String = chars[start..i].iter().collect();
                    let style = match token.as_str() {
                        "true" | "false" | "null" => theme.json_literal,
                        _ => Style::default().fg(theme.fg),
                    };
                    let to_draw = truncate_to_width(&token, (max_x.saturating_sub(x)) as usize);
                    buf.set_string(x, y, to_draw, style.patch(line_bg));
                    x += UnicodeWidthStr::width(to_draw) as u16;
                }
                c => {
                    let ch_str = c.to_string();
                    let ch_w = UnicodeWidthChar::width(c).unwrap_or(1);
                    if x + (ch_w as u16) <= max_x {
                        buf.set_string(x, y, &ch_str, Style::default().fg(theme.fg).patch(line_bg));
                        x += ch_w as u16;
                    }
                    i += 1;
                }
            }
        }
    }

    fn render_table_view(
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        lines: &[(usize, usize, String)],
        format_mgr: &FormatManager,
        filter_engine: &FilterEngine,
        ui_state: &UiState,
    ) {
        if area.height < 2 {
            return;
        }

        // 1. Render Table Header
        let header_y = area.top();
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, header_y)) {
                cell.set_style(Style::default().bg(theme.menu_bar_bg).fg(Color::Yellow).add_modifier(Modifier::BOLD));
            }
        }

        let col_width = 20usize;
        let mut header_x = area.left();

        // Line number column
        if ui_state.show_line_numbers {
            let col_hdr = format!(" {:<6}│", "LINE");
            buf.set_string(header_x, header_y, &col_hdr, Style::default().fg(Color::Cyan));
            header_x += col_hdr.len() as u16;
        }

        for field in &format_mgr.visible_fields {
            if header_x >= area.right() {
                break;
            }
            let hdr = format!(" {:<width$}│", field.to_uppercase(), width = col_width - 2);
            buf.set_string(header_x, header_y, &hdr, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
            header_x += col_width as u16;
        }

        // 2. Render Rows
        for (idx, (display_idx, orig_line_idx, raw_line)) in lines.iter().enumerate() {
            let y = area.top() + 1 + (idx as u16);
            if y >= area.bottom() {
                break;
            }

            let is_sel = *display_idx == ui_state.selected_line;
            let line_bg = if is_sel {
                theme.cursor_line
            } else {
                Style::default().bg(theme.bg)
            };

            for x in area.left()..area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(line_bg);
                }
            }

            let mut row_x = area.left();
            if ui_state.show_line_numbers {
                let is_bookmarked = ui_state.bookmarks.contains(&orig_line_idx);
                let mark = if is_bookmarked { "★" } else if !filter_engine.line_matches(raw_line) { "·" } else { " " };
                let num_str = format!("{}{:<5}│", mark, orig_line_idx + 1);
                let mark_style = if is_bookmarked {
                    Style::default().fg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD)
                } else {
                    theme.line_number
                };
                buf.set_string(row_x, y, &num_str, mark_style.patch(line_bg));
                row_x += num_str.len() as u16;
            }

            let record = format_mgr.parse_line(raw_line);
            for field in &format_mgr.visible_fields {
                if row_x >= area.right() {
                    break;
                }
                let val = record.get_field_value(field).unwrap_or("-");
                let max_col_text = col_width.saturating_sub(2);
                let val_width = UnicodeWidthStr::width(val);
                let truncated = if val_width > max_col_text {
                    let short = truncate_to_width(val, max_col_text.saturating_sub(1));
                    let short_w = UnicodeWidthStr::width(short);
                    let pad = " ".repeat(max_col_text.saturating_sub(short_w + 1));
                    format!("{}…{}│", short, pad)
                } else {
                    let pad = " ".repeat(max_col_text.saturating_sub(val_width));
                    format!("{}{}{}│", val, pad, "")
                };

                let cell_style = if field == "level" {
                    match val.to_uppercase().as_str() {
                        "INFO" => theme.level_info,
                        "WARN" => theme.level_warn,
                        "ERROR" => theme.level_error,
                        _ => Style::default().fg(theme.fg),
                    }
                } else if field == "status" {
                    match val.parse::<u16>() {
                        Ok(200..=299) => theme.status_2xx,
                        Ok(400..=499) => theme.status_4xx,
                        Ok(500..=599) => theme.status_5xx,
                        _ => Style::default().fg(theme.fg),
                    }
                } else {
                    Style::default().fg(theme.fg)
                };

                buf.set_string(row_x, y, &truncated, cell_style.patch(line_bg));
                row_x += col_width as u16;
            }
            if !filter_engine.line_matches(raw_line) {
                for x in area.left()..area.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) { cell.set_fg(Color::DarkGray); }
                }
            }
        }
    }

    fn render_filter_bar(
        area: Rect,
        buf: &mut Buffer,
        _theme: &Theme,
        filter_engine: &FilterEngine,
        format_mgr: &FormatManager,
        ui_state: &UiState,
    ) {
        let bar_bg = Color::Rgb(16, 26, 42); // Deep dark navy
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, area.top())) {
                cell.set_style(Style::default().bg(bar_bg));
            }
        }

        let mut x = area.left() + 1;
        let y = area.top();

        // 1. Icon & Label
        let label = " ⚡ FILTERS: ";
        buf.set_string(
            x,
            y,
            label,
            Style::default().bg(bar_bg).fg(Color::Rgb(255, 205, 60)).add_modifier(Modifier::BOLD),
        );
        x += label.len() as u16;

        // 2. Render each active filter rule badge
        for rule in filter_engine.rules.iter().filter(|r| r.enabled) {
            if x + 6 >= area.right() {
                break;
            }

            let (badge_text, bg_col, fg_col) = match rule.kind {
                FilterKind::Include => (
                    format!(" [+] {} ", rule.pattern),
                    Color::Rgb(15, 50, 35),
                    Color::Rgb(60, 240, 140),
                ),
                FilterKind::Exclude => (
                    format!(" [-] {} ", rule.pattern),
                    Color::Rgb(55, 18, 22),
                    Color::Rgb(255, 105, 105),
                ),
            };

            let badge_style = Style::default().bg(bg_col).fg(fg_col).add_modifier(Modifier::BOLD);
            buf.set_string(x, y, &badge_text, badge_style);
            x += badge_text.len() as u16 + 1;
        }

        // 3. Quick Search badge if active (only if not already displayed in active rules)
        let is_already_filtered = filter_engine
            .rules
            .iter()
            .any(|r| r.enabled && r.pattern.eq_ignore_ascii_case(&ui_state.quick_search_term));

        if !ui_state.quick_search_term.is_empty() && !is_already_filtered && x + 10 < area.right() {
            let s_text = format!(" [/{}] ", ui_state.quick_search_term);
            buf.set_string(
                x,
                y,
                &s_text,
                Style::default().bg(Color::Rgb(45, 20, 60)).fg(Color::Rgb(240, 130, 255)).add_modifier(Modifier::BOLD),
            );
            x += s_text.len() as u16 + 1;
        }

        // 4. Custom visible fields badge if active
        if format_mgr.visible_fields.len() < format_mgr.available_fields.len() && x + 12 < area.right() {
            let f_text = format!(" [Fields: {}/{} visible] ", format_mgr.visible_fields.len(), format_mgr.available_fields.len());
            buf.set_string(
                x,
                y,
                &f_text,
                Style::default().bg(Color::Rgb(40, 40, 20)).fg(Color::Rgb(255, 225, 90)).add_modifier(Modifier::BOLD),
            );
            x += f_text.len() as u16 + 1;
        }

        // 5. Action helpers on the right side
        let right_hints = " [x] Undo Filter  │  [X] Clear All  │  [3/+] Add  │  [Ctrl+F] Manage ";
        let right_x = area.right().saturating_sub(right_hints.len() as u16 + 1);
        if right_x > x {
            buf.set_string(
                right_x,
                y,
                right_hints,
                Style::default().bg(bar_bg).fg(Color::Rgb(140, 180, 220)).add_modifier(Modifier::BOLD),
            );
        }
    }

    fn render_status_bar(
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        reader: &LogReader,
        filter_engine: &FilterEngine,
        format_mgr: &FormatManager,
        ui_state: &UiState,
    ) {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, area.top())) {
                cell.set_style(Style::default().bg(theme.status_bar_bg).fg(theme.status_bar_fg));
            }
        }

        let total_lines = filter_engine.filtered_count(reader);
        let current_line = (ui_state.selected_line + 1).min(total_lines);
        let pct = if total_lines > 0 {
            (current_line as f64 / total_lines as f64) * 100.0
        } else {
            0.0
        };

        let indexing_status = if reader.is_indexing.load(std::sync::atomic::Ordering::Relaxed) {
            " | [Indexing...] "
        } else {
            ""
        };

        let filter_status = if filter_engine.has_active_filters() {
            let active_rules: Vec<&str> = filter_engine.rules.iter().filter(|r| r.enabled).map(|r| r.pattern.as_str()).collect();
            format!(" | Filters: [{}] (x:clear) ", active_rules.join(", "))
        } else {
            " | No filter ".to_string()
        };

        let is_growing = reader.is_actively_growing();
        let is_tail = reader.is_tail_enabled();
        let incoming = reader.incoming_lines_count();

        let (tail_status, tail_style) = if is_growing {
            let now_m = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let pulse = (now_m / 450) % 2 == 0;
            let dot = if pulse { "🟢" } else { "●" };
            if is_tail {
                if ui_state.tail_following {
                    (
                        format!(" | {} LIVE (tail -F) [5/t] ", dot),
                        Style::default().bg(Color::Rgb(0, 110, 40)).fg(Color::Rgb(120, 255, 170)).add_modifier(Modifier::BOLD)
                    )
                } else {
                    (
                        format!(" | ⏸ TAIL (paused - press G to follow) [5/t] "),
                        Style::default().bg(Color::Rgb(130, 60, 0)).fg(Color::Rgb(255, 230, 100)).add_modifier(Modifier::BOLD)
                    )
                }
            } else {
                let msg = if incoming > 0 {
                    format!(" | {} LIVE: +{} new lines! [Press 5/t to follow] ", dot, incoming)
                } else {
                    format!(" | {} LIVE STREAM! [Press 5/t to follow] ", dot)
                };
                (
                    msg,
                    Style::default().bg(Color::Rgb(160, 80, 0)).fg(Color::Rgb(255, 240, 100)).add_modifier(Modifier::BOLD)
                )
            }
        } else if is_tail {
            if ui_state.tail_following {
                (
                    " | 🟢 TAIL: ON (waiting) [5/t] ".to_string(),
                    Style::default().bg(theme.status_bar_bg).fg(Color::Cyan).add_modifier(Modifier::BOLD)
                )
            } else {
                (
                    " | ⏸ TAIL (paused - press G to follow) [5/t] ".to_string(),
                    Style::default().bg(theme.status_bar_bg).fg(Color::Yellow).add_modifier(Modifier::BOLD)
                )
            }
        } else {
            (
                " | TAIL: Off [5/t] ".to_string(),
                Style::default().bg(theme.status_bar_bg).fg(theme.status_bar_fg)
            )
        };

        let view_mode_str = match format_mgr.view_mode {
            ViewMode::CompactLine => "Line",
            ViewMode::ColumnsTable => "Table",
        };

        let bookmark_indicator = if !ui_state.bookmarks.is_empty() {
            format!(" | ★ {} ", ui_state.bookmarks.len())
        } else {
            String::new()
        };

        let diff_indicator = if let Some(pinned) = ui_state.diff_pinned_line {
            format!(" | 📌 Diff:#{} ", pinned + 1)
        } else {
            String::new()
        };

        let status_left = if ui_state.quit_warning {
            " [ PRESS 'q' AGAIN TO QUIT (QQ) ] ".to_string()
        } else if let Some((ref msg, _)) = ui_state.status_toast {
            format!(" [ {} ] ", msg)
        } else {
            let total_raw = reader.line_count();
            let count_info = if filter_engine.has_active_filters() {
                format!(" {} / {} (filtered from {}) ({:.1}%)", current_line, total_lines, total_raw, pct)
            } else {
                format!(" {} / {} ({:.1}%)", current_line, total_lines, pct)
            };
            format!(
                " Line:{}{}{}{}{} | Format: {} | Fields: {}/{} | View: {} ",
                count_info,
                bookmark_indicator,
                diff_indicator,
                indexing_status,
                filter_status,
                format_mgr.active_format().name(),
                format_mgr.visible_fields.len(),
                format_mgr.available_fields.len(),
                view_mode_str
            )
        };

        let status_style = if ui_state.quit_warning {
            Style::default().bg(Color::Red).fg(Color::White).add_modifier(Modifier::BOLD)
        } else if ui_state.status_toast.is_some() {
            Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(theme.status_bar_bg).fg(theme.status_bar_fg)
        };

        buf.set_string(area.left(), area.top(), &status_left, status_style);

        let status_right = format!("{}[Esc]: Menu ", tail_status);
        let right_x = area.right().saturating_sub(status_right.len() as u16);
        if right_x > area.left() {
            buf.set_string(
                right_x,
                area.top(),
                &status_right,
                tail_style,
            );
        }
    }

    fn render_function_keys(
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        reader: &LogReader,
        format_mgr: &FormatManager,
    ) {
        // Clear bar
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, area.top())) {
                cell.set_style(Style::default().bg(theme.fn_key_bg));
            }
        }

        let is_growing = reader.is_actively_growing();
        let tail_label = if is_growing && !reader.is_tail_enabled() {
            "⚡Follow!"
        } else if reader.is_tail_enabled() {
            "●Tail:ON"
        } else {
            "Tail:OFF"
        };
        let view_label = match format_mgr.view_mode {
            ViewMode::CompactLine => "Table",
            ViewMode::ColumnsTable => "Line",
        };

        // Mac & standard accessible keybindings
        let keys = [
            ("1/h", "Help"),
            ("2/o", "Fmt"),
            ("3/+", "+Inc"),
            ("4/Tab", view_label),
            ("5/t", tail_label),
            ("6/f", "Fields"),
            ("7//", "Find"),
            ("8/-", "-Exc"),
            ("9/m", "Menu"),
            ("QQ", "Quit"),
        ];

        let key_width = (area.width / (keys.len() as u16)).max(6);
        for (i, (key_shortcut, label)) in keys.iter().enumerate() {
            let x = area.left() + (i as u16 * key_width);
            if x + key_width > area.right() {
                break;
            }

            // Key shortcut badge
            let num_str = format!("{:<5}", key_shortcut);
            buf.set_string(x, area.top(), &num_str, theme.fn_key_num);

            // Label text
            let remaining = (key_width as usize).saturating_sub(num_str.len());
            let label_str = format!("{:<width$}", label, width = remaining);
            buf.set_string(x + (num_str.len() as u16), area.top(), &label_str, theme.fn_key_label);
        }
    }
}
