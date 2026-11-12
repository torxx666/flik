use std::io::Write;
use std::time::Duration;
use tempfile::NamedTempFile;
use flik::merged_stream::LiveMergedStream;
use flik::reader::LogReader;

#[test]
fn test_live_merged_stream_interleaving_and_live_tail() {
    let mut f1 = NamedTempFile::new().unwrap();
    let mut f2 = NamedTempFile::new().unwrap();

    // Write historical interleaved logs
    // File 1: 10:00:00 and 10:00:02
    writeln!(f1, "2026-11-06T10:00:00Z auth: login attempt user=alice").unwrap();
    writeln!(f1, "2026-11-06T10:00:02Z auth: login success user=alice").unwrap();
    f1.flush().unwrap();

    // File 2: 10:00:01 and 10:00:03
    writeln!(f2, "2026-11-06T10:00:01Z api: GET /profile 200").unwrap();
    writeln!(f2, "2026-11-06T10:00:03Z api: GET /orders 200").unwrap();
    f2.flush().unwrap();

    let p1 = f1.path().to_path_buf();
    let p2 = f2.path().to_path_buf();

    let mut merged = LiveMergedStream::start(&[p1.clone(), p2.clone()]).expect("Start merged stream");
    let (reader, handle) = LogReader::open(merged.temp_file.path()).unwrap();

    let start = std::time::Instant::now();
    while reader.line_count() < 4 {
        if start.elapsed() > Duration::from_secs(5) {
            panic!("timed out waiting for 4 historical lines, got {}", reader.line_count());
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let lines = reader.read_range(0, 4);
    assert_eq!(lines.len(), 4);

    let l1_name = p1.file_name().unwrap().to_string_lossy();
    let l2_name = p2.file_name().unwrap().to_string_lossy();

    // Chronological order: 10:00:00 (f1), 10:00:01 (f2), 10:00:02 (f1), 10:00:03 (f2)
    assert!(lines[0].1.contains(&format!("[{}]", l1_name)));
    assert!(lines[0].1.contains("10:00:00Z auth: login attempt"));

    assert!(lines[1].1.contains(&format!("[{}]", l2_name)));
    assert!(lines[1].1.contains("10:00:01Z api: GET /profile"));

    assert!(lines[2].1.contains(&format!("[{}]", l1_name)));
    assert!(lines[2].1.contains("10:00:02Z auth: login success"));

    assert!(lines[3].1.contains(&format!("[{}]", l2_name)));
    assert!(lines[3].1.contains("10:00:03Z api: GET /orders"));

    // 2. Test Live Appending: append new line to f2, then f1
    writeln!(f2, "2026-11-06T10:00:04Z api: POST /checkout 500").unwrap();
    f2.flush().unwrap();

    writeln!(f1, "2026-11-06T10:00:05Z auth: logout user=alice").unwrap();
    f1.flush().unwrap();

    let start_live = std::time::Instant::now();
    while reader.line_count() < 6 {
        if start_live.elapsed() > Duration::from_secs(5) {
            panic!("timed out waiting for live appended lines, got {}", reader.line_count());
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let live_lines = reader.read_range(4, 2);
    assert_eq!(live_lines.len(), 2);
    assert!(live_lines[0].1.contains(&format!("[{}]", l2_name)));
    assert!(live_lines[0].1.contains("10:00:04Z api: POST /checkout 500"));

    assert!(live_lines[1].1.contains(&format!("[{}]", l1_name)));
    assert!(live_lines[1].1.contains("10:00:05Z auth: logout user=alice"));

    reader.stop();
    let _ = handle.join();
    merged.stop();
}

#[test]
fn test_source_prefix_extraction_and_format_parsing() {
    use flik::format::{extract_source_label, FormatManager, LogFormat};

    // 1. Apache log with source prefix
    let apache_line = r#"[auth.log] 127.0.0.1 - alice [06/Nov/2026:10:00:00 +0000] "GET /login HTTP/1.1" 200 1234"#;
    let (src, remainder) = extract_source_label(apache_line);
    assert_eq!(src, Some("auth.log"));
    assert_eq!(remainder, r#"127.0.0.1 - alice [06/Nov/2026:10:00:00 +0000] "GET /login HTTP/1.1" 200 1234"#);

    let mut format_mgr = FormatManager::new();
    format_mgr.detect_and_index_fields(&[apache_line.to_string()]);
    assert_eq!(format_mgr.active_format(), LogFormat::ApacheCombined);

    let record = format_mgr.parse_line(apache_line);
    assert_eq!(record.source.as_deref(), Some("auth.log"));
    assert_eq!(record.get_field_value("source"), Some("auth.log"));
    assert_eq!(record.get_field_value("ip"), Some("127.0.0.1"));
    assert_eq!(record.get_field_value("user"), Some("alice"));
    assert_eq!(record.status, Some(200));

    // 2. JSON log with source prefix
    let json_line = r#"[api.log] {"timestamp": "2026-11-06T10:00:00Z", "level": "INFO", "user": {"id": 42}, "msg": "hello"}"#;
    let (src_json, remainder_json) = extract_source_label(json_line);
    assert_eq!(src_json, Some("api.log"));
    assert_eq!(remainder_json, r#"{"timestamp": "2026-11-06T10:00:00Z", "level": "INFO", "user": {"id": 42}, "msg": "hello"}"#);

    let mut json_format_mgr = FormatManager::new();
    json_format_mgr.detect_and_index_fields(&[json_line.to_string()]);
    assert_eq!(json_format_mgr.active_format(), LogFormat::Json);

    let json_rec = json_format_mgr.parse_line(json_line);
    assert_eq!(json_rec.source.as_deref(), Some("api.log"));
    assert_eq!(json_rec.get_field_value("source"), Some("api.log"));
    assert_eq!(json_rec.get_field_value("user.id"), Some("42"));
    assert_eq!(json_rec.level.as_deref(), Some("INFO"));

    // 3. Normal bracketed timestamp should NOT be treated as source label
    let normal_ts = "[2026-11-06 10:00:00] [INFO] system booted";
    let (no_src, _) = extract_source_label(normal_ts);
    assert_eq!(no_src, None);

    // 4. Mixed format stream: Active format is Apache, but line is from auth.log (Syslog)
    let auth_line = "[auth.log] Nov 06 11:08:35 bastion-prod-01 sshd[38351]: Failed password for invalid user scanner from 192.168.1.42 port 38610 ssh2";
    let auth_rec = format_mgr.parse_line(auth_line);
    assert_eq!(auth_rec.source.as_deref(), Some("auth.log"));
    assert_eq!(auth_rec.format, LogFormat::Syslog);
    assert_eq!(auth_rec.get_field_value("host"), Some("bastion-prod-01"));
    assert_eq!(auth_rec.get_field_value("service"), Some("sshd"));
    assert_eq!(auth_rec.get_field_value("user"), Some("scanner"));
    assert_eq!(auth_rec.get_field_value("ip"), Some("192.168.1.42"));
    assert!(!auth_rec.message.is_empty());
}

#[test]
fn test_tail_toggle_with_azerty_and_keys() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use flik::app::App;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "line 1").unwrap();
    writeln!(temp, "line 2").unwrap();
    temp.flush().unwrap();

    let mut app = App::new(temp.path().to_path_buf()).unwrap();

    // App::new starts with tail enabled
    assert!(app.reader.is_tail_enabled());
    assert!(app.ui_state.tail_following);

    // 1. Test toggling OFF with AZERTY Mac physical '5' key (which produces '(')
    let key_paren = KeyEvent {
        code: KeyCode::Char('('),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(key_paren, 24);
    assert!(!app.reader.is_tail_enabled(), "Pressing '(' (AZERTY key 5) should toggle tail OFF");
    assert!(!app.ui_state.tail_following);

    // 2. Test toggling ON with '('
    app.handle_key(key_paren, 24);
    assert!(app.reader.is_tail_enabled(), "Pressing '(' should toggle tail ON");
    assert!(app.ui_state.tail_following);

    // 3. Test toggling OFF with '5' (Shift+5 or numeric keypad)
    let key_5 = KeyEvent {
        code: KeyCode::Char('5'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(key_5, 24);
    assert!(!app.reader.is_tail_enabled(), "Pressing '5' should toggle tail OFF");
    assert!(!app.ui_state.tail_following);

    // 4. Test toggling ON with 't'
    let key_t = KeyEvent {
        code: KeyCode::Char('t'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(key_t, 24);
    assert!(app.reader.is_tail_enabled(), "Pressing 't' should toggle tail ON");
    assert!(app.ui_state.tail_following);

    // 5. Test toggling OFF with F5
    let key_f5 = KeyEvent {
        code: KeyCode::F(5),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    app.handle_key(key_f5, 24);
    assert!(!app.reader.is_tail_enabled(), "Pressing F5 should toggle tail OFF");
    assert!(!app.ui_state.tail_following);

    // 6. Test that in reverse_order (Newest first), incoming lines DO NOT shift view when tail is OFF
    app.ui_state.reverse_order = true;
    app.ui_state.scroll_row = 0;
    app.ui_state.selected_line = 0;
    app.last_filtered_count = app.filter.filtered_count(&app.reader);
    let original_top_before = flik::ui::source_display_index(
        app.ui_state.scroll_row,
        app.filter.filtered_count(&app.reader),
        true,
    );

    // Write 3 new lines while tail is OFF
    writeln!(temp, "line 3").unwrap();
    writeln!(temp, "line 4").unwrap();
    writeln!(temp, "line 5").unwrap();
    temp.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));

    // Call tick to process new lines
    app.tick(24);

    // Total lines should have grown from 2 to 5
    assert_eq!(app.filter.filtered_count(&app.reader), 5);
    // In reverse order, scroll_row must have adjusted by 3 lines to keep the view completely static
    assert_eq!(app.ui_state.scroll_row, 3);
    let original_top_after = flik::ui::source_display_index(
        app.ui_state.scroll_row,
        app.filter.filtered_count(&app.reader),
        true,
    );
    assert_eq!(
        original_top_before, original_top_after,
        "The line at the top of the screen must not change when tail is OFF in reverse order!"
    );

    // Re-enabling tail should jump back to top (scroll_row = 0)
    app.handle_key(key_paren, 24);
    assert!(app.reader.is_tail_enabled());
    assert_eq!(app.ui_state.scroll_row, 0);
    assert_eq!(app.ui_state.selected_line, 0);
}

#[test]
fn test_long_split_3page_view_mode() {
    use flik::app::App;
    use flik::format::ViewMode;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut temp = NamedTempFile::new().unwrap();
    // Write 30 lines
    for i in 1..=30 {
        writeln!(temp, "Log message line {:02}", i).unwrap();
    }
    temp.flush().unwrap();

    let mut app = App::new(temp.path().to_path_buf()).unwrap();
    assert_eq!(app.format.view_mode, ViewMode::CompactLine);

    // 1. Test cycling view mode with Tab / 4
    let mode = app.format.toggle_view_mode();
    assert_eq!(mode, ViewMode::ColumnsTable);
    let mode = app.format.toggle_view_mode();
    assert_eq!(mode, ViewMode::LongSplit);
    let mode = app.format.toggle_view_mode();
    assert_eq!(mode, ViewMode::CompactLine);

    // Set to LongSplit
    app.format.view_mode = ViewMode::LongSplit;

    // Set scroll_row to line 20
    app.ui_state.scroll_row = 20;

    let area = Rect::new(0, 0, 90, 10);
    let mut buf = Buffer::empty(area);

    flik::ui::FlikUi::render(
        area,
        &mut buf,
        &app.theme,
        &app.reader,
        &app.filter,
        &app.format,
        &app.menu,
        &app.modal,
        &app.ui_state,
    );

    // Check buffer contains the headers "p-2", "p-1", "Current"
    let mut rendered_text = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            rendered_text.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        rendered_text.push('\n');
    }

    assert!(rendered_text.contains("p-2"), "Rendered buffer must contain p-2 column header");
    assert!(rendered_text.contains("p-1"), "Rendered buffer must contain p-1 column header");
    assert!(rendered_text.contains("Current"), "Rendered buffer must contain Current column header");
    assert!(rendered_text.contains("LONG-SPLIT"), "Title bar must indicate LONG-SPLIT mode");

    // 2. Verify line progression and flow across the 3 columns:
    // When following tail on 30 lines with height 10 (page_size = 5 after title & borders):
    app.ui_state.tail_following = true;
    let mut live_buf = Buffer::empty(area);
    flik::ui::FlikUi::render(
        area,
        &mut live_buf,
        &app.theme,
        &app.reader,
        &app.filter,
        &app.format,
        &app.menu,
        &app.modal,
        &app.ui_state,
    );
    let mut live_text = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            live_text.push_str(live_buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        live_text.push('\n');
    }

    // Right column must have the latest lines (e.g. line 30)
    assert!(live_text.contains("line 30"), "Current page column must contain newest line 30");
}

