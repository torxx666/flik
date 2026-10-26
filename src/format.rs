use regex::Regex;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LogFormat {
    Auto,
    Json,
    ApacheCombined,
    ApacheCommon,
    ApacheError,
    NginxError,
    Kubernetes,
    Syslog,
    Raw,
}

impl LogFormat {
    pub fn name(&self) -> &'static str {
        match self {
            LogFormat::Auto => "Auto-Detect",
            LogFormat::Json => "JSON",
            LogFormat::ApacheCombined => "Apache Combined",
            LogFormat::ApacheCommon => "Apache Common",
            LogFormat::ApacheError => "Apache Error",
            LogFormat::NginxError => "Nginx Error",
            LogFormat::Kubernetes => "Kubernetes / CRI-O",
            LogFormat::Syslog => "Syslog",
            LogFormat::Raw => "Raw Text",
        }
    }

    #[allow(dead_code)]
    pub fn all() -> &'static [LogFormat] {
        &[
            LogFormat::Auto,
            LogFormat::Json,
            LogFormat::ApacheCombined,
            LogFormat::ApacheCommon,
            LogFormat::ApacheError,
            LogFormat::NginxError,
            LogFormat::Kubernetes,
            LogFormat::Syslog,
            LogFormat::Raw,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViewMode {
    CompactLine,
    ColumnsTable,
}

#[derive(Debug, Clone)]
pub struct ParsedField {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldValueCount {
    pub value: String,
    pub count: usize,
    pub percentage: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldInspectionResult {
    pub field_name: String,
    pub total_occurrences: usize,
    pub unique_count: usize,
    pub values: Vec<FieldValueCount>,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedRecord {
    pub timestamp: Option<String>,
    pub level: Option<String>,
    pub status: Option<u16>,
    pub message: String,
    pub fields: Vec<ParsedField>,
    pub tokens: Vec<String>,
}

impl ParsedRecord {
    pub fn get_field_value(&self, key: &str) -> Option<&str> {
        let clean_key = key.trim();

        if let Some(inner_key) = clean_key.strip_prefix("log.") {
            return self
                .fields
                .iter()
                .find(|f| f.key.eq_ignore_ascii_case(inner_key) || f.key.eq_ignore_ascii_case(clean_key))
                .map(|f| f.value.as_str());
        }
        if clean_key == "msg" {
            if let Some(field) = self.fields.iter().find(|f| f.key.eq_ignore_ascii_case("msg")) {
                return Some(&field.value);
            }
        }

        if clean_key == "timestamp" || clean_key == "time" || clean_key == "@timestamp" {
            return self.timestamp.as_deref();
        }
        if clean_key == "level" || clean_key == "lvl" || clean_key == "severity" {
            return self.level.as_deref();
        }
        if clean_key == "status" || clean_key == "status_code" {
            if let Some(f) = self.fields.iter().find(|f| f.key == "status" || f.key == "status_code") {
                return Some(&f.value);
            }
            return self.status.as_ref().and_then(|_| {
                self.fields
                    .iter()
                    .find(|f| f.key == clean_key)
                    .map(|f| f.value.as_str())
            });
        }
        if clean_key == "message" || clean_key == "msg" || clean_key == "log" {
            return Some(&self.message);
        }

        // Check if explicitly present in self.fields (case-insensitive)
        if let Some(f) = self.fields.iter().find(|f| f.key.eq_ignore_ascii_case(clean_key)) {
            return Some(&f.value);
        }

        // Positional token extraction: field_N, col_N, $N, or N (1-indexed)
        let num_opt = if let Some(n_str) = clean_key
            .strip_prefix("field_")
            .or_else(|| clean_key.strip_prefix("col_"))
            .or_else(|| clean_key.strip_prefix('$'))
        {
            n_str.parse::<usize>().ok()
        } else {
            clean_key.parse::<usize>().ok()
        };

        if let Some(idx_1based) = num_opt {
            if idx_1based >= 1 && idx_1based <= self.tokens.len() {
                return Some(&self.tokens[idx_1based - 1]);
            }
        }

        None
    }
}

/// Tokenize a raw log line by recognizing bracketed groups [...], quotes "..." / '...',
/// words, and treating whitespace or standalone delimiter separators (' - ', ' | ') appropriately.
pub fn parse_line_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip leading whitespace
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Skip standalone delimiter noise like " - ", " -- ", " | "
        if chars[i] == '-' || chars[i] == '|' {
            let mut j = i;
            while j < len && (chars[j] == '-' || chars[j] == '|') {
                j += 1;
            }
            if j == len || chars[j].is_whitespace() {
                i = j;
                continue;
            }
        }

        // 1. Bracketed group: [...]
        if chars[i] == '[' {
            let start = i;
            i += 1;
            let mut depth = 1;
            while i < len && depth > 0 {
                if chars[i] == '[' {
                    depth += 1;
                } else if chars[i] == ']' {
                    depth -= 1;
                }
                i += 1;
            }
            let tok: String = chars[start..i].iter().collect();
            let trimmed = tok.trim();
            if !trimmed.is_empty() {
                tokens.push(trimmed.to_string());
            }
            continue;
        }

        // 2. Quoted string: "..." or '...'
        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            let start = i;
            i += 1;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < len && chars[i] == quote {
                i += 1;
            }
            // Include trailing punctuation like colon or comma if attached
            while i < len && (chars[i] == ':' || chars[i] == ',') {
                i += 1;
            }
            let tok: String = chars[start..i].iter().collect();
            let trimmed = tok.trim();
            if !trimmed.is_empty() {
                tokens.push(trimmed.to_string());
            }
            continue;
        }

        // 3. Regular token: read until next whitespace
        let start = i;
        while i < len && !chars[i].is_whitespace() {
            i += 1;
        }
        let tok: String = chars[start..i].iter().collect();
        let trimmed = tok.trim();
        if !trimmed.is_empty() && trimmed != "-" && trimmed != "--" && trimmed != "|" {
            tokens.push(trimmed.to_string());
        }
    }

    tokens
}

/// Parses a field selection string such as "5-7", "5,6,7", "5 6 7", "$5, $6", or "timestamp, 5-7".
pub fn parse_field_selection_query(query: &str, available: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let parts: Vec<&str> = query
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let has_positional = available.iter().any(|f| f == "field_1" || f == "field_2");

    for part in parts {
        // Range syntax: 5-7 or field_5-field_7
        if let Some((start_s, end_s)) = part.split_once('-') {
            let s_clean = start_s.trim().strip_prefix("field_").or_else(|| start_s.trim().strip_prefix('$')).unwrap_or(start_s.trim());
            let e_clean = end_s.trim().strip_prefix("field_").or_else(|| end_s.trim().strip_prefix('$')).unwrap_or(end_s.trim());
            if let (Ok(start), Ok(end)) = (s_clean.parse::<usize>(), e_clean.parse::<usize>()) {
                let (min, max) = if start <= end { (start, end) } else { (end, start) };
                for n in min..=max {
                    let field_name = if has_positional {
                        format!("field_{}", n)
                    } else if n >= 1 && n <= available.len() {
                        available[n - 1].clone()
                    } else {
                        format!("field_{}", n)
                    };
                    if !result.contains(&field_name) {
                        result.push(field_name);
                    }
                }
                continue;
            }
        }

        // Prefix syntax: "field_5", "col_5", "$5"
        if let Some(rest) = part.strip_prefix("field_")
            .or_else(|| part.strip_prefix("col_"))
            .or_else(|| part.strip_prefix('$'))
        {
            if let Ok(n) = rest.parse::<usize>() {
                let field_name = format!("field_{}", n);
                if !result.contains(&field_name) {
                    result.push(field_name);
                }
                continue;
            }
        }

        // Pure integer: "5" -> "field_5" if has_positional, else available[4]
        if let Ok(n) = part.parse::<usize>() {
            let field_name = if has_positional {
                format!("field_{}", n)
            } else if n >= 1 && n <= available.len() {
                available[n - 1].clone()
            } else {
                format!("field_{}", n)
            };
            if !result.contains(&field_name) {
                result.push(field_name);
            }
            continue;
        }

        // Match against existing available field names (case-insensitive)
        if let Some(matched) = available.iter().find(|f| f.eq_ignore_ascii_case(part)) {
            if !result.contains(matched) {
                result.push(matched.clone());
            }
        } else {
            let s = part.to_string();
            if !result.contains(&s) {
                result.push(s);
            }
        }
    }

    result
}

pub struct FormatManager {
    pub selected_format: LogFormat,
    pub detected_format: LogFormat,
    pub available_fields: Vec<String>,
    pub visible_fields: Vec<String>,
    pub view_mode: ViewMode,
    apache_combined_re: Regex,
    apache_common_re: Regex,
    apache_error_re: Regex,
    nginx_error_re: Regex,
    k8s_re: Regex,
    syslog_re: Regex,
    json_kv_re: Regex,
}

impl FormatManager {
    pub fn new() -> Self {
        let apache_combined_re = Regex::new(
            r#"^(\S+) \S+ (\S+) \[([^\]]+)\] "([^"]*)" (\d{3}) (\S+)(?: "([^"]*)" "([^"]*)")?"#,
        )
        .expect("Valid Apache regex");

        let apache_common_re =
            Regex::new(r#"^(\S+) \S+ (\S+) \[([^\]]+)\] "([^"]*)" (\d{3}) (\S+)"#)
                .expect("Valid Apache Common regex");

        let apache_error_re = Regex::new(
            r#"^\[([^\]]+)\]\s+\[(?:([^:\]]+):)?([a-zA-Z]+)\]\s+(?:\[pid\s+(\d+)\]\s+)?(?:\[client\s+([^\]]+)\]\s+)?(.*)$"#,
        )
        .expect("Valid Apache Error regex");

        let nginx_error_re = Regex::new(
            r#"^(\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2})\s+\[([a-zA-Z]+)\]\s+(\d+)#(\d+):\s+(?:\*(\d+)\s+)?(.*)$"#,
        )
        .expect("Valid Nginx Error regex");

        let k8s_re = Regex::new(r#"^(\d{4}-\d{2}-\d{2}T[^\s]+)\s+(stdout|stderr)\s+([FP])\s+(.*)$"#)
            .expect("Valid K8s regex");

        let syslog_re = Regex::new(r#"^([A-Z][a-z]{2}\s+\d+\s+\d{2}:\d{2}:\d{2})\s+(\S+)\s+([^:\[]+)(?:\[(\d+)\])?:\s+(.*)$"#)
            .expect("Valid Syslog regex");

        let json_kv_re = Regex::new(r#"^\s*"([a-zA-Z0-9_\-\.]+)"\s*:\s*(.*)$"#)
            .expect("Valid JSON KV regex");

        Self {
            selected_format: LogFormat::Auto,
            detected_format: LogFormat::Raw,
            available_fields: Vec::new(),
            visible_fields: vec!["timestamp".into(), "level".into(), "message".into()],
            view_mode: ViewMode::CompactLine,
            apache_combined_re,
            apache_common_re,
            apache_error_re,
            nginx_error_re,
            k8s_re,
            syslog_re,
            json_kv_re,
        }
    }

    pub fn active_format(&self) -> LogFormat {
        if self.selected_format == LogFormat::Auto {
            self.detected_format
        } else {
            self.selected_format
        }
    }

    pub fn is_json(&self) -> bool {
        self.active_format() == LogFormat::Json
    }

    /// Inspect a sample of lines to auto-detect format and collect all available keys
    pub fn detect_and_index_fields(&mut self, sample_lines: &[String]) {
        if sample_lines.is_empty() {
            return;
        }

        let mut json_single_count = 0;
        let mut json_kv_count = 0;
        let mut k8s_count = 0;
        let mut apache_count = 0;
        let mut apache_error_count = 0;
        let mut nginx_error_count = 0;
        let mut syslog_count = 0;
        let mut field_set = BTreeSet::new();
        let mut starts_with_json_bracket = false;
        let mut max_tokens = 0usize;

        for (idx, line) in sample_lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let tokens = parse_line_tokens(trimmed);
            if tokens.len() > max_tokens {
                max_tokens = tokens.len();
            }

            if idx == 0 && (trimmed.starts_with('{') || trimmed.starts_with('[')) {
                starts_with_json_bracket = true;
            }

            // Test Single-line JSON (NDJSON)
            if (trimmed.starts_with('{') && trimmed.ends_with('}'))
                || (trimmed.starts_with('[') && trimmed.ends_with(']'))
            {
                if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(trimmed) {
                    json_single_count += 1;
                    for (k, _) in map {
                        field_set.insert(k);
                    }
                    continue;
                }
            }

            // Test Multi-line JSON key-value line: "key": value
            if let Some(caps) = self.json_kv_re.captures(trimmed) {
                if let Some(k) = caps.get(1) {
                    json_kv_count += 1;
                    field_set.insert(k.as_str().to_string());
                }
                continue;
            }

            // Test K8s
            if self.k8s_re.is_match(trimmed) {
                k8s_count += 1;
                // Check if inner log is JSON
                if let Some(caps) = self.k8s_re.captures(trimmed) {
                    if let Some(inner) = caps.get(4) {
                        let inner_str = inner.as_str().trim();
                        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(inner_str) {
                            for (k, _) in map {
                                field_set.insert(k);
                            }
                        }
                    }
                }
                continue;
            }

            // Test Apache Error
            if self.apache_error_re.is_match(trimmed) {
                apache_error_count += 1;
                continue;
            }

            // Test Nginx Error
            if self.nginx_error_re.is_match(trimmed) {
                nginx_error_count += 1;
                continue;
            }

            // Test Apache Combined / Common (also Nginx Access)
            if self.apache_combined_re.is_match(trimmed) || self.apache_common_re.is_match(trimmed) {
                apache_count += 1;
                continue;
            }

            // Test Syslog
            if self.syslog_re.is_match(trimmed) {
                syslog_count += 1;
                continue;
            }
        }

        let total = sample_lines.len().max(1);
        let detected = if json_single_count * 2 >= total
            || json_kv_count * 2 >= total
            || (starts_with_json_bracket && json_kv_count >= 2)
            || json_kv_count >= 5
        {
            LogFormat::Json
        } else if k8s_count * 2 >= total {
            LogFormat::Kubernetes
        } else if apache_error_count * 2 >= total {
            LogFormat::ApacheError
        } else if nginx_error_count * 2 >= total {
            LogFormat::NginxError
        } else if apache_count * 2 >= total {
            LogFormat::ApacheCombined
        } else if syslog_count * 2 >= total {
            LogFormat::Syslog
        } else {
            LogFormat::Raw
        };

        self.detected_format = detected;
        let token_limit = max_tokens.max(7).min(30);

        // Populate standard fields according to detected format
        match detected {
            LogFormat::Json => {
                let mut fields: Vec<String> = field_set.into_iter().collect();
                // Put common fields first
                let priority = ["time", "timestamp", "@timestamp", "level", "lvl", "severity", "status", "msg", "message"];
                fields.sort_by_key(|f| {
                    if let Some(pos) = priority.iter().position(|p| p.eq_ignore_ascii_case(f)) {
                        (0, pos, f.clone())
                    } else {
                        (1, 999, f.clone())
                    }
                });
                self.available_fields = fields.clone();
                self.visible_fields = self.available_fields.clone();
            }
            LogFormat::ApacheError => {
                let mut fields = vec![
                    "timestamp".into(),
                    "level".into(),
                    "module".into(),
                    "pid".into(),
                    "client".into(),
                    "message".into(),
                ];
                for i in 1..=token_limit {
                    fields.push(format!("field_{}", i));
                }
                self.available_fields = fields;
                self.visible_fields = vec![
                    "timestamp".into(),
                    "level".into(),
                    "message".into(),
                ];
            }
            LogFormat::NginxError => {
                let mut fields = vec![
                    "timestamp".into(),
                    "level".into(),
                    "pid".into(),
                    "tid".into(),
                    "cid".into(),
                    "client".into(),
                    "server".into(),
                    "request".into(),
                    "upstream".into(),
                    "host".into(),
                    "message".into(),
                ];
                for i in 1..=token_limit {
                    fields.push(format!("field_{}", i));
                }
                self.available_fields = fields;
                self.visible_fields = vec![
                    "timestamp".into(),
                    "level".into(),
                    "client".into(),
                    "message".into(),
                ];
            }
            LogFormat::ApacheCombined | LogFormat::ApacheCommon => {
                self.available_fields = vec![
                    "ip".into(),
                    "user".into(),
                    "timestamp".into(),
                    "request".into(),
                    "status".into(),
                    "size".into(),
                    "referer".into(),
                    "user_agent".into(),
                ];
                self.visible_fields = vec![
                    "ip".into(),
                    "user".into(),
                    "timestamp".into(),
                    "request".into(),
                    "status".into(),
                    "size".into(),
                    "referer".into(),
                    "user_agent".into(),
                ];
            }
            LogFormat::Kubernetes => {
                let mut fields = vec![
                    "timestamp".into(),
                    "stream".into(),
                    "flag".into(),
                    "message".into(),
                ];
                for f in field_set {
                    fields.push(f);
                }
                for i in 1..=token_limit {
                    fields.push(format!("field_{}", i));
                }
                self.available_fields = fields;
                self.visible_fields = vec![
                    "timestamp".into(),
                    "stream".into(),
                    "flag".into(),
                    "message".into(),
                ];
            }
            LogFormat::Syslog => {
                let mut fields = vec![
                    "timestamp".into(),
                    "host".into(),
                    "service".into(),
                    "pid".into(),
                    "message".into(),
                ];
                for i in 1..=token_limit {
                    fields.push(format!("field_{}", i));
                }
                self.available_fields = fields;
                self.visible_fields = vec![
                    "timestamp".into(),
                    "host".into(),
                    "service".into(),
                    "pid".into(),
                    "message".into(),
                ];
            }
            LogFormat::Raw | LogFormat::Auto => {
                let mut fields = vec!["message".into()];
                for i in 1..=token_limit {
                    fields.push(format!("field_{}", i));
                }
                self.available_fields = fields;
                self.visible_fields = vec!["message".into()];
            }
        }
    }

    pub fn parse_line(&self, line: &str) -> ParsedRecord {
        let fmt = self.active_format();
        match fmt {
            LogFormat::Json => self.parse_json(line),
            LogFormat::Kubernetes => self.parse_kubernetes(line),
            LogFormat::ApacheCombined | LogFormat::ApacheCommon => self.parse_apache(line),
            LogFormat::ApacheError => self.parse_apache_error(line),
            LogFormat::NginxError => self.parse_nginx_error(line),
            LogFormat::Syslog => self.parse_syslog(line),
            LogFormat::Raw | LogFormat::Auto => self.parse_raw(line),
        }
    }

    fn parse_json(&self, line: &str) -> ParsedRecord {
        let trimmed = line.trim();
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(trimmed) {
            let mut record = ParsedRecord::default();
            let mut fields = Vec::new();

            for (k, v) in map {
                let str_val = match &v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => v.to_string(),
                };

                let k_lower = k.to_lowercase();
                if record.timestamp.is_none()
                    && (k_lower == "time" || k_lower == "timestamp" || k_lower == "@timestamp" || k_lower == "ts")
                {
                    record.timestamp = Some(str_val.clone());
                } else if record.level.is_none()
                    && (k_lower == "level" || k_lower == "lvl" || k_lower == "severity" || k_lower == "status_level")
                {
                    record.level = Some(str_val.to_uppercase());
                } else if record.status.is_none() && (k_lower == "status" || k_lower == "status_code" || k_lower == "code") {
                    if let Ok(code) = str_val.parse::<u16>() {
                        record.status = Some(code);
                    }
                } else if record.message.is_empty()
                    && (k_lower == "message" || k_lower == "msg" || k_lower == "log" || k_lower == "error")
                {
                    record.message = str_val.clone();
                }

                fields.push(ParsedField {
                    key: k,
                    value: str_val,
                });
            }

            if record.message.is_empty() {
                record.message = trimmed.to_string();
            }

            record.fields = fields;
            record.tokens = parse_line_tokens(line);
            record
        } else if let Some(caps) = self.json_kv_re.captures(trimmed) {
            let key = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let raw_val = caps.get(2).map(|m| m.as_str()).unwrap_or_default().trim();
            let clean_val = raw_val
                .trim_end_matches(',')
                .trim_matches('"')
                .to_string();

            let mut record = ParsedRecord::default();
            let k_lower = key.to_lowercase();
            if k_lower == "time" || k_lower == "timestamp" || k_lower == "@timestamp" || k_lower == "ts" {
                record.timestamp = Some(clean_val.clone());
            } else if k_lower == "level" || k_lower == "lvl" || k_lower == "severity" {
                record.level = Some(clean_val.to_uppercase());
            } else if k_lower == "status" || k_lower == "status_code" || k_lower == "code" {
                if let Ok(code) = clean_val.parse::<u16>() {
                    record.status = Some(code);
                }
            }

            record.message = line.to_string();
            record.fields = vec![ParsedField {
                key,
                value: clean_val,
            }];
            record.tokens = parse_line_tokens(line);
            record
        } else {
            self.parse_raw(line)
        }
    }

    pub fn inspect_field(&self, field_name: &str, sample_lines: &[String]) -> FieldInspectionResult {
        let mut counts = std::collections::HashMap::new();
        let mut total_occurrences = 0;

        let clean_field = field_name.trim();
        let pattern = format!(
            r#"(?:"{0}"|{0})\s*[:=]\s*(?:"([^"]*)"|([^,\s\}}\];]+))"#,
            regex::escape(clean_field)
        );
        let re = Regex::new(&pattern).ok();

        for line in sample_lines {
            // 1. Try parsed record
            let parsed = self.parse_line(line);
            if let Some(val) = parsed.get_field_value(clean_field) {
                let trimmed_val = val.trim();
                if !trimmed_val.is_empty() {
                    *counts.entry(trimmed_val.to_string()).or_insert(0) += 1;
                    total_occurrences += 1;
                    continue;
                }
            }

            // 2. Fallback to regex scan
            if let Some(ref r) = re {
                if let Some(caps) = r.captures(line) {
                    let val = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str().trim()).unwrap_or("");
                    if !val.is_empty() {
                        *counts.entry(val.to_string()).or_insert(0) += 1;
                        total_occurrences += 1;
                    }
                }
            }
        }

        let mut values: Vec<FieldValueCount> = counts
            .into_iter()
            .map(|(val, count)| {
                let pct = if total_occurrences > 0 {
                    (count as f32 / total_occurrences as f32) * 100.0
                } else {
                    0.0
                };
                FieldValueCount {
                    value: val,
                    count,
                    percentage: pct,
                }
            })
            .collect();

        values.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));

        FieldInspectionResult {
            field_name: clean_field.to_string(),
            total_occurrences,
            unique_count: values.len(),
            values,
        }
    }

    fn parse_kubernetes(&self, line: &str) -> ParsedRecord {
        if let Some(caps) = self.k8s_re.captures(line) {
            let ts = caps.get(1).map(|m| m.as_str().to_string());
            let stream = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let flag = caps.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();
            let msg = caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default();

            let mut record = ParsedRecord {
                timestamp: ts.clone(),
                level: Some(stream.to_uppercase()),
                status: None,
                message: msg.clone(),
                fields: vec![
                    ParsedField {
                        key: "timestamp".into(),
                        value: ts.unwrap_or_default(),
                    },
                    ParsedField {
                        key: "stream".into(),
                        value: stream,
                    },
                    ParsedField {
                        key: "flag".into(),
                        value: flag,
                    },
                    ParsedField {
                        key: "message".into(),
                        value: msg.clone(),
                    },
                ],
                tokens: parse_line_tokens(line),
            };

            // If message itself is JSON, also parse inner JSON fields
            let trimmed_msg = msg.trim();
            if trimmed_msg.starts_with('{') {
                if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(trimmed_msg) {
                    for (k, v) in map {
                        let str_val = match &v {
                            Value::String(s) => s.clone(),
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => v.to_string(),
                        };
                        let k_lower = k.to_lowercase();
                        if record.level.as_deref() == Some("STDOUT")
                            && (k_lower == "level" || k_lower == "lvl")
                        {
                            record.level = Some(str_val.to_uppercase());
                        }
                        if record.status.is_none() && (k_lower == "status" || k_lower == "status_code") {
                            if let Ok(code) = str_val.parse::<u16>() {
                                record.status = Some(code);
                            }
                        }
                        record.fields.push(ParsedField {
                            key: k,
                            value: str_val,
                        });
                    }
                }
            }

            record
        } else {
            self.parse_raw(line)
        }
    }

    fn parse_apache(&self, line: &str) -> ParsedRecord {
        let tokens = parse_line_tokens(line);
        if let Some(caps) = self.apache_combined_re.captures(line) {
            let ip = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let user = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let ts = caps.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();
            let req = caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default();
            let status_code = caps.get(5).and_then(|m| m.as_str().parse::<u16>().ok());
            let size = caps.get(6).map(|m| m.as_str().to_string()).unwrap_or_default();
            let referer = caps.get(7).map(|m| m.as_str().to_string()).unwrap_or_default();
            let ua = caps.get(8).map(|m| m.as_str().to_string()).unwrap_or_default();

            let level = match status_code {
                Some(s) if s >= 500 => Some("ERROR".into()),
                Some(s) if s >= 400 => Some("WARN".into()),
                Some(_) => Some("INFO".into()),
                None => None,
            };

            let full_msg = if !referer.is_empty() || !ua.is_empty() {
                format!("{} {} \"{}\" \"{}\"", req, size, referer, ua)
            } else {
                format!("{} {}", req, size)
            };

            let mut fields = vec![
                ParsedField { key: "ip".into(), value: ip },
                ParsedField { key: "user".into(), value: user },
                ParsedField { key: "timestamp".into(), value: ts.clone() },
                ParsedField { key: "request".into(), value: req },
                ParsedField { key: "status".into(), value: status_code.map(|s| s.to_string()).unwrap_or_default() },
                ParsedField { key: "size".into(), value: size },
                ParsedField { key: "referer".into(), value: referer },
                ParsedField { key: "user_agent".into(), value: ua },
            ];
            for (i, tok) in tokens.iter().enumerate() {
                fields.push(ParsedField {
                    key: format!("field_{}", i + 1),
                    value: tok.clone(),
                });
            }

            ParsedRecord {
                timestamp: Some(ts),
                level,
                status: status_code,
                message: full_msg,
                fields,
                tokens,
            }
        } else if let Some(caps) = self.apache_common_re.captures(line) {
            let ip = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let user = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let ts = caps.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();
            let req = caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default();
            let status_code = caps.get(5).and_then(|m| m.as_str().parse::<u16>().ok());
            let size = caps.get(6).map(|m| m.as_str().to_string()).unwrap_or_default();

            let mut fields = vec![
                ParsedField { key: "ip".into(), value: ip },
                ParsedField { key: "user".into(), value: user },
                ParsedField { key: "timestamp".into(), value: ts.clone() },
                ParsedField { key: "request".into(), value: req.clone() },
                ParsedField { key: "status".into(), value: status_code.map(|s| s.to_string()).unwrap_or_default() },
                ParsedField { key: "size".into(), value: size.clone() },
            ];
            for (i, tok) in tokens.iter().enumerate() {
                fields.push(ParsedField {
                    key: format!("field_{}", i + 1),
                    value: tok.clone(),
                });
            }

            ParsedRecord {
                timestamp: Some(ts),
                level: match status_code {
                    Some(s) if s >= 500 => Some("ERROR".into()),
                    Some(s) if s >= 400 => Some("WARN".into()),
                    _ => Some("INFO".into()),
                },
                status: status_code,
                message: format!("{} {}", req, size),
                fields,
                tokens,
            }
        } else if self.nginx_error_re.is_match(line) {
            self.parse_nginx_error(line)
        } else {
            self.parse_raw(line)
        }
    }

    fn parse_apache_error(&self, line: &str) -> ParsedRecord {
        let tokens = parse_line_tokens(line);
        if let Some(caps) = self.apache_error_re.captures(line) {
            let ts = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let module = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let level_str = caps.get(3).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
            let pid = caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default();
            let client = caps.get(5).map(|m| m.as_str().to_string()).unwrap_or_default();
            let msg = caps.get(6).map(|m| m.as_str().to_string()).unwrap_or_default();

            let mut fields = vec![
                ParsedField { key: "timestamp".into(), value: ts.clone() },
                ParsedField { key: "level".into(), value: level_str.clone() },
                ParsedField { key: "module".into(), value: module.clone() },
                ParsedField { key: "pid".into(), value: pid.clone() },
                ParsedField { key: "client".into(), value: client.clone() },
                ParsedField { key: "message".into(), value: msg.clone() },
            ];

            for (i, tok) in tokens.iter().enumerate() {
                fields.push(ParsedField {
                    key: format!("field_{}", i + 1),
                    value: tok.clone(),
                });
            }

            ParsedRecord {
                timestamp: Some(ts),
                level: Some(level_str),
                status: None,
                message: msg,
                fields,
                tokens,
            }
        } else {
            let mut rec = self.parse_raw(line);
            rec.tokens = tokens;
            rec
        }
    }

    fn parse_nginx_error(&self, line: &str) -> ParsedRecord {
        let tokens = parse_line_tokens(line);
        if let Some(caps) = self.nginx_error_re.captures(line) {
            let ts = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let level_raw = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let level_str = level_raw.to_uppercase();
            let pid = caps.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();
            let tid = caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default();
            let cid = caps.get(5).map(|m| m.as_str().to_string()).unwrap_or_default();
            let full_msg = caps.get(6).map(|m| m.as_str()).unwrap_or_default();

            static KV_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
                Regex::new(r#"(?:^|,)\s*(client|server|request|upstream|host):\s*(?:"([^"]*)"|([^\s,]+))"#).unwrap()
            });

            let mut client = String::new();
            let mut server = String::new();
            let mut request = String::new();
            let mut upstream = String::new();
            let mut host = String::new();

            for kv_cap in KV_RE.captures_iter(full_msg) {
                let key = kv_cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let val = kv_cap.get(2).or_else(|| kv_cap.get(3)).map(|m| m.as_str()).unwrap_or("");
                match key {
                    "client" => client = val.to_string(),
                    "server" => server = val.to_string(),
                    "request" => request = val.to_string(),
                    "upstream" => upstream = val.to_string(),
                    "host" => host = val.to_string(),
                    _ => {}
                }
            }

            let clean_msg = if let Some(idx) = full_msg.find(", client: ") {
                full_msg[..idx].trim().to_string()
            } else {
                full_msg.trim().to_string()
            };

            let mut fields = vec![
                ParsedField { key: "timestamp".into(), value: ts.clone() },
                ParsedField { key: "level".into(), value: level_str.clone() },
                ParsedField { key: "pid".into(), value: pid },
                ParsedField { key: "tid".into(), value: tid },
                ParsedField { key: "cid".into(), value: cid },
                ParsedField { key: "client".into(), value: client },
                ParsedField { key: "server".into(), value: server },
                ParsedField { key: "request".into(), value: request },
                ParsedField { key: "upstream".into(), value: upstream },
                ParsedField { key: "host".into(), value: host },
                ParsedField { key: "message".into(), value: clean_msg.clone() },
            ];

            for (i, tok) in tokens.iter().enumerate() {
                fields.push(ParsedField {
                    key: format!("field_{}", i + 1),
                    value: tok.clone(),
                });
            }

            ParsedRecord {
                timestamp: Some(ts),
                level: Some(level_str),
                status: None,
                message: clean_msg,
                fields,
                tokens,
            }
        } else {
            let mut rec = self.parse_raw(line);
            rec.tokens = tokens;
            rec
        }
    }

    fn parse_syslog(&self, line: &str) -> ParsedRecord {
        let tokens = parse_line_tokens(line);
        if let Some(caps) = self.syslog_re.captures(line) {
            let ts = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let host = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            let service = caps.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();
            let pid = caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default();
            let msg = caps.get(5).map(|m| m.as_str().to_string()).unwrap_or_default();

            let level = if msg.to_lowercase().contains("error") || msg.to_lowercase().contains("failed") {
                Some("ERROR".into())
            } else if msg.to_lowercase().contains("warn") {
                Some("WARN".into())
            } else {
                Some("INFO".into())
            };

            let mut fields = vec![
                ParsedField { key: "timestamp".into(), value: ts.clone() },
                ParsedField { key: "host".into(), value: host },
                ParsedField { key: "service".into(), value: service },
                ParsedField { key: "pid".into(), value: pid },
                ParsedField { key: "message".into(), value: msg.clone() },
            ];
            for (i, tok) in tokens.iter().enumerate() {
                fields.push(ParsedField {
                    key: format!("field_{}", i + 1),
                    value: tok.clone(),
                });
            }

            ParsedRecord {
                timestamp: Some(ts),
                level,
                status: None,
                message: msg,
                fields,
                tokens,
            }
        } else {
            self.parse_raw(line)
        }
    }

    fn parse_raw(&self, line: &str) -> ParsedRecord {
        let trimmed = line.trim();
        // Try extracting standard log level keyword if present
        static LEVEL: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(||
            Regex::new(r"(?i)\b(EMERG|EMERGENCY|ALERT|CRIT|CRITICAL|FATAL|ERROR|ERR|WARNING|WARN|NOTICE|INFO|DEBUG|TRACE[1-8]?)\b").unwrap());
        let level = LEVEL.find(trimmed).map(|m| m.as_str().to_uppercase());

        let tokens = parse_line_tokens(line);
        let mut fields = vec![ParsedField {
            key: "message".into(),
            value: line.to_string(),
        }];
        for (i, tok) in tokens.iter().enumerate() {
            fields.push(ParsedField {
                key: format!("field_{}", i + 1),
                value: tok.clone(),
            });
        }

        ParsedRecord {
            timestamp: None,
            level,
            status: None,
            message: line.to_string(),
            fields,
            tokens,
        }
    }

    pub fn toggle_field_visibility(&mut self, field_name: &str) {
        if let Some(pos) = self.visible_fields.iter().position(|f| f == field_name) {
            self.visible_fields.remove(pos);
        } else {
            self.visible_fields.push(field_name.to_string());
        }
    }

    pub fn solo_field(&mut self, field_name: &str) {
        self.visible_fields = vec![field_name.to_string()];
    }

    pub fn select_all_fields(&mut self) {
        self.visible_fields = self.available_fields.clone();
    }

    pub fn clear_all_fields(&mut self) {
        self.visible_fields.clear();
    }

    pub fn toggle_view_mode(&mut self) -> ViewMode {
        self.view_mode = match self.view_mode {
            ViewMode::CompactLine => ViewMode::ColumnsTable,
            ViewMode::ColumnsTable => ViewMode::CompactLine,
        };
        self.view_mode
    }
}
