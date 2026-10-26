use flik::export::{ExportEngine, ExportFormat, ExportScope};
use flik::filter::{FilterEngine, FilterKind, FilterRule};
use flik::format::{FormatManager, LogFormat};
use flik::reader::LogReader;
use flik::ui::menu::MenuBar;
use flik::ui::modals::ActiveModal;
use flik::ui::theme::Theme;
use flik::ui::{FlikUi, UiState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_log_reader_basic() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "Line 1: system started").unwrap();
    writeln!(temp, "Line 2: loading config").unwrap();
    writeln!(temp, "Line 3: ready").unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    assert_eq!(reader.read_line(0), Some("Line 1: system started".into()));
    assert_eq!(reader.read_line(1), Some("Line 2: loading config".into()));
    assert_eq!(reader.read_line(2), Some("Line 3: ready".into()));
    assert_eq!(reader.read_line(3), None);

    let range = reader.read_range(0, 2);
    assert_eq!(range.len(), 2);
    assert_eq!(range[0].1, "Line 1: system started");
    assert_eq!(range[1].1, "Line 2: loading config");

    let lines = reader.read_lines(&[2, 0, 99]);
    assert_eq!(lines[0].as_deref(), Some("Line 3: ready"));
    assert_eq!(lines[1].as_deref(), Some("Line 1: system started"));
    assert_eq!(lines[2], None);
}

#[test]
fn test_filter_include_and_exclude() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "INFO: User 1 logged in").unwrap();
    writeln!(temp, "WARN: High CPU usage").unwrap();
    writeln!(temp, "ERROR: Database connection failed").unwrap();
    writeln!(temp, "INFO: User 2 logged in").unwrap();
    writeln!(temp, "ERROR: Payment timeout").unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let mut engine = FilterEngine::new();

    // 1. Add Include "ERROR"
    engine
        .add_rule("ERROR".into(), FilterKind::Include, false, false, &reader)
        .unwrap();

    // Wait a brief moment for filter worker
    std::thread::sleep(std::time::Duration::from_millis(100));

    assert_eq!(engine.filtered_count(&reader), 2);
    let orig_idx0 = engine.get_original_line_index(0, &reader).unwrap();
    let orig_idx1 = engine.get_original_line_index(1, &reader).unwrap();
    assert_eq!(orig_idx0, 2); // "ERROR: Database connection failed"
    assert_eq!(orig_idx1, 4); // "ERROR: Payment timeout"

    // 2. Add Exclude "Payment"
    engine
        .add_rule("Payment".into(), FilterKind::Exclude, false, false, &reader)
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine.filtered_count(&reader), 1);
    let rem_idx = engine.get_original_line_index(0, &reader).unwrap();
    assert_eq!(rem_idx, 2); // only Database error left
}

#[test]
fn test_format_detection_and_parsing() {
    let mut format_mgr = FormatManager::new();

    // Test JSON Log Detection
    let json_samples = vec![
        r#"{"time":"2026-10-16T10:00:00Z","level":"info","msg":"Server started","port":8080}"#.to_string(),
        r#"{"time":"2026-10-16T10:00:05Z","level":"error","status":500,"msg":"Database error"}"#.to_string(),
    ];
    format_mgr.detect_and_index_fields(&json_samples);
    assert_eq!(format_mgr.detected_format, LogFormat::Json);

    let parsed = format_mgr.parse_line(&json_samples[1]);
    assert_eq!(parsed.level, Some("ERROR".into()));
    assert_eq!(parsed.status, Some(500));
    assert_eq!(parsed.message, "Database error");
    assert_eq!(parsed.get_field_value("level"), Some("ERROR"));

    // Test Apache Combined Detection
    let apache_samples = vec![
        r#"192.168.1.1 - admin [16/Oct/2026:10:00:00 +0000] "GET /api/v1/health HTTP/1.1" 200 456 "https://example.test" "Mozilla/5.0""#.to_string(),
        r#"192.168.1.2 - - [16/Oct/2026:10:00:02 +0000] "POST /login HTTP/1.1" 401 128 "-" "curl/8.0""#.to_string(),
    ];
    format_mgr.detect_and_index_fields(&apache_samples);
    assert_eq!(format_mgr.detected_format, LogFormat::ApacheCombined);

    let parsed_apache = format_mgr.parse_line(&apache_samples[0]);
    assert_eq!(parsed_apache.status, Some(200));
    assert_eq!(parsed_apache.get_field_value("ip"), Some("192.168.1.1"));
    assert_eq!(parsed_apache.get_field_value("request"), Some("GET /api/v1/health HTTP/1.1"));
}

#[test]
fn test_export_engine() {
    let mut temp_log = NamedTempFile::new().unwrap();
    writeln!(temp_log, "Line A").unwrap();
    writeln!(temp_log, "Line B").unwrap();
    writeln!(temp_log, "Line C").unwrap();
    temp_log.flush().unwrap();

    let (reader, handle) = LogReader::open(temp_log.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let filter = FilterEngine::new();
    let format_mgr = FormatManager::new();

    let export_dir = tempfile::tempdir().unwrap();
    let export_path = export_dir.path().join("export.log");

    let count = ExportEngine::export_to_file(
        &export_path,
        &reader,
        &filter,
        &format_mgr,
        ExportScope::AllFiltered,
        ExportFormat::Raw,
    )
    .unwrap();

    assert_eq!(count, 3);
    let content = std::fs::read_to_string(&export_path).unwrap();
    assert!(content.contains("Line A\nLine B\nLine C"));
}

#[test]
fn test_smart_filters() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "{}", r#"{"time":"10:00","level":"info","service":"auth","status":200,"msg":"ok"}"#).unwrap();
    writeln!(temp, "{}", r#"{"time":"10:01","level":"warn","service":"auth","status":429,"msg":"rate limited"}"#).unwrap();
    writeln!(temp, "{}", r#"{"time":"10:02","level":"error","service":"payment","status":500,"msg":"db down"}"#).unwrap();
    writeln!(temp, "{}", r#"192.168.1.1 - - [16/Oct/2026:10:03:00] "GET /api HTTP/1.1" 503 120"#).unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    // 1. Test status:5xx (matches status 500 JSON and status 503 Apache)
    let mut engine = FilterEngine::new();
    engine.add_rule("status:5xx".into(), FilterKind::Include, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine.filtered_count(&reader), 2);

    // 2. Test level:error
    let mut engine2 = FilterEngine::new();
    engine2.add_rule("level:error".into(), FilterKind::Include, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine2.filtered_count(&reader), 1);

    // 3. Test key-value match service:auth
    let mut engine3 = FilterEngine::new();
    engine3.add_rule("service:auth".into(), FilterKind::Include, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine3.filtered_count(&reader), 2);
}

#[test]
fn test_kubernetes_embedded_json_filters() {
    let line = r#"2026-10-20T10:00:00.000000000Z stdout F {"time":"2026-10-20T10:00:00Z","level":"error","service":"payment-gateway","status":503,"msg":"database timeout","duration_ms":125.5}"#;

    let mut format = FormatManager::new();
    format.detect_and_index_fields(&[line.to_string()]);
    assert_eq!(format.detected_format, LogFormat::Kubernetes);
    assert!(format.available_fields.contains(&"service".to_string()));
    assert!(!format.available_fields.contains(&"log.service".to_string()));

    let record = format.parse_line(line);
    assert_eq!(record.get_field_value("service"), Some("payment-gateway"));
    assert_eq!(record.get_field_value("log.service"), Some("payment-gateway"));
    assert_eq!(record.get_field_value("msg"), Some("database timeout"));

    let service_filter = FilterRule::new(
        1,
        "service:payment-gateway".into(),
        FilterKind::Include,
        false,
        false,
    )
    .unwrap();
    assert!(service_filter.matches(line));

    let status_filter = FilterRule::new(
        2,
        "log.status:5xx".into(),
        FilterKind::Include,
        false,
        false,
    )
    .unwrap();
    assert!(status_filter.matches(line));

    let latency_filter = FilterRule::new(
        3,
        "log.duration_ms>100".into(),
        FilterKind::Include,
        false,
        false,
    )
    .unwrap();
    assert!(latency_filter.matches(line));

    let message_filter = FilterRule::new(
        4,
        "log.msg:database timeout".into(),
        FilterKind::Include,
        false,
        false,
    )
    .unwrap();
    assert!(message_filter.matches(line));
}

#[test]
fn test_kubernetes_selected_fields_render_in_compact_view() {
    let mut temp = NamedTempFile::new().unwrap();
    let line = r#"2026-10-20T10:00:00.000000000Z stdout F {"service":"auth-service","time":"2026-10-20T10:00:00Z","level":"error","status":503,"msg":"JWT token expired","duration_ms":20.67}"#;
    writeln!(temp, "{}", line).unwrap();
    temp.flush().unwrap();


    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let mut format = FormatManager::new();
    format.detect_and_index_fields(&[line.to_string()]);
    format.toggle_field_visibility("service");
    format.toggle_field_visibility("message");

    let mut filter = FilterEngine::new();
    filter.sync_field_filter(
        &format.visible_fields,
        &format.available_fields,
        format.is_json(),
        &reader,
    );
    let theme = Theme::default();
    let menu = MenuBar::new();
    let modal = ActiveModal::None;
    let ui_state = UiState::new();
    let area = Rect::new(0, 0, 120, 12);
    let mut buffer = Buffer::empty(area);

    FlikUi::render(
        area,
        &mut buffer,
        &theme,
        &reader,
        &filter,
        &format,
        &menu,
        &modal,
        &ui_state,
    );

    let rendered: String = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter_map(|position| buffer.cell(position).map(|cell| cell.symbol()))
        .collect();
    assert!(
        rendered.contains("service: auth-service"),
        "Rendered UI: {rendered:?}; format={:?}, fields={:?}",
        format.detected_format,
        format.visible_fields
    );

    format.toggle_field_visibility("message");
    let mut json_buffer = Buffer::empty(area);
    FlikUi::render(
        area,
        &mut json_buffer,
        &theme,
        &reader,
        &filter,
        &format,
        &menu,
        &modal,
        &ui_state,
    );
    let key_position = (0..area.height).find_map(|y| {
        let row: String = (0..area.width)
            .filter_map(|x| json_buffer.cell((x, y)).map(|cell| cell.symbol()))
            .collect();
        row.find("\"service\"").map(|x| (x as u16, y))
    });
    let (key_x, key_y) = key_position.expect("embedded JSON key should be visible");
    assert_eq!(
        json_buffer.cell((key_x, key_y)).unwrap().fg,
        Color::Rgb(110, 210, 255)
    );
}

#[test]
fn test_reverse_order_renders_newest_log_first() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "oldest event").unwrap();
    writeln!(temp, "newest event").unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let filter = FilterEngine::new();
    let format = FormatManager::new();
    let theme = Theme::default();
    let menu = MenuBar::new();
    let modal = ActiveModal::None;
    let mut ui_state = UiState::new();
    ui_state.reverse_order = true;
    let area = Rect::new(0, 0, 100, 10);
    let mut buffer = Buffer::empty(area);

    FlikUi::render(
        area,
        &mut buffer,
        &theme,
        &reader,
        &filter,
        &format,
        &menu,
        &modal,
        &ui_state,
    );

    let rows: Vec<String> = (0..area.height)
        .map(|y| {
            (0..area.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect()
        })
        .collect();
    let newest_row = rows.iter().position(|row| row.contains("newest event")).unwrap();
    let oldest_row = rows.iter().position(|row| row.contains("oldest event")).unwrap();
    assert!(newest_row < oldest_row);
}

#[test]
fn test_auto_indent_renders_every_kubernetes_json_message_in_flow() {
    let mut temp = NamedTempFile::new().unwrap();
    let first_line = r#"2026-10-20T10:00:00.000000000Z stdout F {"service":"auth-service","status":503}"#;
    let second_line = r#"2026-10-01T10:00:01.000000000Z stdout F {"service":"payment-gateway","status":200}"#;
    writeln!(temp, "{}", first_line).unwrap();
    writeln!(temp, "{}", second_line).unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let filter = FilterEngine::new();
    let mut format = FormatManager::new();
    format.detect_and_index_fields(&[first_line.to_string(), second_line.to_string()]);
    let theme = Theme::default();
    let menu = MenuBar::new();
    let modal = ActiveModal::None;
    let mut ui_state = UiState::new();
    ui_state.auto_indent_json = true;
    let area = Rect::new(0, 0, 120, 20);
    let mut buffer = Buffer::empty(area);

    FlikUi::render(
        area,
        &mut buffer,
        &theme,
        &reader,
        &filter,
        &format,
        &menu,
        &modal,
        &ui_state,
    );

    let rows: Vec<String> = (0..area.height)
        .map(|y| {
            (0..area.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect()
        })
        .collect();
    assert!(rows.iter().any(|row| row.contains("[STDOUT] F")));
    assert!(rows.iter().any(|row| row.contains("  \"service\": \"auth-service\",")));
    assert!(rows.iter().any(|row| row.contains("  \"status\": 503")));
    assert!(rows.iter().any(|row| row.contains("  \"service\": \"payment-gateway\",")));
    assert!(rows.iter().any(|row| row.contains("  \"status\": 200")));
}

#[test]
fn test_multiline_json_detection_and_field_inspection() {
    let sample = vec![
        "{".to_string(),
        r#"  "status": "success","#.to_string(),
        r#"  "count": 3,"#.to_string(),
        r#"  "data": ["#.to_string(),
        r#"    {"#.to_string(),
        r#"      "transaction_id": "txn_100000","#.to_string(),
        r#"      "account_id": "acc_137","#.to_string(),
        r#"      "merchant": "Uber","#.to_string(),
        r#"      "amount": 373.66"#.to_string(),
        r#"    },"#.to_string(),
        r#"    {"#.to_string(),
        r#"      "transaction_id": "txn_100001","#.to_string(),
        r#"      "account_id": "acc_102","#.to_string(),
        r#"      "merchant": "Spotify","#.to_string(),
        r#"      "amount": 104.14"#.to_string(),
        r#"    },"#.to_string(),
        r#"    {"#.to_string(),
        r#"      "transaction_id": "txn_100002","#.to_string(),
        r#"      "account_id": "acc_137","#.to_string(),
        r#"      "merchant": "Netflix","#.to_string(),
        r#"      "amount": 15.99"#.to_string(),
        r#"    }"#.to_string(),
        r#"  ]"#.to_string(),
        r#"}"#.to_string(),
    ];

    let mut fmt = FormatManager::new();
    fmt.detect_and_index_fields(&sample);

    // 1. Verify JSON is auto-detected
    assert_eq!(fmt.detected_format, LogFormat::Json);
    assert!(fmt.available_fields.contains(&"account_id".to_string()));
    assert!(fmt.available_fields.contains(&"merchant".to_string()));
    assert!(fmt.available_fields.contains(&"transaction_id".to_string()));

    // 3. Verify all available fields are selected by default at start
    assert_eq!(fmt.visible_fields, fmt.available_fields);
}

#[test]
fn test_field_filtering_shows_only_remaining_fields() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, r#"  "transaction_id": "txn_100","#).unwrap();
    writeln!(temp, r#"  "account_id": "acc_137","#).unwrap();
    writeln!(temp, r#"  "merchant": "Uber","#).unwrap();
    writeln!(temp, r#"  "amount": 25.50,"#).unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let mut fmt = FormatManager::new();
    let sample = reader.read_range(0, 10);
    let sample_lines: Vec<String> = sample.into_iter().map(|(_, l)| l).collect();
    fmt.detect_and_index_fields(&sample_lines);

    // Initial state: all fields selected
    assert_eq!(fmt.visible_fields.len(), 4);

    let mut filter = FilterEngine::new();
    filter.sync_field_filter(&fmt.visible_fields, &fmt.available_fields, fmt.is_json(), &reader);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(filter.filtered_count(&reader), 4);

    // User unchecks "merchant" and "amount" -> only "transaction_id" and "account_id" remain!
    fmt.toggle_field_visibility("merchant");
    fmt.toggle_field_visibility("amount");
    assert_eq!(fmt.visible_fields.len(), 2);
    assert!(!fmt.visible_fields.contains(&"merchant".to_string()));
    assert!(!fmt.visible_fields.contains(&"amount".to_string()));

    filter.sync_field_filter(&fmt.visible_fields, &fmt.available_fields, fmt.is_json(), &reader);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(filter.filtered_count(&reader), 2);

    // Solo "account_id": only 1 line remains!
    fmt.solo_field("account_id");
    assert_eq!(fmt.visible_fields, vec!["account_id".to_string()]);
    filter.sync_field_filter(&fmt.visible_fields, &fmt.available_fields, fmt.is_json(), &reader);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(filter.filtered_count(&reader), 1);
}

#[test]
fn test_numeric_comparison_filters() {
    let mut temp = NamedTempFile::new().unwrap();
    // 4 sample transaction lines
    writeln!(temp, r#"  "amount": 373.66,"#).unwrap();
    writeln!(temp, r#"  "amount": 104.14,"#).unwrap();
    writeln!(temp, r#"  "amount": 15.99,"#).unwrap();
    writeln!(temp, r#"  "amount": 89.00,"#).unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    // 1. Test "amount > 100" -> should match 373.66 and 104.14 (2 lines)
    let mut engine1 = FilterEngine::new();
    engine1.add_rule("amount > 100".into(), FilterKind::Include, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine1.filtered_count(&reader), 2);

    // 2. Test typo normalization: "amout > 100" -> automatically matches "amount > 100"
    let mut engine2 = FilterEngine::new();
    engine2.add_rule("amout > 100".into(), FilterKind::Include, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine2.filtered_count(&reader), 2);

    // 3. Test "amount < 50" -> matches 15.99 (1 line)
    let mut engine3 = FilterEngine::new();
    engine3.add_rule("amount < 50".into(), FilterKind::Include, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine3.filtered_count(&reader), 1);

    // 4. Test "amount >= 104.14" -> matches 373.66 and 104.14 (2 lines)
    let mut engine4 = FilterEngine::new();
    engine4.add_rule("amount >= 104.14".into(), FilterKind::Include, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine4.filtered_count(&reader), 2);

    // 5. Test NDJSON line with numeric comparison
    let mut temp_ndjson = NamedTempFile::new().unwrap();
    writeln!(temp_ndjson, r#"{{"id":"txn_1","amount":350.0,"status":200}}"#).unwrap();
    writeln!(temp_ndjson, r#"{{"id":"txn_2","amount":45.0,"status":500}}"#).unwrap();
    temp_ndjson.flush().unwrap();

    let (reader2, handle2) = LogReader::open(temp_ndjson.path()).unwrap();
    reader2.stop();
    let _ = handle2.join();

    let mut engine5 = FilterEngine::new();
    engine5.add_rule("amount > 100".into(), FilterKind::Include, false, false, &reader2).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(engine5.filtered_count(&reader2), 1);
}

#[test]
fn test_apache_all_fields_preserved_and_status_filtering() {
    let mut fmt = FormatManager::new();
    let apache_line = r#"192.168.1.10 - - [04/Oct/2026:10:16:52 +0200] "DELETE /api/v1/orders/checkout HTTP/1.1" 500 7806 "-" "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15""#;

    fmt.detect_and_index_fields(&[apache_line.to_string()]);
    assert_eq!(fmt.detected_format, LogFormat::ApacheCombined);

    // Verify all 8 fields available and selected by default
    assert_eq!(
        fmt.available_fields,
        vec!["ip", "user", "timestamp", "request", "status", "size", "referer", "user_agent"]
    );
    assert_eq!(fmt.visible_fields, fmt.available_fields);

    // Parse record and verify each field value
    let record = fmt.parse_line(apache_line);
    assert_eq!(record.get_field_value("ip"), Some("192.168.1.10"));
    assert_eq!(record.get_field_value("status"), Some("500"));
    assert_eq!(record.get_field_value("size"), Some("7806"));
    assert_eq!(record.get_field_value("referer"), Some("-"));
    assert!(record.get_field_value("user_agent").unwrap().contains("Mozilla/5.0"));
    assert_eq!(record.level, Some("ERROR".into()));
    assert_eq!(record.status, Some(500));
}

#[test]
fn test_filtering_error_case_insensitive() {
    let path = std::path::Path::new("../pizd/samples/error.log");
    if path.exists() {
        let (reader, handle) = LogReader::open(path).unwrap();
        // Wait for indexing to complete
        while reader.is_indexing.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        reader.stop();
        let _ = handle.join();

        println!("error.log total lines: {}", reader.line_count());
        let mut filter = FilterEngine::new();
        filter.add_rule("ERROR".into(), FilterKind::Include, false, false, &reader).unwrap();

        // Wait for filter to complete
        std::thread::sleep(std::time::Duration::from_millis(200));
        let count = filter.filtered_count(&reader);
        println!("Filtered count for 'ERROR': {}", count);
        assert!(count > 0, "Expected matches for 'ERROR' in error.log, got {}", count);

        // Check first 3 lines
        for i in 0..count.min(3) {
            let orig = filter.get_original_line_index(i, &reader).unwrap();
            let line = reader.read_line(orig).unwrap();
            println!("Line {}: {}", i, line);
            assert!(line.to_lowercase().contains("error"));
        }
    }
}

#[test]
fn test_pop_last_rule_undoes_only_last_filter() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "1. INFO User login").unwrap();
    writeln!(temp, "2. ERROR Database down").unwrap();
    writeln!(temp, "3. ERROR Payment timeout").unwrap();
    writeln!(temp, "4. WARN Disk high").unwrap();
    writeln!(temp, "5. ERROR User not found").unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let mut engine = FilterEngine::new();

    // Add 1st filter: Include ERROR -> matches lines 2, 3, 5 (3 lines)
    engine.add_rule("ERROR".into(), FilterKind::Include, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert_eq!(engine.filtered_count(&reader), 3);

    // Add 2nd filter: Exclude Payment -> matches lines 2, 5 (2 lines)
    engine.add_rule("Payment".into(), FilterKind::Exclude, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert_eq!(engine.filtered_count(&reader), 2);

    // Add 3rd filter: Exclude Database -> matches line 5 only (1 line)
    engine.add_rule("Database".into(), FilterKind::Exclude, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert_eq!(engine.filtered_count(&reader), 1);

    // Now test popping last filter (undo 'Database' filter)
    let popped = engine.pop_last_rule(&reader);
    assert!(popped.is_some());
    assert_eq!(popped.unwrap().pattern, "Database");
    std::thread::sleep(std::time::Duration::from_millis(80));
    // Should be back to 2 lines (rules 1 and 2 still active)
    assert_eq!(engine.filtered_count(&reader), 2);

    // Pop another filter (undo 'Payment' filter)
    let popped2 = engine.pop_last_rule(&reader);
    assert!(popped2.is_some());
    assert_eq!(popped2.unwrap().pattern, "Payment");
    std::thread::sleep(std::time::Duration::from_millis(80));
    // Should be back to 3 lines (rule 1 still active)
    assert_eq!(engine.filtered_count(&reader), 3);

    // Pop last remaining filter
    let popped3 = engine.pop_last_rule(&reader);
    assert!(popped3.is_some());
    assert_eq!(popped3.unwrap().pattern, "ERROR");
    std::thread::sleep(std::time::Duration::from_millis(80));
    // All lines back (5 lines)
    assert_eq!(engine.filtered_count(&reader), 5);

    // Popping on empty rules returns None
    assert!(engine.pop_last_rule(&reader).is_none());
}

#[test]
fn test_filter_quotes_stripping_and_smart_error_level() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, r#"10.0.0.1 - - [18/Oct/2026:10:00:00] "GET /orders HTTP/1.1" 200 123"#).unwrap();
    writeln!(temp, r#"10.0.0.2 - - [18/Oct/2026:10:01:00] "POST /checkout HTTP/1.1" 500 456"#).unwrap();
    writeln!(temp, r#"10.0.0.3 - - [18/Oct/2026:10:02:00] "GET /health HTTP/1.1" 200 789"#).unwrap();
    writeln!(temp, r#"10.0.0.4 - - [18/Oct/2026:10:02:30] "GET /missing HTTP/1.1" 404 120 "Mozilla/5.0 AppleWebKit/537.36""#).unwrap();
    writeln!(temp, r#"[Sun Oct 18 10:03:00] [php:error] script not found"#).unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let mut engine = FilterEngine::new();

    // User types `'ERROR"` with mismatched or matched quotes
    // Quotes must be stripped and it must match both HTTP 500 and [php:error] case-insensitively,
    // and MUST NOT match line 4 (404 WARN despite AppleWebKit/537.36)
    engine.add_rule("'ERROR\"".into(), FilterKind::Include, false, false, &reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));

    assert_eq!(engine.rules[0].pattern, "ERROR");
    // Only Line 2 (500) and Line 5 ([php:error]) must match (2 lines)
    assert_eq!(engine.filtered_count(&reader), 2);
}

#[test]
fn test_stats_summary_analytics() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, r#"192.168.1.1 - - [18/Oct/2026:10:00:00] "GET /api/v1/orders HTTP/1.1" 200 123"#).unwrap();
    writeln!(temp, r#"192.168.1.1 - - [18/Oct/2026:10:00:01] "GET /api/v1/orders HTTP/1.1" 200 125"#).unwrap();
    writeln!(temp, r#"192.168.1.2 - - [18/Oct/2026:10:00:02] "POST /api/v1/checkout HTTP/1.1" 500 456"#).unwrap();
    writeln!(temp, r#"10.0.0.5 - - [18/Oct/2026:10:00:03] "GET /api/v1/users HTTP/1.1" 404 89"#).unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let stats = flik::analytics::StatsSummary::compute(&reader, 100);
    assert_eq!(stats.total_analyzed, 4);
    assert_eq!(stats.status_groups[0], 2); // 2xx: 2
    assert_eq!(stats.status_groups[2], 1); // 4xx: 1
    assert_eq!(stats.status_groups[3], 1); // 5xx: 1

    // Top IP should be 192.168.1.1 with count 2
    assert!(!stats.top_ips.is_empty());
    assert_eq!(stats.top_ips[0].0, "192.168.1.1");
    assert_eq!(stats.top_ips[0].1, 2);

    // Top endpoint should be /api/v1/orders with count 2
    assert!(!stats.top_endpoints.is_empty());
    assert_eq!(stats.top_endpoints[0].0, "/api/v1/orders");
    assert_eq!(stats.top_endpoints[0].1, 2);
}

#[test]
fn test_kubernetes_stats_use_only_lines_with_numeric_status() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, r#"2026-10-20T10:00:00Z stdout F {{"service":"api","status":503}}"#).unwrap();
    writeln!(temp, r#"2026-10-20T10:00:01Z stdout F {{"service":"api","status":200}}"#).unwrap();
    writeln!(temp, "2026-10-20T10:00:02Z stdout F Started background worker").unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let stats = flik::analytics::StatsSummary::compute(&reader, 100);
    assert_eq!(stats.total_analyzed, 3);
    assert_eq!(stats.status_total, 2);
    assert_eq!(stats.status_groups[0], 1);
    assert_eq!(stats.status_groups[3], 1);

    let theme = Theme::default();
    let menu = MenuBar::new();
    let modal = ActiveModal::StatsDashboard {
        stats,
        selected_tab: 0,
        selected_item: 0,
    };
    let ui_state = UiState::new();
    let area = Rect::new(0, 0, 120, 30);
    let mut buffer = Buffer::empty(area);
    FlikUi::render(
        area,
        &mut buffer,
        &theme,
        &reader,
        &FilterEngine::new(),
        &FormatManager::new(),
        &menu,
        &modal,
        &ui_state,
    );
    let rendered: String = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter_map(|position| buffer.cell(position).map(|cell| cell.symbol()))
        .collect();
    assert!(rendered.contains("200:1 (50%)"));
    assert!(rendered.contains("503:1 (50%)"));
}

#[test]
fn test_kubernetes_top_errors_show_repeated_msg_values() {
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, r#"2026-10-20T10:00:00Z stdout F {{"level":"error","status":503,"msg":"JWT token expired"}}"#).unwrap();
    writeln!(temp, r#"2026-10-20T10:00:01Z stdout F {{"level":"error","status":503,"msg":"JWT token expired"}}"#).unwrap();
    writeln!(temp, r#"2026-10-20T10:00:02Z stdout F {{"level":"info","status":200,"msg":"healthy"}}"#).unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let stats = flik::analytics::StatsSummary::compute(&reader, 100);
    assert_eq!(stats.top_errors, vec![("JWT token expired".into(), 2)]);

    let theme = Theme::default();
    let menu = MenuBar::new();
    let modal = ActiveModal::StatsDashboard {
        stats,
        selected_tab: 3,
        selected_item: 0,
    };
    let ui_state = UiState::new();
    let area = Rect::new(0, 0, 120, 30);
    let mut buffer = Buffer::empty(area);
    FlikUi::render(
        area,
        &mut buffer,
        &theme,
        &reader,
        &FilterEngine::new(),
        &FormatManager::new(),
        &menu,
        &modal,
        &ui_state,
    );
    let rendered: String = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter_map(|position| buffer.cell(position).map(|cell| cell.symbol()))
        .collect();
    assert!(rendered.contains("JWT token expired"));
}

#[test]
fn test_timeline_histogram_analytics() {
    let mut temp = NamedTempFile::new().unwrap();
    for i in 0..10 {
        writeln!(temp, r#"2026-10-18T10:0{}:00Z [INFO] Processing request #{} ok"#, i, i).unwrap();
    }
    for i in 0..5 {
        writeln!(temp, r#"2026-10-18T10:1{}:00Z [ERROR] Failed connection #{} critical"#, i, i).unwrap();
    }
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let hist = flik::analytics::TimelineHistogram::compute(&reader, 5);
    assert_eq!(hist.total_scanned, 15);
    assert_eq!(hist.total_errors, 5);
    assert!(!hist.buckets.is_empty());
}

#[test]
fn test_timeline_histogram_uses_timestamp_density() {
    let mut temp = NamedTempFile::new().unwrap();
    for _ in 0..6 {
        writeln!(temp, "2026-10-20T10:00:00Z stdout F {{\"level\":\"info\"}}").unwrap();
    }
    writeln!(temp, "2026-10-20T10:00:30Z stdout F {{\"level\":\"info\"}}").unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let hist = flik::analytics::TimelineHistogram::compute(&reader, 4);
    let counts: Vec<usize> = hist.buckets.iter().map(|bucket| bucket.total_count).collect();
    assert_eq!(counts, vec![6, 0, 0, 1]);
    assert_eq!(hist.max_bucket_count, 6);
    assert_eq!(hist.buckets[3].first_line_idx, 6);
}

#[test]
fn test_pattern_clustering_and_anomaly() {
    let mut temp = NamedTempFile::new().unwrap();
    // 50 common lines
    for i in 0..50 {
        writeln!(temp, r#"10.0.0.1 - - [18/Oct/2026:10:00:{:02}] "GET /health HTTP/1.1" 200 {}"#, i % 60, 100 + i).unwrap();
    }
    // 1 rare anomaly line
    writeln!(temp, r#"192.168.99.99 - - [18/Oct/2026:10:05:00] "FATAL OutOfMemory crash in core worker" 500 0"#).unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    reader.stop();
    let _ = handle.join();

    let clusters = flik::analytics::PatternClusterer::cluster(&reader, 100);
    assert!(!clusters.is_empty());

    // Check abstraction
    let abstracted = flik::analytics::PatternClusterer::abstract_line(
        r#"192.168.1.1 - - [18/Oct/2026:10:00:00 +0000] "GET /api/v1/users/550e8400-e29b-41d4-a716-446655440000 HTTP/1.1" 200 450"#
    );
    assert!(abstracted.contains("<IP>"));
    assert!(abstracted.contains("<TIME>"));
    assert!(abstracted.contains("<UUID>"));
    assert!(abstracted.contains("<NUM>"));

    // Check anomaly detection
    let anomaly = clusters.iter().find(|c| c.is_anomaly);
    assert!(anomaly.is_some(), "Expected rare crash line to be flagged as an anomaly");
}

#[test]
fn test_trace_correlator_identifiers() {
    let json_line = r#"{"time":"2026-10-18T10:00:00Z","level":"info","trace_id":"trace-abc-12345","request_id":"req-999","account_id":"acc-42","client_ip":"192.168.1.50"}"#;
    let ids = flik::analytics::TraceCorrelator::extract_identifiers(json_line);

    assert!(ids.iter().any(|(k, v)| k == "trace_id" && v == "trace-abc-12345"));
    assert!(ids.iter().any(|(k, v)| k == "request_id" && v == "req-999"));
    assert!(ids.iter().any(|(k, v)| k == "account_id" && v == "acc-42"));

    // UUID fallback test
    let raw_line = r#"Error processing job for task 550e8400-e29b-41d4-a716-446655440000"#;
    let ids2 = flik::analytics::TraceCorrelator::extract_identifiers(raw_line);
    assert!(ids2.iter().any(|(k, v)| k == "uuid" && v == "550e8400-e29b-41d4-a716-446655440000"));
}

#[test]
fn test_bookmarks_and_line_pinning() {
    let mut ui_state = flik::ui::UiState::new();
    assert!(ui_state.bookmarks.is_empty());
    assert!(ui_state.diff_pinned_line.is_none());

    // Toggle bookmark
    ui_state.bookmarks.insert(42);
    assert!(ui_state.bookmarks.contains(&42));
    ui_state.bookmarks.remove(&42);
    assert!(!ui_state.bookmarks.contains(&42));

    // Pin line for diff
    ui_state.diff_pinned_line = Some(10);
    assert_eq!(ui_state.diff_pinned_line, Some(10));
    ui_state.diff_pinned_line = None;
    assert_eq!(ui_state.diff_pinned_line, None);
}

#[test]
fn test_live_activity_loupiote_detection() {
    use std::io::Write;
    let mut temp = NamedTempFile::new().unwrap();
    for i in 0..5 {
        writeln!(temp, "Initial line #{}", i).unwrap();
    }
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(reader.line_count(), 5);

    // Dynamically append new lines into the file
    let mut f = std::fs::OpenOptions::new().append(true).open(temp.path()).unwrap();
    for i in 0..3 {
        writeln!(f, "Live stream appended line #{}", i).unwrap();
    }
    f.flush().unwrap();

    // Background thread monitors at 100ms interval
    std::thread::sleep(std::time::Duration::from_millis(250));

    // Must have automatically indexed the 3 new lines
    assert_eq!(reader.line_count(), 8);
    // Loupiote should be ACTIVE (file is actively growing)
    assert!(reader.is_actively_growing(), "Expected reader.is_actively_growing() to be true");
    assert_eq!(reader.incoming_lines_count(), 3);

    // Reset incoming lines counter
    reader.reset_incoming_lines();
    assert_eq!(reader.incoming_lines_count(), 0);

    reader.stop();
    let _ = handle.join();
}

#[test]
fn test_tail_mode_with_filter_incremental_live_stream() {
    use std::io::Write;
    let mut temp = NamedTempFile::new().unwrap();
    writeln!(temp, "2026-10-19 10:00:01 INFO Server starting").unwrap();
    writeln!(temp, "2026-10-19 10:00:02 ERROR Port 8080 already in use").unwrap();
    writeln!(temp, "2026-10-19 10:00:03 INFO Retrying on port 8081").unwrap();
    writeln!(temp, "2026-10-19 10:00:04 ERROR Database connection timed out").unwrap();
    writeln!(temp, "2026-10-19 10:00:05 INFO Worker threads initialized").unwrap();
    temp.flush().unwrap();

    let (reader, handle) = LogReader::open(temp.path()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(reader.line_count(), 5);

    let mut engine = FilterEngine::new();
    // Add filter "ERROR"
    engine.add_rule("ERROR".into(), FilterKind::Include, false, false, &reader).unwrap();

    // Allow initial filter thread to complete
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert_eq!(engine.filtered_count(&reader), 2);
    assert_eq!(engine.get_original_line_index(0, &reader), Some(1));
    assert_eq!(engine.get_original_line_index(1, &reader), Some(3));

    // Now simulate live tail stream: append 4 more lines (2 ERRORs, 2 INFOs)
    let mut f = std::fs::OpenOptions::new().append(true).open(temp.path()).unwrap();
    writeln!(f, "2026-10-19 10:00:06 INFO Healthcheck passed").unwrap();
    writeln!(f, "2026-10-19 10:00:07 ERROR Disk quota exceeded for /tmp").unwrap();
    writeln!(f, "2026-10-19 10:00:08 INFO Garbage collection finished").unwrap();
    writeln!(f, "2026-10-19 10:00:09 ERROR Upstream gateway returned 502").unwrap();
    f.flush().unwrap();

    // Reader background thread detects new lines
    std::thread::sleep(std::time::Duration::from_millis(250));
    assert_eq!(reader.line_count(), 9);

    // Incremental stream processing matches the 2 new ERROR lines in real-time
    let matched_new = engine.process_incoming_lines(&reader);
    assert_eq!(matched_new, 2, "Expected exactly 2 new matching ERROR lines");
    assert_eq!(engine.filtered_count(&reader), 4);
    assert_eq!(engine.get_original_line_index(2, &reader), Some(6)); // Disk quota exceeded
    assert_eq!(engine.get_original_line_index(3, &reader), Some(8)); // Upstream gateway 502

    reader.stop();
    let _ = handle.join();
}

#[test]
fn test_app_stays_in_tail_when_adding_or_removing_filter() {
    let mut temp = NamedTempFile::new().unwrap();
    for i in 0..100 {
        if i % 10 == 0 {
            writeln!(temp, "Line #{}: ERROR something broke", i).unwrap();
        } else {
            writeln!(temp, "Line #{}: INFO regular log message", i).unwrap();
        }
    }
    temp.flush().unwrap();

    let mut app = flik::app::App::new(temp.path().to_path_buf()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Turn on follow mode (tail -F)
    app.reader.set_tail_enabled(true);
    app.ui_state.tail_following = true;
    assert!(app.reader.is_tail_enabled());
    assert!(app.ui_state.tail_following);

    // User adds filter "ERROR"
    app.filter.add_rule("ERROR".into(), FilterKind::Include, false, false, &app.reader).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));

    // Reset view after filter change
    app.reset_view_after_filter_change();

    // MUST remain in tail mode and follow tail!
    assert!(app.reader.is_tail_enabled(), "Tail mode must remain enabled!");
    assert!(app.ui_state.tail_following, "tail_following must remain true!");

    let filtered_total = app.filter.filtered_count(&app.reader);
    assert_eq!(filtered_total, 10);
    // Selected line must be at the TAIL (filtered_total - 1), NOT at line 0!
    assert_eq!(app.ui_state.selected_line, filtered_total - 1);

    // User removes the filter via pop_last_rule
    app.filter.pop_last_rule(&app.reader);
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.reset_view_after_filter_change();

    // MUST STILL remain in tail mode!
    assert!(app.reader.is_tail_enabled());
    assert!(app.ui_state.tail_following);
    assert_eq!(app.ui_state.selected_line, app.reader.line_count() - 1);
}

#[test]
fn test_apache_error_token_guessing_and_fields_5_6_7() {
    use flik::format::{parse_line_tokens, FormatManager, LogFormat};

    let line = "[Mon Oct 19 12:00:00.123456 2026] [php:error] [pid 12345] [client 192.168.1.10:45678] script '/var/www/dada/this_is_a_new_hello_world.php': Uncaught PDOException: connection refused";
    let tokens = parse_line_tokens(line);

    assert_eq!(tokens[0], "[Mon Oct 19 12:00:00.123456 2026]");
    assert_eq!(tokens[1], "[php:error]");
    assert_eq!(tokens[2], "[pid 12345]");
    assert_eq!(tokens[3], "[client 192.168.1.10:45678]");
    assert_eq!(tokens[4], "script");
    assert_eq!(tokens[5], "'/var/www/dada/this_is_a_new_hello_world.php':");
    assert_eq!(tokens[6], "Uncaught");
    assert_eq!(tokens[7], "PDOException:");

    let mut mgr = FormatManager::new();
    mgr.detect_and_index_fields(&[line.to_string()]);
    assert_eq!(mgr.detected_format, LogFormat::ApacheError);

    let record = mgr.parse_line(line);
    // Positional aliases
    assert_eq!(record.get_field_value("field_5"), Some("script"));
    assert_eq!(record.get_field_value("field_6"), Some("'/var/www/dada/this_is_a_new_hello_world.php':"));
    assert_eq!(record.get_field_value("field_7"), Some("Uncaught"));
    assert_eq!(record.get_field_value("$5"), Some("script"));
    assert_eq!(record.get_field_value("5"), Some("script"));
    assert_eq!(record.get_field_value("col_6"), Some("'/var/www/dada/this_is_a_new_hello_world.php':"));

    // Named fields
    assert_eq!(record.get_field_value("level"), Some("ERROR"));
    assert_eq!(record.get_field_value("module"), Some("php"));
    assert_eq!(record.get_field_value("pid"), Some("12345"));
    assert_eq!(record.get_field_value("client"), Some("192.168.1.10:45678"));
}

#[test]
fn test_field_selection_query_parsing_and_mapping() {
    use flik::format::parse_field_selection_query;

    let available_raw = vec![
        "field_1".to_string(),
        "field_2".to_string(),
        "field_3".to_string(),
        "field_4".to_string(),
        "field_5".to_string(),
        "field_6".to_string(),
        "field_7".to_string(),
    ];

    // Range "5-7"
    let q1 = parse_field_selection_query("5-7", &available_raw);
    assert_eq!(q1, vec!["field_5", "field_6", "field_7"]);

    // Comma-separated "5,6,7"
    let q2 = parse_field_selection_query("5,6,7", &available_raw);
    assert_eq!(q2, vec!["field_5", "field_6", "field_7"]);

    // JSON named fields: "3, 6" maps to 3rd and 6th field!
    let available_json = vec![
        "timestamp".to_string(),
        "level".to_string(),
        "service".to_string(),
        "status".to_string(),
        "msg".to_string(),
        "duration_ms".to_string(),
    ];

    let q3 = parse_field_selection_query("3, 6", &available_json);
    assert_eq!(q3, vec!["service", "duration_ms"]);

    let q4 = parse_field_selection_query("3-5", &available_json);
    assert_eq!(q4, vec!["service", "status", "msg"]);
}

#[test]
fn test_json_filtering_fields_3_and_6_excludes_empty_lines() {
    use std::io::Write;
    use tempfile::NamedTempFile;
    use flik::reader::LogReader;
    use flik::filter::FilterEngine;

    let mut temp = NamedTempFile::new().unwrap();
    // Line 0: has service and duration_ms
    writeln!(temp, r#"{{"timestamp": "2026-10-19T12:00:00Z", "level": "INFO", "service": "auth", "status": 200, "msg": "ok", "duration_ms": 12.5}}"#).unwrap();
    // Line 1: container brace in multiline
    writeln!(temp, r#"{{"#).unwrap();
    // Line 2: irrelevant line without service or duration_ms
    writeln!(temp, r#"{{"other_key": "some_value"}}"#).unwrap();
    // Line 3: has service
    writeln!(temp, r#"{{"service": "payment"}}"#).unwrap();
    // Line 4: closing brace
    writeln!(temp, r#"}}"#).unwrap();
    temp.flush().unwrap();

    let (reader, _handle) = LogReader::open(temp.path().to_path_buf()).unwrap();
    while reader.is_indexing.load(std::sync::atomic::Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let mut engine = FilterEngine::new();
    let available = vec![
        "timestamp".into(), "level".into(), "service".into(),
        "status".into(), "msg".into(), "duration_ms".into(),
    ];
    let visible = vec!["service".into(), "duration_ms".into()];

    engine.sync_field_filter(&visible, &available, true, &reader);
    while engine.is_filtering.load(std::sync::atomic::Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let count = engine.filtered_count(&reader);
    // Only line 0 and line 3 have service or duration_ms! Lines 1, 2, 4 are excluded!
    assert_eq!(count, 2);
    assert_eq!(engine.get_original_line_index(0, &reader), Some(0));
    assert_eq!(engine.get_original_line_index(1, &reader), Some(3));
}

#[test]
fn test_live_log_apache_error_display() {
    let path = std::path::PathBuf::from("/tmp/live.log");
    if path.exists() {
        let app = flik::app::App::new(path).unwrap();
        while app.reader.is_indexing.load(std::sync::atomic::Ordering::Relaxed)
            || app.filter.is_filtering.load(std::sync::atomic::Ordering::Relaxed)
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let total = app.filter.filtered_count(&app.reader);
        println!("Lines read: {}, Filtered lines: {}", app.reader.line_count(), total);
        assert!(total > 0, "Expected filtered_count > 0 for /tmp/live.log, got {}", total);

        // Test field range 5-7
        let selected = flik::format::parse_field_selection_query("5-7", &app.format.available_fields);
        assert_eq!(selected, vec!["field_5", "field_6", "field_7"]);

        // Parse line 0 and verify values
        let line0 = app.reader.read_line(0).unwrap();
        let rec = app.format.parse_line(&line0);
        assert_eq!(rec.get_field_value("field_5"), Some("script"));
        assert!(rec.get_field_value("field_6").unwrap().starts_with("'/var/www/"));
    }
}

#[test]
fn test_unicode_slice_never_panics_on_char_boundaries() {
    use flik::ui::{slice_by_columns, truncate_to_width, skip_columns};

    let sample = "field_1: [Mon Oct 19 12:04:52.708041 2026] │ other field: 123";

    // Test all possible column truncation widths
    for width in 0..=80 {
        let truncated = truncate_to_width(sample, width);
        assert!(truncated.len() <= sample.len());
    }

    // Test all possible scroll offsets
    for skip in 0..=80 {
        let skipped = skip_columns(sample, skip);
        assert!(skipped.len() <= sample.len());
    }

    // Test combined slice_by_columns for all skip and width combinations
    for skip in 0..=50 {
        for width in 0..=50 {
            let res = slice_by_columns(sample, skip, width);
            assert!(res.len() <= sample.len());
        }
    }
}

#[test]
fn test_nginx_error_log_detection_and_parsing() {
    use flik::format::{FormatManager, LogFormat};
    use flik::analytics::extract_timestamp;

    let lines = vec![
        r#"2026/10/26 11:00:23 [warn] 4547#0: *63068 upstream timed out (110: Connection timed out) while reading response header from upstream, client: 82.165.197.1, server: cdn.example.test, request: "GET /api/v1/users/profile HTTP/1.1", host: "cdn.example.test""#.to_string(),
        r#"2026/10/26 11:00:24 [error] 2824#0: *86037 connect() failed (111: Connection refused) while connecting to upstream, client: 10.0.4.25, server: api.example.test, request: "POST /api/v1/auth/token/refresh HTTP/1.1", upstream: "http://127.0.0.1:8080/api/v1/auth/token/refresh", host: "api.example.test""#.to_string(),
    ];

    let mut mgr = FormatManager::new();
    mgr.detect_and_index_fields(&lines);
    assert_eq!(mgr.detected_format, LogFormat::NginxError);

    let rec0 = mgr.parse_line(&lines[0]);
    assert_eq!(rec0.timestamp, Some("2026/10/26 11:00:23".to_string()));
    assert_eq!(rec0.level, Some("WARN".to_string()));
    assert_eq!(rec0.get_field_value("client"), Some("82.165.197.1"));
    assert_eq!(rec0.get_field_value("server"), Some("cdn.example.test"));
    assert_eq!(rec0.get_field_value("request"), Some("GET /api/v1/users/profile HTTP/1.1"));
    assert_eq!(rec0.get_field_value("host"), Some("cdn.example.test"));
    assert_eq!(rec0.get_field_value("cid"), Some("63068"));
    assert_eq!(rec0.get_field_value("pid"), Some("4547"));
    assert_eq!(
        rec0.message,
        "upstream timed out (110: Connection timed out) while reading response header from upstream"
    );

    let rec1 = mgr.parse_line(&lines[1]);
    assert_eq!(rec1.level, Some("ERROR".to_string()));
    assert_eq!(rec1.get_field_value("client"), Some("10.0.4.25"));
    assert_eq!(rec1.get_field_value("upstream"), Some("http://127.0.0.1:8080/api/v1/auth/token/refresh"));

    // Verify timestamp extraction in analytics
    let ts_opt = extract_timestamp(&lines[0]);
    assert!(ts_opt.is_some());
    let ts = ts_opt.unwrap();
    assert_eq!(ts.to_rfc3339(), "2026-10-26T11:00:23+00:00");
}

#[test]
fn test_nginx_access_log_detection() {
    use flik::format::{FormatManager, LogFormat};

    let lines = vec![
        r#"172.16.8.99 - john [26/Oct/2026:11:00:23 +0200] "GET /v1/payments/intent/verify HTTP/1.1" 200 12137 "https://example.test/app" "curl/8.4.0""#.to_string(),
        r#"10.0.8.44 - alice [26/Oct/2026:11:00:24 +0200] "GET /api/v1/orders/checkout HTTP/1.1" 200 1303 "https://example.test/app" "Mozilla/5.0""#.to_string(),
    ];

    let mut mgr = FormatManager::new();
    mgr.detect_and_index_fields(&lines);
    assert_eq!(mgr.detected_format, LogFormat::ApacheCombined);

    let rec = mgr.parse_line(&lines[0]);
    assert_eq!(rec.get_field_value("ip"), Some("172.16.8.99"));
    assert_eq!(rec.get_field_value("user"), Some("john"));
    assert_eq!(rec.status, Some(200));
}
