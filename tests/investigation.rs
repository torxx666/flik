use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use flik::{
    app::App,
    filter::{FilterKind, FilterRule},
    investigation::{self, Redactor, Source},
    session::{SavedPane, Session},
    ui::modals::ActiveModal,
    workspace::Workspace,
};
use std::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};
fn wait(mut done: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < until, "operation timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn ready(ws: &mut Workspace) {
    wait(|| {
        ws.panes.iter().all(|a| {
            !a.reader.is_indexing.load(Ordering::Acquire)
                && !a.filter.is_filtering.load(Ordering::Acquire)
        })
    });
}
fn job(ws: &mut Workspace) {
    wait(|| {
        ws.poll_job();
        !ws.is_investigating()
    });
}
fn rule(pattern: &str) -> FilterRule {
    FilterRule::new(0, pattern.into(), FilterKind::Include, false, false).unwrap()
}
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn fixture() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("api.log");
    let second = dir.path().join("worker.log");
    std::fs::write(&first,concat!(
        "{\"timestamp\":\"2026-10-21T10:00:00Z\",\"level\":\"info\",\"msg\":\"before\"}\n",
        "{\"timestamp\":\"2026-10-21T10:01:00Z\",\"level\":\"error\",\"trace_id\":\"req-42\",\"password\":\"secret-one\",\"msg\":\"failed\"}\n",
        "{\"timestamp\":\"2026-10-21T10:02:00Z\",\"level\":\"info\",\"msg\":\"after\"}\n")).unwrap();
    std::fs::write(&second,concat!(
        "{\"timestamp\":\"2026-10-21T12:01:30+02:00\",\"level\":\"error\",\"trace_id\":\"req-42\",\"token\":\"secret-two\",\"msg\":\"retry\"}\n",
        "undated event\n")).unwrap();
    let mut ws = Workspace::new(App::new(first).unwrap());
    ws.open_other(&second).unwrap();
    ws.active = 0;
    ready(&mut ws);
    (dir, ws)
}
#[test]
fn boolean_grammar_precedence_groups_quotes_and_errors() {
    let filter = rule("(level:error OR status>=500) AND NOT path:/health");
    assert!(filter.matches(r#"{"level":"error","status":400,"path":"/pay"}"#));
    assert!(filter.matches(r#"{"level":"info","status":503,"path":"/pay"}"#));
    assert!(!filter.matches(r#"{"level":"error","status":503,"path":"/health"}"#));
    assert!(rule("apple OR banana AND cherry").matches("apple"));
    assert!(!rule("apple OR banana AND cherry").matches("banana"));
    assert!(rule("NOT NOT apple").matches("apple"));
    assert!(rule("\"apple OR banana\" AND cherry").matches("apple OR banana cherry"));
    assert!(!rule("\"apple OR banana\" AND cherry").matches("apple cherry"));
    assert!(
        rule("service:\"payment gateway\" AND level:error")
            .matches(r#"{"service":"payment gateway","level":"error"}"#)
    );
    for invalid in [
        "apple AND",
        "OR apple",
        "(apple OR banana",
        "apple OR (banana AND)",
        "apple OR \"unclosed",
    ] {
        assert!(
            FilterRule::new(0, invalid.into(), FilterKind::Include, false, false).is_err(),
            "{invalid}"
        );
    }
    let regex = FilterRule::new(0, "(foo|bar)".into(), FilterKind::Include, true, false).unwrap();
    assert!(regex.matches("bar"));
}
#[test]
fn time_filters_use_offsets_and_inclusive_boundaries() {
    let filter = rule("@time:2026-10-21T10:01:00Z..2026-10-21T10:02:00Z");
    assert!(filter.matches("2026-10-21T12:01:00+02:00 at start"));
    assert!(filter.matches("2026-10-21T10:02:00Z at end"));
    assert!(!filter.matches("2026-10-21T10:02:00.001Z too late"));
    assert!(!filter.matches("missing timestamp"));
    assert!(
        FilterRule::new(
            0,
            "@time:2026-10-22T10:00:00Z..2026-10-21T10:00:00Z".into(),
            FilterKind::Include,
            false,
            false
        )
        .is_err()
    );
}
#[test]
fn context_recovers_hidden_neighbors_and_navigates_matches() {
    let (_dir, mut ws) = fixture();
    let app = &mut ws.panes[0];
    app.filter
        .add_rule(
            "error".into(),
            FilterKind::Include,
            false,
            false,
            &app.reader,
        )
        .unwrap();
    ready(&mut ws);
    ws.execute_command("context 1").unwrap();
    let rows = ws.investigation_rows().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter().map(|r| r.matched).collect::<Vec<_>>(),
        vec![false, true, false]
    );
    ws.handle_key(key(KeyCode::Home), 30);
    ws.handle_key(key(KeyCode::Enter), 30); // reveal a hidden neighbor, as described by the view's hint
    assert!(ws.investigation_rows().is_none());
    assert!(!ws.panes[0].filter.has_active_filters());
    assert_eq!(ws.panes[0].ui_state.selected_line, 0);
}
#[test]
fn time_range_jump_and_around_work_in_active_pane() {
    let (_dir, mut ws) = fixture();
    ws.execute_command("time 2026-10-21T10:01:00Z..2026-10-21T10:02:00Z")
        .unwrap();
    ready(&mut ws);
    assert_eq!(ws.panes[0].filter.filtered_count(&ws.panes[0].reader), 2);
    assert!(
        !ws.panes[1]
            .filter
            .rules
            .iter()
            .any(|r| r.pattern.starts_with("@time:"))
    );
    ws.execute_command("jump 2026-10-21T10:02:00Z").unwrap();
    job(&mut ws);
    assert_eq!(ws.panes[0].ui_state.selected_line, 1);
    ws.execute_command("around 0").unwrap();
    ready(&mut ws);
    assert_eq!(ws.panes[0].filter.filtered_count(&ws.panes[0].reader), 1);
    ws.execute_command("untime").unwrap();
    ready(&mut ws);
    assert_eq!(ws.panes[0].filter.filtered_count(&ws.panes[0].reader), 3);
}
#[test]
fn merge_orders_both_sources_and_trace_keeps_provenance() {
    let (_dir, mut ws) = fixture();
    ws.execute_command("merge").unwrap();
    job(&mut ws);
    let rows = ws.investigation_rows().unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(
        rows.iter().map(|r| (r.source, r.line)).collect::<Vec<_>>(),
        vec![(0, 0), (0, 1), (1, 0), (0, 2), (1, 1)]
    );
    assert!(rows.last().unwrap().timestamp_ms.is_none());
    ws.handle_key(key(KeyCode::Esc), 30);
    ws.execute_command("trace req-42").unwrap();
    job(&mut ws);
    ready(&mut ws);
    let rows = ws.investigation_rows().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].source, 0);
    assert_eq!(rows[1].source, 1);
    assert!(ws.panes.iter().all(|a| a.filter.has_active_filters()));
}
#[test]
fn session_roundtrip_preserves_views_and_failed_load_is_transactional() {
    let (dir, mut ws) = fixture();
    ws.panes[0].ui_state.bookmarks.insert(1);
    ws.panes[0].ui_state.scroll_col = 17;
    ws.panes[0].ui_state.reverse_order = true;
    ws.panes[1].reader.set_tail_enabled(true);
    let reader = ws.panes[0].reader.clone();
    ws.panes[0]
        .filter
        .add_rule(
            "NOT level:debug".into(),
            FilterKind::Include,
            false,
            false,
            &reader,
        )
        .unwrap();
    ready(&mut ws);
    let session = dir.path().join("investigation session.json");
    ws.execute_command(&format!("save {}", session.display()))
        .unwrap();
    ws.panes[0].ui_state.scroll_col = 0;
    ws.execute_command(&format!("load {}", session.display()))
        .unwrap();
    ready(&mut ws);
    assert_eq!(ws.panes.len(), 2);
    assert_eq!(ws.panes[0].ui_state.scroll_col, 17);
    assert!(ws.panes[0].ui_state.bookmarks.contains(&1));
    assert!(ws.panes[0].ui_state.reverse_order);
    assert!(ws.panes[1].reader.is_tail_enabled());
    assert_eq!(ws.panes[0].filter.rules[0].pattern, "NOT level:debug");
    ws.execute_command(&format!("save {}", session.display()))
        .unwrap(); // saving an existing valid session is supported
    let original = ws.panes[0].reader.path.clone();
    std::fs::remove_file(&ws.panes[1].reader.path).unwrap();
    assert!(
        ws.execute_command(&format!("load {}", session.display()))
            .is_err()
    );
    assert_eq!(ws.panes[0].reader.path, original);
}
#[test]
fn redaction_handles_nested_json_embedded_json_headers_and_custom_keys() {
    let r = Redactor::new(&["email".into()]).unwrap();
    for text in [
        r#"{"token":"TOKEN_VALUE","nested":[{"password":"PASSWORD_VALUE"}],"email":"EMAIL_VALUE","message":"safe"}"#,
        r#"2026-10-21T10:00:00Z stdout F {"api_key":"API_VALUE","msg":"safe"}"#,
        r#"[{"token":["TOKEN_VALUE","PASSWORD_VALUE"]}]"#,
        "Authorization: Bearer AUTH_VALUE",
        "token='QUOTED_VALUE' password=PLAIN_VALUE email=EMAIL_VALUE",
        "Cookie: id=COOKIE_VALUE; other=COOKIE_OTHER",
    ] {
        let result = r.redact(text);
        for secret in [
            "TOKEN_VALUE",
            "PASSWORD_VALUE",
            "EMAIL_VALUE",
            "API_VALUE",
            "AUTH_VALUE",
            "QUOTED_VALUE",
            "PLAIN_VALUE",
            "COOKIE_VALUE",
            "COOKIE_OTHER",
        ] {
            assert!(!result.contains(secret), "secret leaked in {result}");
        }
        assert!(result.contains("[REDACTED]"), "{result}");
    }
}
#[test]
fn incident_export_includes_context_and_sources_without_secrets_or_duplicates() {
    let (dir, mut ws) = fixture();
    ws.execute_command("trace req-42").unwrap();
    job(&mut ws);
    let out = dir.path().join("incident.jsonl");
    ws.execute_command(&format!("export {}", out.display()))
        .unwrap();
    job(&mut ws);
    assert!(matches!(
        ws.panes[ws.active].modal,
        ActiveModal::Message {
            is_error: false,
            ..
        }
    ));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(!text.contains("secret-one"));
    assert!(!text.contains("secret-two"));
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["type"], "flik_investigation");
    assert_eq!(lines[0]["sources"].as_array().unwrap().len(), 2);
    assert_eq!(lines.len(), 6); // metadata plus 5 unique events, including context
    assert_eq!(
        lines.iter().skip(1).filter(|v| v["match"] == true).count(),
        2
    );
    let before = std::fs::read(&out).unwrap();
    ws.execute_command(&format!("export {}", out.display()))
        .unwrap();
    job(&mut ws);
    assert_eq!(std::fs::read(&out).unwrap(), before);
}
#[test]
fn cancellation_and_bad_sources_do_not_publish_partial_exports() {
    let (dir, mut ws) = fixture();
    let sources: Vec<_> = ws.panes.iter().map(Source::capture).collect();
    let rows =
        investigation::collect(&sources, &AtomicBool::new(false), &AtomicUsize::new(0)).unwrap();
    let out = dir.path().join("cancelled.jsonl");
    assert!(
        investigation::export_bundle(
            &out,
            &sources,
            &rows,
            1,
            &Redactor::new(&[]).unwrap(),
            &AtomicBool::new(true)
        )
        .is_err()
    );
    assert!(!out.exists());
    ws.execute_command("merge").unwrap();
    ws.handle_key(key(KeyCode::Esc), 30);
    assert!(!ws.is_investigating());
    assert!(ws.investigation_rows().is_none());
}
#[test]
fn session_save_refuses_overwriting_logs_and_invalid_files() {
    let (dir, ws) = fixture();
    let session = Session {
        version: 1,
        panes: ws
            .panes
            .iter()
            .map(SavedPane::capture)
            .collect::<Result<_, _>>()
            .unwrap(),
        active: 0,
        history: vec![],
        redaction_keys: vec![],
    };
    let path = &ws.panes[0].reader.path;
    let before = std::fs::read(path).unwrap();
    assert!(session.save(path).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
    let out = dir.path().join("bad.json");
    std::fs::write(&out, "{\"version\":2,\"panes\":[],\"active\":0}").unwrap();
    assert!(Session::read(&out).is_err());
}
#[test]
fn command_dialog_and_investigation_views_render_in_small_terminals() {
    use ratatui::{buffer::Buffer, layout::Rect};
    let (_dir, mut ws) = fixture();
    ws.execute_command("context 1").unwrap();
    for (w, h) in [(200, 40), (80, 20), (20, 6), (1, 1), (0, 0)] {
        let area = Rect::new(0, 0, w, h);
        let mut buffer = Buffer::empty(area);
        ws.render(area, &mut buffer);
    }
    ws.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL), 30);
    for ch in "merge".chars() {
        ws.handle_key(key(KeyCode::Char(ch)), 30);
    }
    ws.handle_key(key(KeyCode::Enter), 30);
    job(&mut ws);
    assert_eq!(ws.investigation_rows().unwrap().len(), 5);
}

#[test]
fn filter_editor_preserves_quoted_boolean_atoms() {
    let (_dir, mut ws) = fixture();
    ws.handle_key(key(KeyCode::Char('3')), 30);
    for c in "level:error AND NOT msg:\"normal output\"".chars() {
        ws.handle_key(key(KeyCode::Char(c)), 30);
    }
    ws.handle_key(key(KeyCode::Enter), 30);
    ready(&mut ws);
    assert_eq!(ws.panes[0].filter.rules.len(), 1);
    assert_eq!(ws.panes[0].filter.filtered_count(&ws.panes[0].reader), 1);
}
#[test]
fn context_next_previous_matches_and_snapshot_invalidity() {
    let (dir, mut ws) = fixture();
    let path = dir.path().join("extra.log");
    std::fs::write(&path, "ERROR first\nnormal neighbor\nERROR second\n").unwrap();
    ws.open_other(&path).unwrap();
    ready(&mut ws);
    let app = &mut ws.panes[ws.active];
    app.filter
        .add_rule(
            "ERROR".into(),
            FilterKind::Include,
            false,
            false,
            &app.reader,
        )
        .unwrap();
    ready(&mut ws);
    ws.execute_command("context 1").unwrap();
    ws.handle_key(key(KeyCode::Char('n')), 30);
    assert!(
        ws.investigation_rows()
            .unwrap()
            .iter()
            .any(|r| r.text.contains("second"))
    );
    assert!(
        !ws.investigation_rows()
            .unwrap()
            .iter()
            .any(|r| r.text.contains("first"))
    );
    ws.handle_key(key(KeyCode::Char('p')), 30);
    assert!(
        ws.investigation_rows()
            .unwrap()
            .iter()
            .any(|r| r.text.contains("first"))
    );
    let source = Source::capture(&ws.panes[ws.active]);
    let replacement = dir.path().join("new.log");
    std::fs::write(&replacement, "replacement longer file content\n").unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    assert!(source.validate().is_err());
}
