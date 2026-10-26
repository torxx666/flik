use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use flik::{
    analytics::{StatsSummary, extract_timestamp},
    app::App,
    completion,
    filter::{FilterKind, FilterRule},
    investigation::Source,
    reports,
    ui::modals::ActiveModal,
    workspace::Workspace,
};
use std::{
    io::Write,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

fn wait(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn ready(app: &App) {
    wait(|| {
        !app.reader.is_indexing.load(Ordering::Acquire)
            && !app.filter.is_filtering.load(Ordering::Acquire)
    });
}
fn app(text: &str) -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.log");
    std::fs::write(&path, text).unwrap();
    let app = App::new(path).unwrap();
    ready(&app);
    (dir, app)
}
fn add(app: &mut App, pattern: &str) {
    app.filter
        .add_rule(
            pattern.into(),
            FilterKind::Include,
            false,
            false,
            &app.reader,
        )
        .unwrap();
    ready(app);
}
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn apache_error_stats_detect_levels_and_timestamps_without_http_groups() {
    let text = "[Mon Oct 19 12:04:52.708041 2026] [core:error] [pid 10] failure\n[Mon Oct 19 12:04:53.000001 2026] [mpm:notice] [pid 10] ready\n[Mon Oct 19 13:04:53.000001 2026] [core:warn] [pid 10] slow\n";
    let (_dir, app) = app(text);
    let stats = StatsSummary::compute(&app.reader, 5000);
    assert_eq!(stats.status_total, 0);
    assert_eq!(stats.level_counts["ERROR"], 1);
    assert_eq!(stats.level_counts["NOTICE"], 1);
    assert_eq!(stats.level_counts["WARN"], 1);
    assert_eq!(
        extract_timestamp(text.lines().next().unwrap())
            .unwrap()
            .to_rfc3339(),
        "2026-10-19T12:04:52.708041+00:00"
    );
    let report = reports::frequency(
        &Source::capture(&app),
        3600,
        &AtomicBool::new(false),
        &AtomicUsize::new(0),
    )
    .unwrap();
    assert!(report.iter().any(|line| line.contains("3 timed matches")));
    assert!(
        report
            .iter()
            .any(|line| line.contains("2026-10-19 12:00") && line.contains("         2"))
    );
}

#[test]
fn frequency_respects_filters_dates_timezones_and_excludes_context() {
    let (_dir, mut app) = app(
        "2026-10-19T14:00:00+02:00 INFO cause\n2026-10-19T12:00:01Z ERROR failure\n2026-10-19T14:00:00Z ERROR failure\n2026-10-20T12:00:00Z ERROR failure\nERROR untimed\n",
    );
    add(&mut app, "//error");
    let report = reports::frequency(
        &Source::capture(&app),
        3600,
        &AtomicBool::new(false),
        &AtomicUsize::new(0),
    )
    .unwrap();
    assert!(
        report
            .iter()
            .any(|line| line.contains("3 timed matches | 1 without"))
    );
    assert!(
        report
            .iter()
            .any(|line| line.contains("2026-10-19 13:00") && line.contains("         0"))
    );
    assert!(report.iter().any(|line| line.contains("2026-10-20 12:00")));
    assert_eq!(*app.filter.filtered_indices.read(), vec![0, 1, 2, 3, 4]);
}

#[test]
fn comparison_uses_equal_windows_and_detects_new_and_disappeared_patterns() {
    let (_dir, app) = app(
        "2026-10-19T11:59:00Z ERROR database disconnected\n2026-10-19T12:00:00Z ERROR queue unavailable\n2026-10-19T12:01:00Z ERROR queue unavailable\n2026-10-19T13:00:00Z ERROR outside window\n",
    );
    let pivot = extract_timestamp("2026-10-19T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    let lines = reports::compare(
        &[Source::capture(&app)],
        Some((pivot, 5 * 60000)),
        &AtomicBool::new(false),
        &AtomicUsize::new(0),
    )
    .unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Before/A: 1 matches | After/B: 2"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("NEW") && line.contains("queue unavailable"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("DISAPPEARED") && line.contains("database disconnected"))
    );
    assert!(!lines.iter().any(|line| line.contains("outside window")));
    let (_dir2, app2) = self::app("2026-10-19T11:59:00Z ERROR database disconnected\n");
    let lines = reports::compare(
        &[Source::capture(&app), Source::capture(&app2)],
        None,
        &AtomicBool::new(false),
        &AtomicUsize::new(0),
    )
    .unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Before/A: 4 matches | After/B: 1"))
    );
}

#[test]
fn completion_suggests_observed_fields_and_values_and_cycles_in_search() {
    let (_dir, app) = app(
        "{\"service\":\"auth\",\"level\":\"ERROR\"}\n{\"service\":\"billing\",\"level\":\"INFO\"}\n",
    );
    assert_eq!(
        completion::candidates("serv", &app.reader, &app.format),
        vec!["service:"]
    );
    assert_eq!(
        completion::candidates("service:", &app.reader, &app.format),
        vec!["service:auth", "service:billing"]
    );
    assert_eq!(
        completion::candidates("level:ERROR AND service:b", &app.reader, &app.format),
        vec!["level:ERROR AND service:billing"]
    );
    let mut ws = Workspace::new(app);
    ws.handle_key(key(KeyCode::Char('/')), 30);
    for c in "service:".chars() {
        ws.handle_key(key(KeyCode::Char(c)), 30);
    }
    ws.handle_key(key(KeyCode::Tab), 30);
    assert!(matches!(&ws.panes[0].modal, ActiveModal::Search { query } if query == "service:auth"));
    ws.handle_key(key(KeyCode::Tab), 30);
    assert!(
        matches!(&ws.panes[0].modal, ActiveModal::Search { query } if query == "service:billing")
    );
    ws.handle_key(key(KeyCode::Enter), 30);
    ready(&ws.panes[0]);
    assert_eq!(*ws.panes[0].filter.filtered_indices.read(), vec![1]);
}

#[test]
fn configurable_context_handles_chunk_boundaries_and_future_live_lines() {
    let mut text = "INFO noise\n".repeat(499);
    text.push_str("ERROR boundary\nINFO next\n");
    let (_dir, mut app) = app(&text);
    add(&mut app, "//2,3:error");
    assert_eq!(
        *app.filter.filtered_indices.read(),
        vec![497, 498, 499, 500]
    );
    std::fs::OpenOptions::new()
        .append(true)
        .open(&app.reader.path)
        .unwrap()
        .write_all(b"INFO future1\nINFO future2\nINFO unrelated\n")
        .unwrap();
    wait(|| app.reader.line_count() == 504);
    wait(|| {
        app.filter.process_incoming_lines(&app.reader);
        app.filter.filtered_indices.read().last() == Some(&502)
    });
    assert_eq!(
        *app.filter.filtered_indices.read(),
        vec![497, 498, 499, 500, 501, 502]
    );
    assert!(FilterRule::new(0, "//1001:error".into(), FilterKind::Include, false, false).is_err());
}

#[test]
fn histogram_shortcut_runs_filtered_report_in_background_and_report_scrolls() {
    let (_dir, mut app) =
        app("2026-10-19T12:00:00Z INFO okay\n2026-10-19T13:00:00Z ERROR failure\n");
    add(&mut app, "error");
    let mut ws = Workspace::new(app);
    ws.handle_key(key(KeyCode::Char('H')), 30);
    assert!(ws.is_investigating());
    wait(|| {
        ws.poll_job();
        !ws.is_investigating()
    });
    assert!(
        matches!(&ws.panes[0].modal, ActiveModal::Report { lines, .. } if lines.iter().any(|s| s.contains("1 timed matches")))
    );
    ws.handle_key(key(KeyCode::Down), 30);
    assert!(matches!(
        &ws.panes[0].modal,
        ActiveModal::Report { scroll: 1, .. }
    ));
    ws.handle_key(key(KeyCode::Esc), 30);
    ws.execute_command("compare 2026-10-19T12:30:00Z 60")
        .unwrap();
    wait(|| {
        ws.poll_job();
        !ws.is_investigating()
    });
    assert!(
        matches!(&ws.panes[0].modal, ActiveModal::Report { title, .. } if title.contains("comparison"))
    );
    assert!(
        reports::frequency(
            &Source::capture(&ws.panes[0]),
            3600,
            &AtomicBool::new(true),
            &AtomicUsize::new(0)
        )
        .is_err()
    );
}

#[test]
fn apache_stats_render_levels_and_select_notice_without_empty_http_statistics() {
    use ratatui::{buffer::Buffer, layout::Rect};
    let (_dir, app) = app(
        "[Mon Oct 19 12:04:52.708041 2026] [core:error] failure\n[Mon Oct 19 12:04:53.000001 2026] [mpm:notice] ready\n",
    );
    let mut ws = Workspace::new(app);
    ws.handle_key(key(KeyCode::Char('s')), 40);
    let area = Rect::new(0, 0, 180, 45);
    let mut buffer = Buffer::empty(area);
    ws.render(area, &mut buffer);
    let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("[1] Levels"));
    assert!(text.contains("NOTICE"));
    assert!(!text.contains("2xx Success"));
    ws.handle_key(key(KeyCode::Down), 40);
    ws.handle_key(key(KeyCode::Enter), 40);
    ready(&ws.panes[0]);
    assert_eq!(*ws.panes[0].filter.filtered_indices.read(), vec![1]);
}

#[test]
fn frequency_scans_beyond_preview_and_sample_limits() {
    let mut text = "2026-10-19T12:00:00Z INFO unrelated\n".repeat(6000);
    text.push_str("2026-10-19T14:00:00Z ERROR late\n2026-10-19T13:00:00Z ERROR out_of_order\n");
    let (_dir, mut app) = app(&text);
    add(&mut app, "error");
    let scanned = AtomicUsize::new(0);
    let report = reports::frequency(
        &Source::capture(&app),
        3600,
        &AtomicBool::new(false),
        &scanned,
    )
    .unwrap();
    assert_eq!(scanned.load(Ordering::Relaxed), 6002);
    assert!(report.iter().any(|line| line.contains("2 timed matches")));
    assert!(report.iter().any(|line| line.contains("2026-10-19 13:00")));
    assert!(report.iter().any(|line| line.contains("2026-10-19 14:00")));
}

#[test]
fn field_completion_can_continue_to_values_and_preserves_quoted_values() {
    let (_dir, app) = app("{\"service\":\"billing worker\",\"level\":\"ERROR\"}\n");
    let mut ws = Workspace::new(app);
    ws.handle_key(key(KeyCode::Char('/')), 30);
    for c in "serv".chars() {
        ws.handle_key(key(KeyCode::Char(c)), 30);
    }
    ws.handle_key(key(KeyCode::Tab), 30);
    ws.handle_key(key(KeyCode::Tab), 30);
    assert!(
        matches!(&ws.panes[0].modal, ActiveModal::Search { query } if query == "service:\"billing worker\"")
    );
    ws.handle_key(key(KeyCode::Enter), 30);
    ready(&ws.panes[0]);
    assert_eq!(*ws.panes[0].filter.filtered_indices.read(), vec![0]);
}
