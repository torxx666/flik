use flik::format::{FormatManager, LogFormat};

#[test]
fn test_custom_regex_parser_with_named_groups() {
    let mut fmt = FormatManager::new();
    let pattern = r#"^(?P<ip>\S+) (?P<user>\S+) \[(?P<timestamp>[^\]]+)\] "(?P<request>[^"]*)" (?P<status>\d{3}) (?P<latency>[\d\.]+)$"#;
    
    assert!(fmt.set_custom_regex(pattern).is_ok());
    assert_eq!(fmt.active_format(), LogFormat::Custom);
    assert_eq!(
        fmt.available_fields,
        vec!["ip", "user", "timestamp", "request", "status", "latency"]
    );

    let line = r#"172.16.0.42 admin [05/Nov/2026:10:00:00 +0200] "POST /api/v2/deploy HTTP/2" 500 0.425"#;
    let record = fmt.parse_line(line);

    assert_eq!(record.get_field_value("ip"), Some("172.16.0.42"));
    assert_eq!(record.get_field_value("user"), Some("admin"));
    assert_eq!(record.get_field_value("timestamp"), Some("05/Nov/2026:10:00:00 +0200"));
    assert_eq!(record.get_field_value("request"), Some("POST /api/v2/deploy HTTP/2"));
    assert_eq!(record.status, Some(500));
    assert_eq!(record.get_field_value("status"), Some("500"));
    assert_eq!(record.get_field_value("latency"), Some("0.425"));
}

#[test]
fn test_flexible_access_log_with_vhost_and_custom_latency() {
    let mut fmt = FormatManager::new();
    let sample = vec![
        r#"api.example.com 192.168.1.50 - frank [05/Nov/2026:10:00:00 +0000] "GET /v1/metrics HTTP/1.1" 200 4096 "https://dashboard.example.com" "curl/8.5.0" 0.035 0.012"#.to_string(),
        r#"api.example.com 192.168.1.51 - - [05/Nov/2026:10:00:01 +0000] "POST /v1/auth HTTP/1.1" 401 128 "-" "PostmanRuntime/7.36.0" 0.120"#.to_string(),
    ];

    fmt.detect_and_index_fields(&sample);
    assert_eq!(fmt.detected_format, LogFormat::ApacheCombined);
    assert!(fmt.available_fields.contains(&"vhost".to_string()));
    assert!(fmt.available_fields.contains(&"request_time".to_string()));

    let record = fmt.parse_line(&sample[0]);
    assert_eq!(record.get_field_value("vhost"), Some("api.example.com"));
    assert_eq!(record.get_field_value("ip"), Some("192.168.1.50"));
    assert_eq!(record.get_field_value("user"), Some("frank"));
    assert_eq!(record.status, Some(200));
    assert_eq!(record.get_field_value("request_time"), Some("0.035"));
    assert_eq!(record.get_field_value("extra_1"), Some("0.035"));
    assert_eq!(record.get_field_value("extra_2"), Some("0.012"));
}

#[test]
fn test_logfmt_detection_and_field_parsing() {
    let mut fmt = FormatManager::new();
    let samples = vec![
        r#"time="2026-11-05T09:12:30Z" level=info msg="User signed in" user_id=42 tenant=acme ip=10.0.0.1 status=200 duration=15ms"#.to_string(),
        r#"time="2026-11-05T09:12:31Z" level=error msg="Database connection timeout" db=postgres_replica status=504 retry=3 duration=5000ms"#.to_string(),
    ];

    fmt.detect_and_index_fields(&samples);
    assert_eq!(fmt.detected_format, LogFormat::Logfmt);
    assert!(fmt.available_fields.contains(&"user_id".to_string()));
    assert!(fmt.available_fields.contains(&"tenant".to_string()));
    assert!(fmt.available_fields.contains(&"db".to_string()));

    let record = fmt.parse_line(&samples[1]);
    assert_eq!(record.timestamp, Some("2026-11-05T09:12:31Z".to_string()));
    assert_eq!(record.level, Some("ERROR".to_string()));
    assert_eq!(record.status, Some(504));
    assert_eq!(record.message, "Database connection timeout");
    assert_eq!(record.get_field_value("db"), Some("postgres_replica"));
    assert_eq!(record.get_field_value("retry"), Some("3"));
    assert_eq!(record.get_field_value("duration"), Some("5000ms"));
}

#[test]
fn test_syslog_auth_log_enrichment() {
    let mut fmt = FormatManager::new();
    let samples = vec![
        r#"Nov  5 10:14:02 bastion sshd[28491]: Failed password for invalid user admin from 192.168.1.100 port 49152 ssh2"#.to_string(),
        r#"Nov  5 10:14:15 bastion sshd[28495]: Accepted publickey for ubuntu from 192.168.1.42 port 51234 ssh2: RSA SHA256:abc123xyz"#.to_string(),
        r#"Nov  5 10:15:30 bastion sudo:    alice : TTY=pts/2 ; PWD=/home/alice ; USER=root ; COMMAND=/bin/cat /etc/shadow"#.to_string(),
        r#"2026-11-05T10:16:00.543210+00:00 bastion sshd[29000]: pam_unix(sshd:auth): authentication failure; logname= uid=0 euid=0 tty=ssh ruser= rhost=10.0.0.99  user=root"#.to_string(),
    ];

    fmt.detect_and_index_fields(&samples);
    assert_eq!(fmt.detected_format, LogFormat::Syslog);
    assert!(fmt.auth_patterns_detected);
    assert!(fmt.available_fields.contains(&"user".to_string()));
    assert!(fmt.available_fields.contains(&"ip".to_string()));
    assert!(fmt.available_fields.contains(&"action".to_string()));
    assert!(fmt.available_fields.contains(&"auth_status".to_string()));

    // 1. SSH failed password
    let rec1 = fmt.parse_line(&samples[0]);
    assert_eq!(rec1.get_field_value("user"), Some("admin"));
    assert_eq!(rec1.get_field_value("ip"), Some("192.168.1.100"));
    assert_eq!(rec1.get_field_value("port"), Some("49152"));
    assert_eq!(rec1.get_field_value("action"), Some("failed_login"));
    assert_eq!(rec1.get_field_value("auth_status"), Some("FAIL"));
    assert_eq!(rec1.level, Some("WARN".to_string()));

    // 2. SSH accepted publickey
    let rec2 = fmt.parse_line(&samples[1]);
    assert_eq!(rec2.get_field_value("user"), Some("ubuntu"));
    assert_eq!(rec2.get_field_value("ip"), Some("192.168.1.42"));
    assert_eq!(rec2.get_field_value("action"), Some("accepted_publickey"));
    assert_eq!(rec2.get_field_value("auth_status"), Some("SUCCESS"));
    assert_eq!(rec2.level, Some("INFO".to_string()));

    // 3. Sudo command
    let rec3 = fmt.parse_line(&samples[2]);
    assert_eq!(rec3.get_field_value("user"), Some("alice"));
    assert_eq!(rec3.get_field_value("target_user"), Some("root"));
    assert_eq!(rec3.get_field_value("command"), Some("/bin/cat /etc/shadow"));
    assert_eq!(rec3.get_field_value("action"), Some("sudo_command"));
    assert_eq!(rec3.get_field_value("auth_status"), Some("SUCCESS"));
    assert_eq!(rec3.level, Some("NOTICE".to_string()));

    // 4. ISO8601 Syslog + PAM failure
    let rec4 = fmt.parse_line(&samples[3]);
    assert_eq!(rec4.timestamp, Some("2026-11-05T10:16:00.543210+00:00".to_string()));
    assert_eq!(rec4.get_field_value("user"), Some("root"));
    assert_eq!(rec4.get_field_value("ip"), Some("10.0.0.99"));
    assert_eq!(rec4.get_field_value("action"), Some("pam_failure"));
    assert_eq!(rec4.get_field_value("auth_status"), Some("FAIL"));
    assert_eq!(rec4.level, Some("ERROR".to_string()));
}

#[test]
fn test_w3c_traceparent_and_correlation_extraction() {
    use flik::analytics::TraceCorrelator;

    // 1. W3C traceparent extraction
    let w3c_line = r#"2026-11-05T10:20:00Z INFO HTTP request traceparent=00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01 uri=/checkout"#;
    let ids = TraceCorrelator::extract_identifiers(w3c_line);
    assert!(ids.iter().any(|(k, v)| k == "trace_id" && v == "4bf92f3577b34da6a3ce929d0e0e4736"));
    assert!(ids.iter().any(|(k, v)| k == "span_id" && v == "00f067aa0ba902b7"));

    // 2. Co-existence of UUID, user, trace_id and IP without shadowing
    let mixed_line = r#"{"trace_id":"trace-123","user_id":"usr-99","task_uuid":"123e4567-e89b-12d3-a456-426614174000","client_ip":"10.0.0.42"}"#;
    let ids2 = TraceCorrelator::extract_identifiers(mixed_line);
    assert!(ids2.iter().any(|(k, v)| k == "trace_id" && v == "trace-123"));
    assert!(ids2.iter().any(|(k, v)| k == "user_id" && v == "usr-99"));
    assert!(ids2.iter().any(|(k, v)| k == "uuid" && v == "123e4567-e89b-12d3-a456-426614174000"));
    assert!(ids2.iter().any(|(k, v)| (k == "ip" || k == "client_ip") && v == "10.0.0.42"));

    // 3. Syslog security event user extraction
    let auth_line = "Nov 5 10:25:00 bastion sshd[1234]: Failed password for invalid user hacker from 198.51.100.23 port 54321 ssh2";
    let ids3 = TraceCorrelator::extract_identifiers(auth_line);
    assert!(ids3.iter().any(|(k, v)| k == "user" && v == "hacker"));
    assert!(ids3.iter().any(|(k, v)| k == "ip" && v == "198.51.100.23"));
}

#[test]
fn test_live_stream_error_alerting() {
    use std::io::Write;
    use flik::app::App;
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(file, "2026-11-05T10:00:00Z INFO Server starting").unwrap();
    file.flush().unwrap();

    let mut app = App::new(file.path().to_path_buf()).unwrap();
    assert_eq!(app.ui_state.live_stream_errors, 0);
    assert_eq!(app.ui_state.alert_threshold, 5);

    // Append 5 errors to trigger alert
    for i in 1..=5 {
        writeln!(file, "2026-11-05T10:00:0{}Z ERROR Failed request status=500 id={}", i, i).unwrap();
    }
    file.flush().unwrap();

    // Allow reader indexer to observe appends
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.tick(30);

    assert_eq!(app.ui_state.live_stream_errors, 5);
    assert!(app.ui_state.alert_toast_shown);
    assert!(app.ui_state.status_toast.is_some());
    let (msg, _) = app.ui_state.status_toast.as_ref().unwrap();
    assert!(msg.contains("STREAM ALERT"));
}

#[test]
fn test_cloudwatch_and_stackdriver_json_parsing() {
    let mut fmt = FormatManager::new();
    fmt.selected_format = LogFormat::Json;

    // 1. AWS CloudWatch Insights format with @timestamp, @message, @logStream
    let cw_line = r#"{"@timestamp":"2026-11-05T10:00:00.000Z","@message":"Task timed out after 30.00 seconds","@logStream":"2026/11/05/[$LATEST]abcd","status":504}"#;
    let rec_cw = fmt.parse_line(cw_line);
    assert_eq!(rec_cw.timestamp, Some("2026-11-05T10:00:00.000Z".to_string()));
    assert_eq!(rec_cw.message, "Task timed out after 30.00 seconds");
    assert_eq!(rec_cw.status, Some(504));
    assert_eq!(rec_cw.get_field_value("@logStream"), Some("2026/11/05/[$LATEST]abcd"));

    // 2. AWS CloudWatch with numeric epoch milliseconds timestamp
    let cw_epoch = r#"{"timestamp":1762336800000,"@message":"Payment processor connection failed","logStreamName":"i-12345","level":"error"}"#;
    let rec_cw2 = fmt.parse_line(cw_epoch);
    assert!(rec_cw2.timestamp.is_some());
    assert!(rec_cw2.timestamp.as_ref().unwrap().starts_with("2025") || rec_cw2.timestamp.as_ref().unwrap().starts_with("2026"));
    assert_eq!(rec_cw2.message, "Payment processor connection failed");
    assert_eq!(rec_cw2.level, Some("ERROR".to_string()));

    // 3. Google Cloud Logging / Stackdriver with textPayload and severity
    let gcp_text = r#"{"insertId":"abc-123","timestamp":"2026-11-05T10:05:00Z","severity":"WARNING","textPayload":"Slow database query on users table"}"#;
    let rec_gcp = fmt.parse_line(gcp_text);
    assert_eq!(rec_gcp.timestamp, Some("2026-11-05T10:05:00Z".to_string()));
    assert_eq!(rec_gcp.level, Some("WARNING".to_string()));
    assert_eq!(rec_gcp.message, "Slow database query on users table");
    assert_eq!(rec_gcp.get_field_value("insertId"), Some("abc-123"));

    // 4. Google Cloud Logging with jsonPayload
    let gcp_json = r#"{"insertId":"xyz-999","timestamp":"2026-11-05T10:06:00Z","severity":"ERROR","jsonPayload":{"message":"Connection pool exhausted","active_connections":100,"status":503}}"#;
    let rec_gcp2 = fmt.parse_line(gcp_json);
    assert_eq!(rec_gcp2.message, "Connection pool exhausted");
    assert_eq!(rec_gcp2.level, Some("ERROR".to_string()));
    assert_eq!(rec_gcp2.status, Some(503));
    // 5. Automatic format detection on CloudWatch & Stackdriver sample
    let mut auto_fmt = FormatManager::new();
    let samples = vec![
        cw_line.to_string(),
        gcp_text.to_string(),
        gcp_json.to_string(),
    ];
    auto_fmt.detect_and_index_fields(&samples);
    assert_eq!(auto_fmt.detected_format, LogFormat::Json);
    assert!(auto_fmt.available_fields.contains(&"@message".to_string()));
    assert!(auto_fmt.available_fields.contains(&"textPayload".to_string()));
    assert!(auto_fmt.available_fields.contains(&"active_connections".to_string()));
}

