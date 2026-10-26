use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget},
};

use crate::filter::{FilterEngine, FilterKind};
use crate::format::FormatManager;
use crate::ui::theme::Theme;
use crate::ui::truncate_to_width;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveModal {
    None,
    Report { title: String, lines: Vec<String>, scroll: usize, horizontal: usize },
    AddFilter {
        pattern: String,
        kind: FilterKind,
        is_regex: bool,
        is_case_sensitive: bool,
    },
    FilterManager {
        selected_index: usize,
    },
    FieldSelector {
        selected_index: usize,
    },
    SelectFieldRange {
        input: String,
    },
    InspectFieldValues {
        field_name: String,
        results: Option<crate::format::FieldInspectionResult>,
        selected_index: usize,
        scroll_offset: usize,
    },
    Export {
        output_path: String,
        selected_format_idx: usize, // 0: Raw, 1: JSON, 2: CSV
        selected_scope_idx: usize,  // 0: All filtered, 1: Visible
        focused_field: usize,       // 0: path, 1: scope, 2: format
    },
    Search {
        query: String,
    },
    StatsDashboard {
        stats: crate::analytics::StatsSummary,
        selected_tab: usize,
        selected_item: usize,
    },
    TimelineHistogram {
        histogram: crate::analytics::TimelineHistogram,
        selected_bucket: usize,
    },
    PatternClustering {
        clusters: Vec<crate::analytics::LogCluster>,
        selected_index: usize,
        scroll_offset: usize,
    },
    LineDiff {
        line1_idx: usize,
        line1_text: String,
        line2_idx: usize,
        line2_text: String,
    },
    TraceCorrelation {
        line_idx: usize,
        identifiers: Vec<(String, String)>,
        selected_index: usize,
    },
    Help {
        tab: usize,
    },
    Message {
        title: String,
        body: String,
        is_error: bool,
    },
}

pub struct ModalRenderer;

impl ModalRenderer {
    pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }

    pub fn render(
        modal: &ActiveModal,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
        filter_engine: &FilterEngine,
        format_mgr: &FormatManager,
    ) {
        match modal {
            ActiveModal::Report { title, lines, scroll, horizontal } => {
                let popup = Self::centered_rect(95, 90, area);
                Clear.render(popup, buf);
                let block = Block::default().borders(Borders::ALL).title(title.as_str())
                    .style(Style::default().bg(theme.modal_bg).fg(theme.fg));
                let inner = block.inner(popup);
                block.render(popup, buf);
                let content = Rect { height: inner.height.saturating_sub(1), ..inner };
                Paragraph::new(lines.iter().skip(*scroll).take(content.height as usize)
                    .map(|s| Line::raw(s.clone())).collect::<Vec<_>>())
                    .scroll((0, (*horizontal).min(u16::MAX as usize) as u16)).render(content, buf);
                if inner.height > 0 {
                    Paragraph::new(format!("↑↓ / PgUp PgDn: scroll | ←→: pan | Esc: close | {}/{} lines", scroll + 1, lines.len()))
                        .style(Style::default().fg(Color::Yellow))
                        .render(Rect { y: inner.bottom() - 1, height: 1, ..inner }, buf);
                }
            }
            ActiveModal::None => {}
            ActiveModal::AddFilter {
                pattern,
                kind,
                is_regex,
                is_case_sensitive,
            } => {
                Self::render_add_filter(pattern, *kind, *is_regex, *is_case_sensitive, area, buf, theme);
            }
            ActiveModal::FilterManager { selected_index } => {
                Self::render_filter_manager(*selected_index, filter_engine, area, buf, theme);
            }
            ActiveModal::FieldSelector { selected_index } => {
                Self::render_field_selector(*selected_index, format_mgr, area, buf, theme);
            }
            ActiveModal::SelectFieldRange { input } => {
                Self::render_select_field_range(input, format_mgr, area, buf, theme);
            }
            ActiveModal::InspectFieldValues {
                field_name,
                results,
                selected_index,
                scroll_offset,
            } => {
                Self::render_inspect_field_values(
                    field_name,
                    results,
                    *selected_index,
                    *scroll_offset,
                    format_mgr,
                    area,
                    buf,
                    theme,
                );
            }
            ActiveModal::Export {
                output_path,
                selected_format_idx,
                selected_scope_idx,
                focused_field,
            } => {
                Self::render_export(
                    output_path,
                    *selected_format_idx,
                    *selected_scope_idx,
                    *focused_field,
                    area,
                    buf,
                    theme,
                );
            }
            ActiveModal::Search { query } => {
                Self::render_search(query, area, buf, theme);
            }
            ActiveModal::StatsDashboard {
                stats,
                selected_tab,
                selected_item,
            } => {
                Self::render_stats_dashboard(stats, *selected_tab, *selected_item, area, buf, theme);
            }
            ActiveModal::TimelineHistogram {
                histogram,
                selected_bucket,
            } => {
                Self::render_timeline_histogram(histogram, *selected_bucket, area, buf, theme);
            }
            ActiveModal::PatternClustering {
                clusters,
                selected_index,
                scroll_offset,
            } => {
                Self::render_pattern_clustering(clusters, *selected_index, *scroll_offset, area, buf, theme);
            }
            ActiveModal::LineDiff {
                line1_idx,
                line1_text,
                line2_idx,
                line2_text,
            } => {
                Self::render_line_diff(*line1_idx, line1_text, *line2_idx, line2_text, area, buf, theme);
            }
            ActiveModal::TraceCorrelation {
                line_idx,
                identifiers,
                selected_index,
            } => {
                Self::render_trace_correlation(*line_idx, identifiers, *selected_index, area, buf, theme);
            }
            ActiveModal::Help { tab } => {
                Self::render_help(*tab, area, buf, theme);
            }
            ActiveModal::Message {
                title,
                body,
                is_error,
            } => {
                Self::render_message(title, body, *is_error, area, buf, theme);
            }
        }
    }

    fn render_add_filter(
        pattern: &str,
        kind: FilterKind,
        is_regex: bool,
        is_case_sensitive: bool,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(65, 36, area);
        Clear.render(popup_area, buf);

        let title = match kind {
            FilterKind::Include => " [+] Add Include Filter ",
            FilterKind::Exclude => " [-] Add Exclude Filter ",
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(title)
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1), // label
                Constraint::Length(3), // input box
                Constraint::Length(1), // smart suggestions
                Constraint::Length(2), // toggles
                Constraint::Length(2), // hint / buttons
            ])
            .split(inner);

        // Label
        let label = Line::from(vec![
            Span::raw("Pattern or keyword to "),
            Span::styled(
                if kind == FilterKind::Include { "INCLUDE" } else { "EXCLUDE" },
                if kind == FilterKind::Include { theme.level_info } else { theme.level_error },
            ),
            Span::raw(" :"),
        ]);
        Paragraph::new(label).render(chunks[0], buf);

        // Input
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(theme.modal_input_bg));
        let input_text = format!("{}█", pattern);
        Paragraph::new(input_text)
            .block(input_block)
            .render(chunks[1], buf);

        // Smart Filter suggestions
        let smart_line = Line::from(vec![
            Span::styled("Smart Examples: ", Style::default().fg(Color::Yellow)),
            Span::styled("amount > 100", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" | "),
            Span::styled("status:5xx", Style::default().fg(Color::Cyan)),
            Span::raw(" | "),
            Span::styled("status>=400", Style::default().fg(Color::LightRed)),
            Span::raw(" | "),
            Span::styled("fraud_score>0.1", Style::default().fg(Color::LightYellow)),
            Span::raw(" | "),
            Span::styled("level:error", Style::default().fg(Color::Magenta)),
        ]);
        Paragraph::new(smart_line).render(chunks[2], buf);

        // Toggles
        let kind_str = match kind {
            FilterKind::Include => "[Tab] Include (+) · Ctrl+Space: complete",
            FilterKind::Exclude => "[Tab] Exclude (-) · Ctrl+Space: complete",
        };
        let regex_str = if is_regex { "[F2/r] Regex: YES" } else { "[F2/r] Regex: NO" };
        let case_str = if is_case_sensitive { "[F3/c] Case: MATCH" } else { "[F3/c] Case: IGNORE" };

        let toggle_line = Line::from(vec![
            Span::styled(kind_str, Style::default().fg(Color::Cyan)),
            Span::raw("   "),
            Span::styled(regex_str, Style::default().fg(Color::Yellow)),
            Span::raw("   "),
            Span::styled(case_str, Style::default().fg(Color::LightBlue)),
        ]);
        Paragraph::new(toggle_line).render(chunks[3], buf);

        // Buttons
        let buttons = Line::from(vec![
            Span::styled("[ Enter ] Confirm", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("        "),
            Span::styled("[ Esc ] Cancel", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(buttons)
            .alignment(Alignment::Center)
            .render(chunks[4], buf);
    }

    fn render_filter_manager(
        selected: usize,
        engine: &FilterEngine,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(65, 55, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ Filter Stack Manager ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Min(3),    // list
                Constraint::Length(2), // actions
            ])
            .split(inner);

        if engine.rules.is_empty() {
            let empty_text = Paragraph::new(
                "No active filters.\n\nPress '+' to Include or '-' to Exclude.",
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
            empty_text.render(chunks[0], buf);
        } else {
            let items: Vec<ListItem> = engine
                .rules
                .iter()
                .enumerate()
                .map(|(idx, r)| {
                    let is_sel = idx == selected;
                    let check = if r.enabled { "[x]" } else { "[ ]" };
                    let (kind_str, kind_style) = match r.kind {
                        FilterKind::Include => ("INC", theme.level_info),
                        FilterKind::Exclude => ("EXC", theme.level_error),
                    };

                    let line = Line::from(vec![
                        Span::styled(format!("{} ", check), if r.enabled { Style::default().fg(Color::Green) } else { Style::default().fg(Color::DarkGray) }),
                        Span::styled(format!("{:<4}", kind_str), kind_style),
                        Span::styled(
                            format!("{:<30}", r.pattern),
                            if r.enabled { Style::default().fg(Color::White) } else { Style::default().fg(Color::DarkGray) },
                        ),
                        Span::styled(
                            if r.is_regex { "[regex] " } else { "" },
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::styled(
                            if r.is_case_sensitive { "[case] " } else { "" },
                            Style::default().fg(Color::LightBlue),
                        ),
                    ]);

                    let item_style = if is_sel {
                        theme.modal_selected
                    } else {
                        Style::default().bg(theme.modal_bg)
                    };

                    ListItem::new(line).style(item_style)
                })
                .collect();

            let list = List::new(items);
            list.render(chunks[0], buf);
        }

        let hint = Line::from(vec![
            Span::styled("[Space] Toggle", Style::default().fg(Color::Cyan)),
            Span::raw(" | "),
            Span::styled("[d/Del] Delete", Style::default().fg(Color::Red)),
            Span::raw(" | "),
            Span::styled("[+] Include", Style::default().fg(Color::Green)),
            Span::raw(" | "),
            Span::styled("[-] Exclude", Style::default().fg(Color::Yellow)),
            Span::raw(" | "),
            Span::styled("[x] Clear All", Style::default().fg(Color::Magenta)),
            Span::raw(" | "),
            Span::styled("[Esc] Close", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .render(chunks[1], buf);
    }

    fn render_field_selector(
        selected: usize,
        format_mgr: &FormatManager,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(55, 60, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(format!(" [ Fields / Columns - {} ] ", format_mgr.active_format().name()))
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1), // subtitle
                Constraint::Min(3),    // field list
                Constraint::Length(3), // shortcuts
            ])
            .split(inner);

        let total_avail = format_mgr.available_fields.len();
        let total_vis = format_mgr.visible_fields.len();
        let view_name = match format_mgr.view_mode {
            crate::format::ViewMode::CompactLine => "Compact Line View",
            crate::format::ViewMode::ColumnsTable => "Columns Table View",
        };

        let sub_line = Line::from(vec![
            Span::styled(format!("Fields: {}/{} selected", total_vis, total_avail), Style::default().fg(Color::Yellow)),
            Span::raw(" | "),
            Span::styled(format!("Current: {}", view_name), Style::default().fg(Color::Cyan)),
        ]);
        Paragraph::new(sub_line).render(chunks[0], buf);

        let visible_height = (chunks[1].height as usize).max(1);
        let scroll_offset = if selected >= visible_height {
            selected - visible_height + 1
        } else {
            0
        };

        let items: Vec<ListItem> = format_mgr
            .available_fields
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_height)
            .map(|(idx, field)| {
                let is_sel = idx == selected;
                let is_visible = format_mgr.visible_fields.contains(field);
                let check = if is_visible { "[*]" } else { "[ ]" };

                let line = Line::from(vec![
                    Span::styled(
                        format!(" {} ", check),
                        if is_visible {
                            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Span::styled(
                        field,
                        if is_visible {
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    ),
                ]);

                let item_style = if is_sel {
                    theme.modal_selected
                } else {
                    Style::default().bg(theme.modal_bg)
                };

                ListItem::new(line).style(item_style)
            })
            .collect();

        let list = List::new(items);
        list.render(chunks[1], buf);

        let hints = vec![
            Line::from(vec![
                Span::styled("[a] All", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled("[c] Clear", Style::default().fg(Color::Magenta)),
                Span::raw("  "),
                Span::styled("[r] Range", Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled("[Space] Toggle", Style::default().fg(Color::White)),
                Span::raw("  "),
                Span::styled("[s] Solo", Style::default().fg(Color::Green)),
                Span::raw("  "),
                Span::styled("[1-9] Col", Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::styled("[Tab] Table", Style::default().fg(Color::LightBlue)),
                Span::raw("  "),
                Span::styled("[Esc] Done", Style::default().fg(Color::Gray)),
            ]),
        ];
        Paragraph::new(hints)
            .alignment(Alignment::Center)
            .render(chunks[2], buf);
    }

    fn render_select_field_range(
        input: &str,
        format_mgr: &FormatManager,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(58, 44, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ Quick Column / Field Range Selection ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1), // prompt label
                Constraint::Length(3), // input box
                Constraint::Length(4), // preview / help
                Constraint::Length(1), // buttons
            ])
            .split(inner);

        Paragraph::new("Enter column numbers (e.g. 5-7, 5,6,7) or field names:")
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .render(chunks[0], buf);

        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(theme.modal_input_bg));
        Paragraph::new(format!("{}█", input))
            .block(input_block)
            .render(chunks[1], buf);

        // Preview parsed fields
        let parsed = crate::format::parse_field_selection_query(input, &format_mgr.available_fields);
        let preview_line = if !input.trim().is_empty() {
            if parsed.is_empty() {
                Line::from(vec![
                    Span::styled("No matching fields", Style::default().fg(Color::Red)),
                ])
            } else {
                Line::from(vec![
                    Span::styled("Selected columns: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::styled(parsed.join(" │ "), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                ])
            }
        } else {
            Line::from(vec![
                Span::styled("Examples: ", Style::default().fg(Color::Yellow)),
                Span::styled("5-7 ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::raw("(cols 5, 6, 7)  |  "),
                Span::styled("5,6,7 ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::raw(" |  "),
                Span::styled("1, 3, 5-7", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            ])
        };

        let help_lines = vec![
            preview_line,
            Line::from(vec![
                Span::styled("Format: ", Style::default().fg(Color::DarkGray)),
                Span::styled(format_mgr.active_format().name(), Style::default().fg(Color::Cyan)),
                Span::styled(format!(" ({} indexed fields/tokens)", format_mgr.available_fields.len()), Style::default().fg(Color::DarkGray)),
            ]),
        ];
        Paragraph::new(help_lines).render(chunks[2], buf);

        let buttons = Line::from(vec![
            Span::styled("[ Enter ] Apply & Show Table", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("        "),
            Span::styled("[ Esc ] Cancel", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(buttons)
            .alignment(Alignment::Center)
            .render(chunks[3], buf);
    }

    fn render_export(
        output_path: &str,
        format_idx: usize,
        scope_idx: usize,
        focused_field: usize,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(60, 45, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ Save / Export Extraction ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1), // path label
                Constraint::Length(3), // path input
                Constraint::Length(2), // scope
                Constraint::Length(2), // format
                Constraint::Length(2), // buttons
            ])
            .split(inner);

        // Path Label
        Paragraph::new("Destination file path:")
            .style(if focused_field == 0 { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) })
            .render(chunks[0], buf);

        // Path Input
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(if focused_field == 0 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) })
            .style(Style::default().bg(theme.modal_input_bg));
        let path_text = format!("{}█", output_path);
        Paragraph::new(path_text)
            .block(input_block)
            .render(chunks[1], buf);

        // Scope
        let scope_str = format!(
            "Scope [2] : {}  {}",
            if scope_idx == 0 { "(•) All filtered lines" } else { "( ) All filtered lines" },
            if scope_idx == 1 { "(•) Visible lines" } else { "( ) Visible lines" }
        );
        Paragraph::new(scope_str)
            .style(if focused_field == 1 { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) })
            .render(chunks[2], buf);

        // Format
        let fmt_str = format!(
            "Format [3] : {}  {}  {}",
            if format_idx == 0 { "(•) Raw Lines" } else { "( ) Raw Lines" },
            if format_idx == 1 { "(•) JSON Lines" } else { "( ) JSON Lines" },
            if format_idx == 2 { "(•) CSV" } else { "( ) CSV" }
        );
        Paragraph::new(fmt_str)
            .style(if focused_field == 2 { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Gray) })
            .render(chunks[3], buf);

        // Buttons
        let buttons = Line::from(vec![
            Span::styled("[ Enter ] Export", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("        "),
            Span::styled("[ Esc ] Cancel", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(buttons)
            .alignment(Alignment::Center)
            .render(chunks[4], buf);
    }

    fn render_search(query: &str, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let popup_area = Self::centered_rect(50, 22, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ Quick Search ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(inner);

        Paragraph::new("Text · /error or /10,3:error: before,after").render(chunks[0], buf);

        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(theme.modal_input_bg));
        Paragraph::new(format!("{}█", query))
            .block(input_block)
            .render(chunks[1], buf);

        let hint = Line::from(vec![
            Span::styled("[ Enter ] Filter · Tab: complete", Style::default().fg(Color::Green)),
            Span::raw("    "),
            Span::styled("[ Esc ] Cancel", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(hint).alignment(Alignment::Center).render(chunks[2], buf);
    }

    fn render_help(tab: usize, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let popup_area = Self::centered_rect(86, 90, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ Flik - User & Investigation Guide ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        if inner.width < 20 || inner.height < 6 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2), // Tab bar
                Constraint::Min(4),    // Content
                Constraint::Length(1), // Footer shortcuts
            ])
            .split(inner);

        // Render Tabs
        let tab_titles = [
            " 1: Navigation ",
            " 2: Filters & Syntax ",
            " 3: Analytics & Tools ",
            " 4: Panes & Session ",
            " 5: Keys & Mouse ",
        ];

        let mut tab_spans = Vec::new();
        for (i, title) in tab_titles.iter().enumerate() {
            let is_sel = i == tab;
            let style = if is_sel {
                theme.menu_selected
            } else {
                Style::default().bg(theme.modal_input_bg).fg(Color::Gray)
            };
            tab_spans.push(Span::styled(*title, style));
            tab_spans.push(Span::raw(" "));
        }

        Paragraph::new(Line::from(tab_spans))
            .alignment(Alignment::Center)
            .render(chunks[0], buf);

        // Tab Content
        let content: Vec<Line> = match tab {
            0 => vec![
                Line::from(Span::styled("LOG VIEWING & SCROLLING :", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
                Line::from("  ↑ / ↓  or  k / j   : Scroll line-by-line vertically"),
                Line::from("  Space / f / PgDn   : Page down  │  b / PgUp : Page up"),
                Line::from("  g / Home           : Jump to start of file (first log line)"),
                Line::from("  G / End            : Jump to end of file (automatically re-engages live tail)"),
                Line::from("  ← / →              : Horizontal scroll across wide log lines"),
                Line::from(""),
                Line::from(Span::styled("DISPLAY MODES & FORMATTING :", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                Line::from("  Tab  or  [4]       : Toggle view mode: Compact Raw Lines <-> Structured Columns Grid"),
                Line::from("  [5]  or  t         : Toggle live follow (tail -F). Pulsing indicator displays stream activity"),
                Line::from("  n                  : Toggle original line numbers in gutter"),
                Line::from("  c                  : Toggle ANSI coloration & log level highlighting"),
                Line::from("  T                  : Switch graphic theme: Dark Slate <-> Warm Sepia (eye-comfort)"),
                Line::from("  I                  : Reverse order: display newest incoming logs at top of view"),
                Line::from("  w                  : Toggle auto-indenting for embedded JSON strings (e.g. Kubernetes)"),
                Line::from("  r                  : Reload & re-index file immediately from disk"),
                Line::from(""),
                Line::from(Span::styled("BOOKMARKS & SELECTION :", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
                Line::from("  B  or  Space       : Toggle bookmark flag (*) on selected line"),
                Line::from("  ' (Quote)          : Jump to next bookmarked line (*)"),
                Line::from("  y                  : Copy (yank) selected line to clipboard (pbcopy / OSC 52 / xclip)"),
            ],
            1 => vec![
                Line::from(Span::styled("FILTER ENGINE BASICS :", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
                Line::from("  +  or  [3]  or  i  : Add INCLUDE filter (show only lines matching rule)"),
                Line::from("  -  or  [8]  or  e  : Add EXCLUDE filter (hide lines matching rule)"),
                Line::from("  Ctrl+F             : Open Filter Stack Manager (toggle, edit, delete rules)"),
                Line::from("  x  /  X            : Pop (undo) last filter rule  │  Clear all active filters"),
                Line::from(""),
                Line::from(Span::styled("INSIDE FILTER PROMPT :", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                Line::from("  Tab                : Toggle between Include (+) and Exclude (-) mode"),
                Line::from("  F2  or  r          : Toggle Regular Expression mode (Rust regex supported)"),
                Line::from("  F3  or  c          : Toggle Case Sensitivity (Exact Case vs Ignore Case)"),
                Line::from("  Ctrl+Space         : Auto-complete: suggests observed field keys and frequent values"),
                Line::from(""),
                Line::from(Span::styled("SMART EXPRESSION SYNTAX :", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
                Line::from("  amount > 100       : Numerical comparisons (>, >=, <, <=, ==, !=)"),
                Line::from("  latency_ms <= 50   : Numerical filters on JSON numbers, Apache bytes, response time"),
                Line::from("  status:5xx         : Match all HTTP 500-599 server error status codes"),
                Line::from("  status:404         : Match exact HTTP status code"),
                Line::from("  level:error        : Matches ERROR, FATAL, CRITICAL in JSON, Apache, Syslog, K8s"),
                Line::from("  service:auth       : Match 'service': 'auth' or service=auth"),
                Line::from("  \"exact phrase\"     : Quoted string search for punctuation and spaces"),
            ],
            2 => vec![
                Line::from(Span::styled("INVESTIGATION COMMANDS :", Style::default().fg(Color::Rgb(255, 120, 220)).add_modifier(Modifier::BOLD))),
                Line::from("  Ctrl+K             : Investigation Palette & History: jump, run actions, restore sessions"),
                Line::from("  Ctrl+G             : Context Mode: inspect surrounding N lines before/after error in source"),
                Line::from("  Ctrl+T             : Time Range: jump to timestamp or filter incidents by time window"),
                Line::from("  Ctrl+L             : Merged Chronology: merge multiple log files into a single timeline"),
                Line::from("  Ctrl+R             : Trace Correlator: trace Request IDs / UUIDs across all open files"),
                Line::from("  Ctrl+E             : Incident Export: export logs with automatic PII & credential redaction"),
                Line::from(""),
                Line::from(Span::styled("ANALYTICS & DIAGNOSTICS (KILLER FEATURES) :", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
                Line::from("  [S]  or  s         : Statistics Dashboard: HTTP status codes, log levels, top client IPs,"),
                Line::from("                       slowest requests, and error frequencies. Press [Enter] to filter!"),
                Line::from("  [H] (Shift+H)      : Timeline Histogram: visualize event frequency per hour or minute to"),
                Line::from("                       spot incident spikes. Use ←/→ to navigate, [Enter] to zoom window!"),
                Line::from("  [P]  or  p         : Pattern Clustering (Anomalies): abstracts dynamic variables (<*>) to"),
                Line::from("                       group logs into templates. Flags rare patterns (< 2%) as anomalies!"),
                Line::from("  [D]  or  d         : Side-by-Side Line Diff: press D on line 1, navigate to line 2, press D"),
                Line::from("                       again to view character-level differences side-by-side!"),
                Line::from("  v                  : Field Values Inspector (Facets): view cardinalities and unique values"),
                Line::from("                       for the active field, with instant drill-down filtering!"),
            ],
            3 => vec![
                Line::from(Span::styled("SPLIT PANES & WORKSPACES :", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                Line::from("  Ctrl+O             : Open file or live stream in active pane"),
                Line::from("  Ctrl+N             : Open second file side-by-side in split pane view"),
                Line::from("  Ctrl+]  or  F12    : Switch active focus between split panes"),
                Line::from("  Ctrl+Left / Right  : Focus left pane / Focus right pane"),
                Line::from("  Ctrl+W             : Close focused pane and return to single-pane view"),
                Line::from(""),
                Line::from(Span::styled("SESSION MANAGEMENT :", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
                Line::from("  Ctrl+P             : Save session: persist open files, filters, views, and positions"),
                Line::from("  Ctrl+K load        : Restore saved session from disk to resume investigation instantly"),
                Line::from(""),
                Line::from(Span::styled("MOUSE & RESIZING :", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
                Line::from("  Click in pane      : Instantly focus clicked pane"),
                Line::from("  Drag border        : Drag the split border to resize panes smoothly"),
            ],
            _ => vec![
                Line::from(Span::styled("FUNCTION BAR SHORTCUTS (Numbers 1-10 or F1-F10) :", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                Line::from("  [1] or ?           : Open this Help Guide"),
                Line::from("  [2] or o           : Auto-detect log format (JSON, Apache, K8s, Syslog, Raw)"),
                Line::from("  [3] or +           : Add Include filter"),
                Line::from("  [4] or Tab         : Toggle View (Compact Lines <-> Columns Table)"),
                Line::from("  [5] or t           : Toggle Live Tail Follow (tail -F)"),
                Line::from("  [6] or f           : Select Table Columns / Fields"),
                Line::from("  [7] or /           : Quick Search in view"),
                Line::from("  [8] or -           : Add Exclude filter"),
                Line::from("  [9] or m           : Open Top Menu Bar (Norton Commander style)"),
                Line::from("  [10] or QQ         : Quit Flik (press 'q' twice within 1.5s, or Shift+Q / F10)"),
                Line::from(""),
                Line::from(Span::styled("CROSS-PLATFORM MOUSE (macOS & Linux) :", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
                Line::from("  Left Click         : Focus pane, select row, or activate menu/function key"),
                Line::from("  Double Click Word  : Instantly copy the clicked word under cursor to clipboard"),
                Line::from("  Scroll Wheel       : Vertical scrolling with smooth deceleration"),
                Line::from("  Option (⌥) + Drag  : macOS native terminal selection (bypasses mouse capture)"),
                Line::from("  Shift + Drag       : Linux X11/Wayland native terminal text selection"),
            ],
        };

        Paragraph::new(content).render(chunks[1], buf);

        // Footer
        let footer = Line::from(vec![
            Span::styled("[Tab / ← → / 1-5] Switch Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("    │    "),
            Span::styled("[Esc / q / Enter] Close Guide", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(footer)
            .alignment(Alignment::Center)
            .render(chunks[2], buf);
    }

    fn render_message(title: &str, body: &str, is_error: bool, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let popup_area = Self::centered_rect(50, 25, area);
        Clear.render(popup_area, buf);

        let border_color = if is_error { Color::Red } else { Color::Green };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(format!(" [ {} ] ", title))
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(2), Constraint::Length(1)])
            .split(inner);

        Paragraph::new(body)
            .alignment(Alignment::Center)
            .render(chunks[0], buf);

        let ok = Span::styled("[ OK - Enter ]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        Paragraph::new(Line::from(ok))
            .alignment(Alignment::Center)
            .render(chunks[1], buf);
    }


    fn render_inspect_field_values(
        field_name: &str,
        results: &Option<crate::format::FieldInspectionResult>,
        selected_index: usize,
        scroll_offset: usize,
        format_mgr: &FormatManager,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(75, 75, area);
        Clear.render(popup_area, buf);

        let title = match results {
            Some(res) => format!(
                " [ Distinct Values: '{}' ({} unique / {} occurrences) ] ",
                res.field_name, res.unique_count, res.total_occurrences
            ),
            None => " [ Inspect Field Values ] ".to_string(),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(title)
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        match results {
            None => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(1), // Instruction
                        Constraint::Length(3), // Search/filter input box
                        Constraint::Min(6),    // List of fields
                        Constraint::Length(2), // Footer hints
                    ])
                    .split(inner);

                Paragraph::new("Select a field to analyze all distinct values, counts & percentages:")
                    .style(Style::default().fg(Color::Yellow))
                    .render(chunks[0], buf);

                let input_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan))
                    .title(" Quick Filter / Field Name ");
                let input_inner = input_block.inner(chunks[1]);
                input_block.render(chunks[1], buf);

                let display_field = if field_name.is_empty() {
                    Span::styled("Type to filter (or use ↑/↓ to choose a field below)...", Style::default().fg(Color::DarkGray))
                } else {
                    Span::styled(format!("{}█", field_name), Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
                };
                Paragraph::new(Line::from(display_field)).render(input_inner, buf);

                // Filter available fields
                let query = field_name.trim().to_lowercase();
                let mut filtered: Vec<String> = format_mgr
                    .available_fields
                    .iter()
                    .filter(|f| query.is_empty() || f.to_lowercase().contains(&query))
                    .cloned()
                    .collect();

                // If user typed something not in available fields, offer it as a custom entry
                if !query.is_empty() && !filtered.iter().any(|f| f.eq_ignore_ascii_case(&query)) {
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

                let visible_height = chunks[2].height as usize;
                let start_idx = if selected_index >= visible_height {
                    selected_index.saturating_sub(visible_height.saturating_sub(1))
                } else {
                    0
                };

                let items: Vec<ListItem> = filtered
                    .iter()
                    .enumerate()
                    .skip(start_idx)
                    .take(visible_height)
                    .map(|(idx, f)| {
                        let is_selected = idx == selected_index;
                        let line = if is_selected {
                            Line::from(vec![
                                Span::styled(" ▶ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                                Span::styled(f.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                                Span::styled("  [Press Enter to inspect]", Style::default().fg(Color::Green)),
                            ])
                        } else {
                            Line::from(vec![
                                Span::raw("   "),
                                Span::styled(f.clone(), Style::default().fg(Color::Cyan)),
                            ])
                        };

                        let item_style = if is_selected {
                            Style::default().bg(Color::Blue)
                        } else {
                            Style::default().bg(theme.modal_bg)
                        };

                        ListItem::new(line).style(item_style)
                    })
                    .collect();

                let list = List::new(items);
                list.render(chunks[2], buf);

                let footer = Line::from(vec![
                    Span::styled("[ Enter: Inspect Field ]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw("   "),
                    Span::styled("[ ↑/↓: Select ]", Style::default().fg(Color::Yellow)),
                    Span::raw("   "),
                    Span::styled("[ Type: Filter ]", Style::default().fg(Color::Cyan)),
                    Span::raw("   "),
                    Span::styled("[ Esc: Cancel ]", Style::default().fg(Color::Gray)),
                ]);
                Paragraph::new(footer).alignment(Alignment::Center).render(chunks[3], buf);
            }
            Some(res) => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(1), // Table Header
                        Constraint::Min(5),    // Table rows
                        Constraint::Length(2), // Controls
                    ])
                    .split(inner);

                let header = Line::from(vec![
                    Span::styled(format!("{:<6}", " #"), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{:<38}", "DISTINCT VALUE"), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{:>10}", "COUNT"), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{:>12}", "PERCENT"), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                ]);
                Paragraph::new(header).render(chunks[0], buf);

                let visible_height = chunks[1].height as usize;
                let items: Vec<ListItem> = res.values
                    .iter()
                    .enumerate()
                    .skip(scroll_offset)
                    .take(visible_height)
                    .map(|(idx, v)| {
                        let is_selected = idx == selected_index;
                        let prefix = if is_selected { "> " } else { "  " };
                        let style = if is_selected {
                            Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        };

                        let val_display = if UnicodeWidthStr::width(v.value.as_str()) > 36 {
                            format!("{}...", truncate_to_width(&v.value, 33))
                        } else {
                            v.value.clone()
                        };

                        let line = Line::from(vec![
                            Span::raw(prefix),
                            Span::styled(format!("{:<4}", idx + 1), if is_selected { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) }),
                            Span::styled(format!("{:<36}", val_display), style),
                            Span::styled(format!("{:>10}", v.count), if is_selected { Style::default().fg(Color::White) } else { Style::default().fg(Color::Green) }),
                            Span::styled(format!("{:>11.1}%", v.percentage), if is_selected { Style::default().fg(Color::White) } else { Style::default().fg(Color::Magenta) }),
                        ]);
                        ListItem::new(line)
                    })
                    .collect();

                List::new(items).render(chunks[1], buf);

                let footer = Line::from(vec![
                    Span::styled("[ Enter: Filter by this value ]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled("[ r: Change field ]", Style::default().fg(Color::Yellow)),
                    Span::raw("  "),
                    Span::styled("[ ↑/↓: Select ]", Style::default().fg(Color::Cyan)),
                    Span::raw("  "),
                    Span::styled("[ Esc: Close ]", Style::default().fg(Color::Gray)),
                ]);
                Paragraph::new(footer).alignment(Alignment::Center).render(chunks[2], buf);
            }
        }
    }

    fn render_stats_dashboard(
        stats: &crate::analytics::StatsSummary,
        selected_tab: usize,
        selected_item: usize,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(75, 65, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ 📊 Instant Statistics & Analytics Dashboard ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2), // Tabs
                Constraint::Min(8),    // Tab Content
                Constraint::Length(2), // Footer help
            ])
            .split(inner);

        // Render Tabs
        let tab_names = [if stats.status_total == 0 { "[1] Levels" } else { "[1] Status Codes" }, "[2] Top IPs", "[3] Endpoints", "[4] Top Errors", "[5] Latency"];
        let mut tab_spans = Vec::new();
        for (i, name) in tab_names.iter().enumerate() {
            if i > 0 {
                tab_spans.push(Span::raw(" │ "));
            }
            if i == selected_tab {
                tab_spans.push(Span::styled(
                    format!(" {} ", name),
                    Style::default().bg(Color::Rgb(30, 80, 160)).fg(Color::White).add_modifier(Modifier::BOLD),
                ));
            } else {
                tab_spans.push(Span::styled(
                    format!(" {} ", name),
                    Style::default().fg(Color::Rgb(150, 170, 200)),
                ));
            }
        }
        Paragraph::new(Line::from(tab_spans)).render(chunks[0], buf);

        // Tab content
        match selected_tab {
            0 if stats.status_total == 0 => {
                let total = stats.total_analyzed.max(1);
                let mut lines = vec![Line::raw(format!("Analyzed: {} lines (sample, up to 5,000)", stats.total_analyzed))];
                for (index, (level, count)) in stats.level_counts.iter().enumerate() {
                    let pct = *count as f64 * 100.0 / total as f64;
                    let color = if index == selected_item { Color::Yellow } else { Color::Cyan };
                    lines.push(Line::styled(format!("{} {:<12} {:>8} ({:5.1}%) {}",
                        if index == selected_item { ">" } else { " " }, level, count, pct,
                        "█".repeat((pct / 4.0).round() as usize)), Style::default().fg(color)));
                }
                if stats.level_counts.is_empty() {
                    lines.push(Line::raw("No severity levels detected in this sample."));
                }
                Paragraph::new(lines).render(chunks[1], buf);
            }
            0 => {
                // Status breakdown: 2xx, 3xx, 4xx, 5xx
                let status_total = stats.status_total.max(1);
                let mut lines = Vec::new();

                let g = stats.status_groups;
                let pct = |c: usize| (c as f64 / status_total as f64) * 100.0;
                let bar = |p: f64| {
                    let w = ((p / 100.0) * 24.0).round() as usize;
                    let filled = "█".repeat(w.min(24));
                    let empty = "░".repeat(24usize.saturating_sub(w));
                    format!("[{}{}]", filled, empty)
                };

                lines.push(Line::from(vec![
                    Span::styled("Analyzed: ", Style::default().fg(Color::Yellow)),
                    Span::styled(format!("{}", stats.total_analyzed), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                    Span::raw("  │  With status: "),
                    Span::styled(
                        format!("{}", stats.status_total),
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                ]));
                let mut code_summary = vec![Span::styled("Status codes:", Style::default().fg(Color::Cyan))];
                if stats.status_counts.is_empty() {
                    code_summary.push(Span::raw(" none found"));
                } else {
                    for (&code, &count) in &stats.status_counts {
                        let style = match code {
                            200..=299 => Style::default().fg(Color::Green),
                            300..=399 => Style::default().fg(Color::Cyan),
                            400..=499 => Style::default().fg(Color::Yellow),
                            _ => Style::default().fg(Color::Red),
                        };
                        let percentage = count as f64 / status_total as f64 * 100.0;
                        code_summary.push(Span::raw("  "));
                        code_summary.push(Span::styled(
                            format!("{}:{} ({:.0}%)", code, count, percentage),
                            style.add_modifier(Modifier::BOLD),
                        ));
                    }
                }
                lines.push(Line::from(code_summary));
                lines.push(Line::raw(""));

                lines.push(Line::from(vec![
                    Span::styled("  2xx Success   : ", Style::default().fg(Color::Rgb(60, 230, 130)).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{:>6} ({:>5.1}%) ", g[0], pct(g[0])), Style::default().fg(Color::White)),
                    Span::styled(bar(pct(g[0])), Style::default().fg(Color::Rgb(60, 230, 130))),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("  3xx Redirect  : ", Style::default().fg(Color::Rgb(100, 210, 255)).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{:>6} ({:>5.1}%) ", g[1], pct(g[1])), Style::default().fg(Color::White)),
                    Span::styled(bar(pct(g[1])), Style::default().fg(Color::Rgb(100, 210, 255))),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("  4xx Client Err: ", Style::default().fg(Color::Rgb(255, 205, 60)).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{:>6} ({:>5.1}%) ", g[2], pct(g[2])), Style::default().fg(Color::White)),
                    Span::styled(bar(pct(g[2])), Style::default().fg(Color::Rgb(255, 205, 60))),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("  5xx Server Err: ", Style::default().fg(Color::Rgb(255, 90, 90)).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{:>6} ({:>5.1}%) ", g[3], pct(g[3])), Style::default().fg(Color::White)),
                    Span::styled(bar(pct(g[3])), Style::default().fg(Color::Rgb(255, 90, 90))),
                ]));

                Paragraph::new(lines).render(chunks[1], buf);
            }
            1 => {
                // Top IPs
                let mut items = Vec::new();
                let total = stats.total_analyzed.max(1);
                for (idx, (ip, cnt)) in stats.top_ips.iter().enumerate() {
                    let is_sel = idx == selected_item;
                    let style = if is_sel {
                        Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let pct_val = (*cnt as f64 / total as f64) * 100.0;
                    let line = Line::from(vec![
                        Span::styled(format!("  {:>2}. ", idx + 1), Style::default().fg(Color::DarkGray)),
                        Span::styled(format!("{:<20}", ip), style),
                        Span::styled(format!("{:>8} reqs ", cnt), Style::default().fg(Color::Green)),
                        Span::styled(format!("({:>5.1}%)", pct_val), Style::default().fg(Color::Yellow)),
                    ]);
                    items.push(ListItem::new(line));
                }
                if items.is_empty() {
                    items.push(ListItem::new("No IP addresses detected in current sample."));
                }
                List::new(items).render(chunks[1], buf);
            }
            2 => {
                // Top Endpoints
                let mut items = Vec::new();
                let total = stats.total_analyzed.max(1);
                for (idx, (ep, cnt)) in stats.top_endpoints.iter().enumerate() {
                    let is_sel = idx == selected_item;
                    let style = if is_sel {
                        Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let pct_val = (*cnt as f64 / total as f64) * 100.0;
                    let ep_str = if UnicodeWidthStr::width(ep.as_str()) > 38 { truncate_to_width(ep, 38) } else { ep.as_str() };
                    let line = Line::from(vec![
                        Span::styled(format!("  {:>2}. ", idx + 1), Style::default().fg(Color::DarkGray)),
                        Span::styled(format!("{:<38}", ep_str), style),
                        Span::styled(format!("{:>8} reqs ", cnt), Style::default().fg(Color::Green)),
                        Span::styled(format!("({:>5.1}%)", pct_val), Style::default().fg(Color::Yellow)),
                    ]);
                    items.push(ListItem::new(line));
                }
                if items.is_empty() {
                    items.push(ListItem::new("No HTTP endpoints or paths detected."));
                }
                List::new(items).render(chunks[1], buf);
            }
            3 => {
                // Top Errors
                let mut items = Vec::new();
                for (idx, (err, cnt)) in stats.top_errors.iter().enumerate() {
                    let is_sel = idx == selected_item;
                    let style = if is_sel {
                        Style::default().bg(Color::Red).fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::LightRed)
                    };
                    let line = Line::from(vec![
                        Span::styled(format!("  {:>2}. ", idx + 1), Style::default().fg(Color::DarkGray)),
                        Span::styled(format!("[x{}] ", cnt), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled(err.clone(), style),
                    ]);
                    items.push(ListItem::new(line));
                }
                if items.is_empty() {
                    items.push(ListItem::new("🎉 No errors detected in current sample!"));
                }
                List::new(items).render(chunks[1], buf);
            }
            _ => {
                // Latency
                let mut lines = Vec::new();
                lines.push(Line::styled("Request Latency & Response Times:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
                lines.push(Line::raw(""));
                if let Some(avg) = stats.avg_latency_ms {
                    lines.push(Line::from(vec![
                        Span::raw("    • Average Latency : "),
                        Span::styled(format!("{:.2} ms", avg), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    ]));
                }
                if let Some(p95) = stats.p95_latency_ms {
                    lines.push(Line::from(vec![
                        Span::raw("    • 95th Percentile : "),
                        Span::styled(format!("{:.2} ms", p95), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    ]));
                }
                if stats.avg_latency_ms.is_none() {
                    lines.push(Line::raw("    No duration_ms or latency fields found in current log format."));
                }
                Paragraph::new(lines).render(chunks[1], buf);
            }
        }

        // Footer
        let footer = Line::from(vec![
            Span::styled("[Tab / 1-5] Switch Tab", Style::default().fg(Color::Cyan)),
            Span::raw("  │  "),
            Span::styled("[↑/↓] Select Item", Style::default().fg(Color::Yellow)),
            Span::raw("  │  "),
            Span::styled("[Enter] Filter by Selected", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("  │  "),
            Span::styled("[Esc] Close", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(footer).alignment(Alignment::Center).render(chunks[2], buf);
    }

    fn render_timeline_histogram(
        hist: &crate::analytics::TimelineHistogram,
        selected_bucket: usize,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(80, 55, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ ⏱️ Log Timeline & Error Distribution (Histogram) ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2), // Summary header
                Constraint::Min(8),    // ASCII Bar Chart
                Constraint::Length(3), // Selected bucket inspector
                Constraint::Length(1), // Key hints
            ])
            .split(inner);

        // Summary header
        let err_pct = if hist.total_scanned > 0 {
            (hist.total_errors as f64 / hist.total_scanned as f64) * 100.0
        } else {
            0.0
        };
        let summary = Line::from(vec![
            Span::styled(format!("Total Scanned: {} lines", hist.total_scanned), Style::default().fg(Color::White)),
            Span::raw("  │  "),
            Span::styled(format!("Errors: {} ({:.1}%)", hist.total_errors, err_pct), if hist.total_errors > 0 { Style::default().fg(Color::Red).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Green) }),
            Span::raw("  │  "),
            Span::styled(format!("Buckets: {}", hist.buckets.len()), Style::default().fg(Color::Cyan)),
        ]);
        Paragraph::new(summary).render(chunks[0], buf);

        // Render Bar Chart
        let chart_area = chunks[1];
        let bar_height = chart_area.height.saturating_sub(2) as usize;
        let num_buckets = hist.buckets.len();

        if num_buckets > 0 && bar_height > 0 {
            let col_width = (chart_area.width as usize / num_buckets).max(2).min(5);

            for (b_idx, b) in hist.buckets.iter().enumerate() {
                let x = chart_area.left() + (b_idx * col_width) as u16;
                if x + (col_width as u16) > chart_area.right() {
                    break;
                }

                let fraction = (b.total_count as f64 / hist.max_bucket_count as f64).min(1.0);
                let filled_rows = ((fraction * (bar_height as f64)).round() as usize).min(bar_height);
                let has_errors = b.error_count > 0;

                for row in 0..bar_height {
                    let y = chart_area.top() + (bar_height - 1 - row) as u16;
                    let style = if b_idx == selected_bucket {
                        if has_errors {
                            Style::default().bg(Color::Red).fg(Color::White).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD)
                        }
                    } else if row < filled_rows {
                        if has_errors {
                            Style::default().fg(Color::Red)
                        } else {
                            Style::default().fg(Color::Rgb(70, 160, 240))
                        }
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let ch = if row < filled_rows { "█" } else { " " };
                    for dx in 0..(col_width.saturating_sub(1) as u16) {
                        buf.set_string(x + dx, y, ch, style);
                    }
                }

                // Time label under bar
                let label_y = chart_area.top() + bar_height as u16;
                let short_lbl = if UnicodeWidthStr::width(b.label.as_str()) > col_width {
                    truncate_to_width(&b.label, col_width)
                } else {
                    &b.label
                };
                let lbl_style = if b_idx == selected_bucket {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                buf.set_string(x, label_y, short_lbl, lbl_style);
            }
        }

        // Selected bucket inspector panel
        if let Some(b) = hist.buckets.get(selected_bucket) {
            let inspector_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));
            let info = if b.total_count == 0 {
                Line::from(vec![
                    Span::styled(format!("Time: {} ", b.label), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw("│ No logs in this interval"),
                ])
            } else {
                let err_style = if b.error_count > 0 { Style::default().fg(Color::Red).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Green) };
                Line::from(vec![
                    Span::styled(format!("Time: {} ", b.label), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw("│ Volume: "),
                    Span::styled(format!("{} logs", b.total_count), Style::default().fg(Color::White)),
                    Span::raw(" │ Errors: "),
                    Span::styled(format!("{}", b.error_count), err_style),
                    Span::raw(" │ Target Line: "),
                    Span::styled(format!("#{}", b.first_line_idx + 1), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ])
            };
            Paragraph::new(info).block(inspector_block).render(chunks[2], buf);
        }

        // Key hints
        let hints = Line::from(vec![
            Span::styled("[←/→] Select Time Bucket", Style::default().fg(Color::Cyan)),
            Span::raw("  │  "),
            Span::styled("[Enter] Jump to Time in Log", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("  │  "),
            Span::styled("[Esc] Close", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(hints).alignment(Alignment::Center).render(chunks[3], buf);
    }

    fn render_pattern_clustering(
        clusters: &[crate::analytics::LogCluster],
        selected_index: usize,
        scroll_offset: usize,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(80, 65, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ 🧠 Log Pattern Clustering & Anomaly Detection ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2), // Header
                Constraint::Min(8),    // Clusters List
                Constraint::Length(2), // Footer hints
            ])
            .split(inner);

        let header = Line::from(vec![
            Span::styled(format!("{} unique log patterns", clusters.len()), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" detected in first 3000 lines (active filters ignored). Variables abstracted."),
        ]);
        Paragraph::new(header).render(chunks[0], buf);

        let visible_items = (chunks[1].height as usize / 2).max(1);
        let items: Vec<ListItem> = clusters
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_items)
            .map(|(idx, c)| {
                let is_sel = idx == selected_index;
                let bg_style = if is_sel {
                    Style::default().bg(Color::Rgb(20, 50, 110))
                } else {
                    Style::default().bg(theme.modal_bg)
                };

                let badge_style = if c.is_anomaly {
                    Style::default().bg(Color::Red).fg(Color::White).add_modifier(Modifier::BOLD)
                } else if c.percentage > 50.0 {
                    Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)
                };

                let badge_text = if c.is_anomaly {
                    format!(" [ANOMALY {:.2}% | x{}] ", c.percentage, c.count)
                } else {
                    format!(" [{:>5.1}% | x{}] ", c.percentage, c.count)
                };

                let line1 = Line::from(vec![
                    Span::styled(badge_text, badge_style),
                    Span::raw(" "),
                    Span::styled(c.pattern_template.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                ]);

                let sample_display = if UnicodeWidthStr::width(c.sample_line.as_str()) > 80 {
                    format!("{}...", truncate_to_width(&c.sample_line, 77))
                } else {
                    c.sample_line.clone()
                };
                let line2 = Line::from(vec![
                    Span::styled("   Sample: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(sample_display, Style::default().fg(Color::Gray)),
                ]);

                ListItem::new(vec![line1, line2]).style(bg_style)
            })
            .collect();

        List::new(items).render(chunks[1], buf);

        let footer = Line::from(vec![
            Span::styled("[↑/↓] Select Pattern", Style::default().fg(Color::Cyan)),
            Span::raw("  │  "),
            Span::styled("[Enter] Filter by Pattern", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("  │  "),
            Span::styled("[Esc] Close", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(footer).alignment(Alignment::Center).render(chunks[2], buf);
    }

    fn render_line_diff(
        line1_idx: usize,
        line1_text: &str,
        line2_idx: usize,
        line2_text: &str,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(85, 70, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ 🔍 Side-by-Side Line Comparison (Diff) ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Min(8),    // Split panels
                Constraint::Length(2), // Footer
            ])
            .split(inner);

        let panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(chunks[0]);

        // Panel 1
        let p1_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" Line #{} (Reference) ", line1_idx + 1));
        let p1_inner = p1_block.inner(panels[0]);
        p1_block.render(panels[0], buf);

        // Panel 2
        let p2_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(format!(" Line #{} (Compared) ", line2_idx + 1));
        let p2_inner = p2_block.inner(panels[1]);
        p2_block.render(panels[1], buf);

        let w1: Vec<&str> = line1_text.split_whitespace().collect();
        let w2: Vec<&str> = line2_text.split_whitespace().collect();

        let mut lines1 = Vec::new();
        let mut lines2 = Vec::new();

        let max_len = w1.len().max(w2.len());
        for i in 0..max_len {
            let token1 = w1.get(i).copied().unwrap_or("-");
            let token2 = w2.get(i).copied().unwrap_or("-");
            let is_diff = token1 != token2;

            let s1 = if is_diff {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let s2 = if is_diff {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            lines1.push(Line::styled(token1, s1));
            lines2.push(Line::styled(token2, s2));
        }

        Paragraph::new(lines1).render(p1_inner, buf);
        Paragraph::new(lines2).render(p2_inner, buf);

        let footer = Line::from(vec![
            Span::styled("[Esc] Close Diff", Style::default().fg(Color::Gray)),
            Span::raw("  │  "),
            Span::styled("Red/Green indicates differences between requests", Style::default().fg(Color::DarkGray)),
        ]);
        Paragraph::new(footer).alignment(Alignment::Center).render(chunks[1], buf);
    }

    fn render_trace_correlation(
        line_idx: usize,
        identifiers: &[(String, String)],
        selected_index: usize,
        area: Rect,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        let popup_area = Self::centered_rect(65, 45, area);
        Clear.render(popup_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.modal_border)
            .title(" [ 🔗 Trace & Identifier Correlation ] ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(theme.modal_bg).fg(theme.fg));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(2), // Header
                Constraint::Min(6),    // List of IDs
                Constraint::Length(2), // Footer
            ])
            .split(inner);

        let header = Line::from(vec![
            Span::raw("Identifiers discovered on Line #"),
            Span::styled(format!("{}:", line_idx + 1), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]);
        Paragraph::new(header).render(chunks[0], buf);

        let items: Vec<ListItem> = identifiers
            .iter()
            .enumerate()
            .map(|(idx, (k, v))| {
                let is_sel = idx == selected_index;
                let style = if is_sel {
                    Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let line = Line::from(vec![
                    Span::styled(format!("  • {:<16} : ", k), Style::default().fg(Color::Cyan)),
                    Span::styled(v.clone(), style),
                ]);
                ListItem::new(line)
            })
            .collect();

        if items.is_empty() {
            let empty = Line::styled("No correlation keys (trace_id, request_id, IP, UUID) found on this line.", Style::default().fg(Color::DarkGray));
            Paragraph::new(empty).render(chunks[1], buf);
        } else {
            List::new(items).render(chunks[1], buf);
        }

        let footer = Line::from(vec![
            Span::styled("[Enter] Filter all lines by this ID", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("  │  "),
            Span::styled("[↑/↓] Select", Style::default().fg(Color::Cyan)),
            Span::raw("  │  "),
            Span::styled("[Esc] Cancel", Style::default().fg(Color::Gray)),
        ]);
        Paragraph::new(footer).alignment(Alignment::Center).render(chunks[2], buf);
    }
}
