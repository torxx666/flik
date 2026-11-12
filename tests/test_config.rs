use std::io::Write;
use tempfile::tempdir;
use flik::config::FlikConfig;
use flik::app::App;

#[test]
fn test_config_parsing_and_apply_to_app() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
[general]
mouse_capture = false
auto_indent_json = true
reverse_order = true
theme = "light"

[alerts]
enabled = true
threshold = 12

[[formats]]
name = "my_app"
pattern = '^\[(?P<timestamp>[^\]]+)\] \[(?P<level>[A-Z]+)\] (?P<msg>.*)$'
"#,
    ).unwrap();

    let formats_dir = dir.path().join("formats");
    std::fs::create_dir_all(&formats_dir).unwrap();
    std::fs::write(
        formats_dir.join("worker.toml"),
        r#"
name = "worker_log"
pattern = '^(?P<time>\S+) worker=(?P<id>\d+) (?P<status>\S+)$'
"#,
    ).unwrap();

    let config = FlikConfig::load_from_dir(dir.path());
    assert_eq!(config.general.mouse_capture, Some(false));
    assert_eq!(config.general.auto_indent_json, Some(true));
    assert_eq!(config.general.reverse_order, Some(true));
    assert_eq!(config.general.theme.as_deref(), Some("light"));
    assert_eq!(config.alerts.threshold, Some(12));

    // Formats loaded from both config.toml and formats/*.toml
    assert_eq!(config.formats.len(), 2);
    let my_app_fmt = config.find_format("my_app").expect("my_app format found");
    assert_eq!(my_app_fmt.name, "my_app");
    let worker_fmt = config.find_format("worker_log").expect("worker_log format found");
    assert_eq!(worker_fmt.name, "worker_log");

    // Test apply to App
    let mut temp_log = tempfile::NamedTempFile::new().unwrap();
    writeln!(temp_log, "[2026-10-26T12:00:00Z] [ERROR] Something failed").unwrap();
    temp_log.flush().unwrap();

    let mut app = App::new(temp_log.path().to_path_buf()).unwrap();
    config.apply(&mut app);

    assert!(!app.mouse_capture_enabled);
    assert!(app.ui_state.auto_indent_json);
    assert!(app.ui_state.reverse_order);
    assert_eq!(app.ui_state.alert_threshold, 12);

    // Apply custom format to app
    app.format.set_custom_regex(&my_app_fmt.pattern).unwrap();
    assert_eq!(app.format.selected_format, flik::format::LogFormat::Custom);
    assert!(app.format.available_fields.contains(&"timestamp".to_string()));
    assert!(app.format.available_fields.contains(&"level".to_string()));
    assert!(app.format.available_fields.contains(&"msg".to_string()));
}

#[test]
fn test_auto_save_preferences_and_theme_persistence() {
    let dir = tempdir().unwrap();
    let config_file = dir.path().join("config.toml");

    // Write initial config
    std::fs::write(&config_file, "[general]\ntheme = \"dark_slate\"\n").unwrap();

    // Load and update preferences
    let mut config = FlikConfig::load_from_dir(dir.path());
    config.general.theme = Some("nord_light".to_string());
    config.general.auto_indent_json = Some(true);
    config.general.mouse_capture = Some(false);

    let toml_str = toml::to_string_pretty(&config).unwrap();
    std::fs::write(&config_file, toml_str).unwrap();

    // Reload from dir and verify persistence
    let reloaded = FlikConfig::load_from_dir(dir.path());
    assert_eq!(reloaded.general.theme.as_deref(), Some("nord_light"));
    assert_eq!(reloaded.general.auto_indent_json, Some(true));
    assert_eq!(reloaded.general.mouse_capture, Some(false));

    // Verify ThemeMode conversion
    let mode = flik::ui::theme::ThemeMode::from_str_name("nord_light").unwrap();
    assert_eq!(mode, flik::ui::theme::ThemeMode::NordLight);
    assert_eq!(mode.as_str(), "nord_light");
}

