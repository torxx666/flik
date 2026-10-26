use parking_lot::RwLock;
use regex::{Regex, RegexBuilder};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};

use crate::reader::LogReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FilterKind {
    Include,
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericOp {
    Gt,  // >
    Gte, // >=
    Lt,  // <
    Lte, // <=
    Eq,  // == or =
    Neq, // !=
}

#[derive(Debug, Clone)]
pub struct NumericFilterRule {
    pub key: String,
    pub op: NumericOp,
    pub target_value: f64,
    extractor_regex: Regex,
}

impl NumericFilterRule {
    pub fn new(key: String, op: NumericOp, target_value: f64) -> Result<Self, String> {
        let key_esc = regex::escape(&key);
        let pattern =
            format!(r#"(?i)(?:"{key_esc}"|{key_esc})\s*[:=]\s*"?[\$€£¥]?\s*(-?\d+(?:\.\d+)?)"?"#);
        let extractor_regex = RegexBuilder::new(&pattern)
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            key,
            op,
            target_value,
            extractor_regex,
        })
    }

    pub fn matches(&self, text: &str) -> bool {
        if let Some(caps) = self.extractor_regex.captures(text) {
            if let Some(m) = caps.get(1) {
                if let Ok(val) = m.as_str().parse::<f64>() {
                    return match self.op {
                        NumericOp::Gt => val > self.target_value,
                        NumericOp::Gte => val >= self.target_value,
                        NumericOp::Lt => val < self.target_value,
                        NumericOp::Lte => val <= self.target_value,
                        NumericOp::Eq => (val - self.target_value).abs() < 1e-9,
                        NumericOp::Neq => (val - self.target_value).abs() >= 1e-9,
                    };
                }
            }
        }
        false
    }
}

fn normalize_field_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let lower = lower.strip_prefix("log.").unwrap_or(&lower);
    match lower {
        "amout" | "ammount" => "amount".to_string(),
        "acc_id" | "account" => "account_id".to_string(),
        "txn_id" | "trans_id" | "tx_id" => "transaction_id".to_string(),
        "stat" => "status".to_string(),
        "msg" => "message".to_string(),
        _ => lower.to_string(),
    }
}

fn parse_numeric_filter(pattern: &str) -> Option<(String, NumericOp, f64)> {
    let clean = pattern
        .trim()
        .replace(": >=", ">=")
        .replace(":>=", ">=")
        .replace(": <=", "<=")
        .replace(":<=", "<=")
        .replace(": !=", "!=")
        .replace(":!=", "!=")
        .replace(": ==", "==")
        .replace(":==", "==")
        .replace(": >", ">")
        .replace(":>", ">")
        .replace(": <", "<")
        .replace(":<", "<")
        .replace(": =", "=");

    let ops = [
        (">=", NumericOp::Gte),
        ("<=", NumericOp::Lte),
        ("!=", NumericOp::Neq),
        ("==", NumericOp::Eq),
        (">", NumericOp::Gt),
        ("<", NumericOp::Lt),
        ("=", NumericOp::Eq),
    ];

    for (op_str, op) in ops {
        if let Some((left, right)) = clean.split_once(op_str) {
            let raw_key = left.trim().trim_matches('"').trim_matches('\'').trim();
            let key = normalize_field_name(raw_key);
            let mut val_str = right.trim().trim_matches('"').trim_matches('\'').trim();
            val_str = val_str
                .trim_matches(|c| c == '$' || c == '€' || c == '£' || c == '¥')
                .trim();

            if !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
            {
                if let Ok(num) = val_str.parse::<f64>() {
                    return Some((key, op, num));
                }
            }
        }
    }
    None
}

/// Smart find: //text, //10:text or //10,3:text (before,after).
pub fn smart_find_parts(pattern: &str) -> Result<Option<(usize, usize, &str)>, String> {
    let Some(rest) = pattern.strip_prefix("//") else { return Ok(None); };
    let (before, after, term) = if let Some((counts, term)) = rest.split_once(':') {
        if counts.chars().all(|c| c.is_ascii_digit() || c == ',') && !counts.is_empty() {
            let (before, after) = counts.split_once(',').unwrap_or((counts, "0"));
            (before.parse::<usize>().map_err(|_| "Use //10,3:text")?,
             after.parse::<usize>().map_err(|_| "Use //10,3:text")?, term)
        } else { (1, 0, rest) }
    } else { (1, 0, rest) };
    if before > 1000 || after > 1000 { return Err("Smart find context is limited to 1000 lines per side".into()); }
    if term.trim().is_empty() { return Err("Smart find needs a search term".into()); }
    Ok(Some((before, after, term)))
}

fn context_sizes(rules: &[FilterRule]) -> (usize, usize) {
    rules.iter().filter(|r| r.enabled && r.kind == FilterKind::Include && !r.is_regex)
        .filter_map(|r| smart_find_parts(&r.pattern).ok().flatten())
        .fold((0, 0), |(b, a), (before, after, _)| (b.max(before), a.max(after)))
}

fn merge_indices(target: &mut Vec<usize>, additions: &[usize]) {
    if additions.is_empty() { return; }
    let start = target.partition_point(|i| *i < additions[0]);
    let mut tail = target.split_off(start);
    tail.extend_from_slice(additions);
    tail.sort_unstable();
    tail.dedup();
    target.extend(tail);
}

fn append_context(indices: &mut Vec<usize>, index: usize, before: usize, after: usize, total: usize) {
    let start = index.saturating_sub(before).max(indices.last().map_or(0, |last| last + 1));
    let end = index.saturating_add(after).saturating_add(1).min(total);
    indices.extend(start..end);
}

#[derive(Debug, Clone)]
pub struct FilterRule {
    pub id: usize,
    pub pattern: String,
    pub kind: FilterKind,
    pub is_regex: bool,
    pub is_case_sensitive: bool,
    pub enabled: bool,
    expression: Option<Box<crate::expression::Expression>>,
    time_window: Option<(i64, i64)>,
    compiled_regex: Option<Regex>,
    numeric_filter: Option<NumericFilterRule>,
}

impl FilterRule {
    pub fn new(
        id: usize,
        pattern: String,
        kind: FilterKind,
        is_regex: bool,
        is_case_sensitive: bool,
    ) -> Result<Self, String> {
        if !is_regex && kind == FilterKind::Include {
            if let Some((_, _, term)) = smart_find_parts(&pattern)? {
                let mut rule = Self::new(id, term.to_string(), kind, false, is_case_sensitive)?;
                rule.pattern = pattern;
                return Ok(rule);
            }
        }
        if !is_regex {
            let expression = crate::expression::parse(&pattern, is_case_sensitive)?;
            let time_window = if let Some(window) = pattern.trim().strip_prefix("@time:") {
                let (start, end) = window
                    .split_once("..")
                    .ok_or("Use @time:START..END (RFC3339)")?;
                let start = chrono::DateTime::parse_from_rfc3339(start)
                    .map_err(|e| e.to_string())?
                    .timestamp_millis();
                let end = chrono::DateTime::parse_from_rfc3339(end)
                    .map_err(|e| e.to_string())?
                    .timestamp_millis();
                if start > end {
                    return Err("Time range starts after it ends".into());
                }
                Some((start, end))
            } else {
                None
            };
            if expression.is_some() || time_window.is_some() {
                return Ok(Self {
                    id,
                    pattern,
                    kind,
                    is_regex,
                    is_case_sensitive,
                    enabled: true,
                    expression: expression.map(Box::new),
                    time_window,
                    compiled_regex: None,
                    numeric_filter: None,
                });
            }
        }
        let pattern = pattern
            .trim()
            .trim_matches('\'')
            .trim_matches('"')
            .trim()
            .to_string();

        let (compiled, numeric) = if is_regex {
            let re = RegexBuilder::new(&pattern)
                .case_insensitive(!is_case_sensitive)
                .build()
                .map_err(|e| e.to_string())?;
            (Some(re), None)
        } else if let Some((key, op, target)) = parse_numeric_filter(&pattern) {
            let num_rule = NumericFilterRule::new(key, op, target)?;
            (None, Some(num_rule))
        } else if let Some((key, val)) = pattern.split_once(':') {
            // Smart Filter: field-based matching (e.g. status:5xx, level:error, service:auth)
            let key = key.trim().to_lowercase();
            let key = key.strip_prefix("log.").unwrap_or(&key);
            let val = val.trim().trim_matches('"').trim_matches('\'');

            let re_str = if key == "status" || key == "code" {
                match val.to_lowercase().as_str() {
                    "2xx" | "2*" => {
                        r#"(?:"status"\s*:\s*2\d{2}|"status_code"\s*:\s*2\d{2}|"\s+2\d{2}\s+)"#
                            .to_string()
                    }
                    "3xx" | "3*" => {
                        r#"(?:"status"\s*:\s*3\d{2}|"status_code"\s*:\s*3\d{2}|"\s+3\d{2}\s+)"#
                            .to_string()
                    }
                    "4xx" | "4*" => {
                        r#"(?:"status"\s*:\s*4\d{2}|"status_code"\s*:\s*4\d{2}|"\s+4\d{2}\s+)"#
                            .to_string()
                    }
                    "5xx" | "5*" => {
                        r#"(?:"status"\s*:\s*5\d{2}|"status_code"\s*:\s*5\d{2}|"\s+5\d{2}\s+)"#
                            .to_string()
                    }
                    code => {
                        let c = regex::escape(code);
                        format!(r#"(?:"status"\s*:\s*{c}|"status_code"\s*:\s*{c}|"\s+{c}\s+)"#)
                    }
                }
            } else if key == "level" || key == "lvl" || key == "severity" {
                match val.to_lowercase().as_str() {
                    "error" | "err" | "fatal" | "critical" => {
                        r#"(?i)(?:\[(?:ERROR|ERR|FATAL|CRITICAL)\]|"level"\s*:\s*"(?:error|err|fatal|critical)"|level=(?:error|err|fatal|critical)|\b(ERROR|FATAL|CRITICAL)\b)"#.to_string()
                    }
                    "warn" | "warning" => {
                        r#"(?i)(?:\[(?:WARN|WARNING)\]|"level"\s*:\s*"(?:warn|warning)"|level=(?:warn|warning)|\b(WARN|WARNING)\b)"#.to_string()
                    }
                    "info" => {
                        r#"(?i)(?:\[INFO\]|"level"\s*:\s*"info"|level=info|\bINFO\b)"#.to_string()
                    }
                    "debug" => {
                        r#"(?i)(?:\[DEBUG\]|"level"\s*:\s*"debug"|level=debug|\bDEBUG\b)"#.to_string()
                    }
                    other => {
                        let o = regex::escape(other);
                        format!(r#"(?i)(?:\[{o}\]|"level"\s*:\s*"{o}"|level={o}|\b{o}\b)"#)
                    }
                }
            } else {
                // Key-value pair: e.g. service:auth or user_id:123
                let k_esc = regex::escape(&key);
                let v_esc = regex::escape(val);
                format!(
                    r#"(?i)(?:"{k_esc}"\s*:\s*"?{v_esc}"?|{k_esc}={v_esc}|\[{k_esc}:{v_esc}\])"#
                )
            };

            let compiled = RegexBuilder::new(&re_str)
                .case_insensitive(!is_case_sensitive)
                .build()
                .map_err(|e| e.to_string())?;
            (Some(compiled), None)
        } else {
            // Standalone pattern: smart level/status expansion if keyword matches log level
            let lower = pattern.to_lowercase();
            let re_str = match lower.as_str() {
                "error" | "fatal" | "critical" => {
                    r#"(?i)(?:\b(?:error|fatal|critical)\b|"\s+5\d{2}\s+|"status"\s*:\s*5\d{2}|"status_code"\s*:\s*5\d{2}|\bstatus=5\d{2}\b|\[(?:ERROR|FATAL|CRITICAL)\])"#.to_string()
                }
                "err" => {
                    r#"(?i)(?:\b(?:error|err|fatal|critical)\b|"\s+5\d{2}\s+|"status"\s*:\s*5\d{2}|"status_code"\s*:\s*5\d{2}|\bstatus=5\d{2}\b|\[(?:ERROR|ERR|FATAL|CRITICAL)\])"#.to_string()
                }
                "warn" | "warning" => {
                    r#"(?i)(?:\b(?:warn|warning)\b|"\s+4\d{2}\s+|"status"\s*:\s*4\d{2}|"status_code"\s*:\s*4\d{2}|\bstatus=4\d{2}\b|\[(?:WARN|WARNING)\])"#.to_string()
                }
                "info" => {
                    r#"(?i)(?:\binfo\b|"\s+2\d{2}\s+|"status"\s*:\s*2\d{2}|"status_code"\s*:\s*2\d{2}|\bstatus=2\d{2}\b|\[INFO\])"#.to_string()
                }
                _ => regex::escape(&pattern),
            };

            let compiled = RegexBuilder::new(&re_str)
                .case_insensitive(!is_case_sensitive)
                .build()
                .map_err(|e| e.to_string())?;
            (Some(compiled), None)
        };

        Ok(Self {
            id,
            pattern,
            kind,
            is_regex,
            is_case_sensitive,
            enabled: true,
            expression: None,
            time_window: None,
            compiled_regex: compiled,
            numeric_filter: numeric,
        })
    }

    pub fn matches(&self, text: &str) -> bool {
        if !self.enabled {
            return false;
        }
        if let Some(ref expr) = self.expression {
            return expr.matches(text);
        }
        if let Some((start, end)) = self.time_window {
            return crate::analytics::extract_timestamp(text)
                .is_some_and(|time| (start..=end).contains(&time.timestamp_millis()));
        }
        if let Some(ref num) = self.numeric_filter {
            return num.matches(text);
        }
        if let Some(ref re) = self.compiled_regex {
            re.is_match(text)
        } else {
            false
        }
    }
}

pub struct FilterEngine {
    pub rules: Vec<FilterRule>,
    next_id: usize,
    reader_revision: u64,
    pub visible_fields: Option<Vec<String>>,
    pub available_fields: Option<Vec<String>>,
    pub is_json_filter_active: bool,
    /// Indices in LogReader that match all enabled rules
    pub filtered_indices: Arc<RwLock<Vec<usize>>>,
    pub is_filtering: Arc<AtomicBool>,
    pub cancel_signal: Arc<AtomicBool>,
    pub scanned_lines: Arc<AtomicUsize>,
    filter_thread: Option<JoinHandle<()>>,
}

impl FilterEngine {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            next_id: 1,
            reader_revision: 0,
            visible_fields: None,
            available_fields: None,
            is_json_filter_active: false,
            filtered_indices: Arc::new(RwLock::new(Vec::new())),
            is_filtering: Arc::new(AtomicBool::new(false)),
            cancel_signal: Arc::new(AtomicBool::new(false)),
            scanned_lines: Arc::new(AtomicUsize::new(0)),
            filter_thread: None,
        }
    }

    pub fn restore_rules(&mut self, rules: Vec<FilterRule>) {
        self.next_id = rules.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        self.rules = rules;
    }

    pub fn has_active_filters(&self) -> bool {
        self.rules.iter().any(|r| r.enabled) || self.has_active_field_filter()
    }

    pub fn has_active_field_filter(&self) -> bool {
        self.is_json_filter_active
    }

    pub fn sync_field_filter(
        &mut self,
        visible: &[String],
        available: &[String],
        is_json: bool,
        reader: &LogReader,
    ) {
        if !is_json || available.is_empty() {
            self.visible_fields = None;
            self.available_fields = None;
            self.is_json_filter_active = false;
        } else {
            let is_custom = !visible.is_empty() && visible.len() < available.len();
            if is_custom {
                self.visible_fields = Some(visible.to_vec());
                self.available_fields = Some(available.to_vec());
                self.is_json_filter_active = true;
            } else {
                self.visible_fields = None;
                self.available_fields = None;
                self.is_json_filter_active = false;
            }
        }
        self.trigger_refilter(reader);
    }

    pub fn add_rule(
        &mut self,
        pattern: String,
        kind: FilterKind,
        is_regex: bool,
        is_case_sensitive: bool,
        reader: &LogReader,
    ) -> Result<usize, String> {
        let id = self.next_id;
        self.next_id += 1;
        let rule = FilterRule::new(id, pattern, kind, is_regex, is_case_sensitive)?;
        self.rules.push(rule);
        self.trigger_refilter(reader);
        Ok(id)
    }

    pub fn toggle_rule(&mut self, rule_id: usize, reader: &LogReader) {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == rule_id) {
            rule.enabled = !rule.enabled;
            self.trigger_refilter(reader);
        }
    }

    pub fn remove_rule(&mut self, rule_id: usize, reader: &LogReader) {
        self.rules.retain(|r| r.id != rule_id);
        self.trigger_refilter(reader);
    }

    pub fn pop_last_rule(&mut self, reader: &LogReader) -> Option<FilterRule> {
        let popped = self.rules.pop();
        if popped.is_some() {
            self.trigger_refilter(reader);
        }
        popped
    }

    pub fn clear_rules(&mut self, reader: &LogReader) {
        self.rules.clear();
        self.trigger_refilter(reader);
    }

    #[allow(dead_code)]
    pub fn line_matches(&self, line: &str) -> bool {
        let mut has_includes = false;
        let mut matches_all_includes = true;

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            match rule.kind {
                FilterKind::Exclude => {
                    if rule.matches(line) {
                        return false;
                    }
                }
                FilterKind::Include => {
                    has_includes = true;
                    if !rule.matches(line) {
                        matches_all_includes = false;
                    }
                }
            }
        }

        if has_includes {
            if !matches_all_includes {
                return false;
            }
        }

        if self.has_active_field_filter() {
            if let Some(ref vis) = self.visible_fields {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return false;
                }
                // Discard container punctuation
                if trimmed == "{"
                    || trimmed == "}"
                    || trimmed == "},"
                    || trimmed == "["
                    || trimmed == "]"
                    || trimmed == "],"
                {
                    return false;
                }
                // Single-line JSON or line containing { ... }
                if trimmed.starts_with('{') {
                    let has_any = vis.iter().any(|vf| {
                        let p1 = format!("\"{}\":", vf);
                        let p2 = format!("\"{}\" :", vf);
                        let p3 = format!("\"{}\":", vf.to_lowercase());
                        trimmed.contains(&p1)
                            || trimmed.contains(&p2)
                            || trimmed.to_lowercase().contains(&p3)
                    });
                    if !has_any {
                        return false;
                    }
                } else if let Some(pos) = trimmed.find(':') {
                    let left = trimmed[..pos].trim();
                    let key = left
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim_start_matches('{')
                        .trim();
                    if !key.is_empty() && !vis.iter().any(|v| v.eq_ignore_ascii_case(key)) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        true
    }

    pub fn trigger_refilter(&mut self, reader: &LogReader) {
        self.reader_revision = reader.revision.load(Ordering::Acquire);
        // Cancel existing background filter job
        self.cancel_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.filter_thread.take() {
            let _ = handle.join();
        }

        if !self.has_active_filters() {
            self.filtered_indices.write().clear();
            self.is_filtering.store(false, Ordering::Relaxed);
            self.scanned_lines.store(0, Ordering::Relaxed);
            return;
        }

        self.cancel_signal = Arc::new(AtomicBool::new(false));
        self.is_filtering = Arc::new(AtomicBool::new(true));
        self.scanned_lines = Arc::new(AtomicUsize::new(0));

        let thread_cancel = Arc::clone(&self.cancel_signal);
        let thread_filtering = Arc::clone(&self.is_filtering);
        let thread_scanned = Arc::clone(&self.scanned_lines);
        let thread_indices: Arc<RwLock<Vec<usize>>> = Arc::clone(&self.filtered_indices);
        let thread_reader = reader.clone();
        let rules_snapshot = self.rules.clone();
        let visible_snapshot = self.visible_fields.clone();
        let is_json_filter_active = self.is_json_filter_active;

        self.filter_thread = Some(thread::spawn(move || {
            let mut matched = Vec::with_capacity(1000);
            let chunk_size = 500;
            let mut current = 0;
            let (before_context, after_context) = context_sizes(&rules_snapshot);
            let mut pending_after: Option<usize> = None;

            let has_field_filter = is_json_filter_active;
            let vis_set: std::collections::HashSet<String> = visible_snapshot
                .as_ref()
                .map(|v| v.iter().cloned().collect())
                .unwrap_or_default();
            let _vis_len = visible_snapshot.as_ref().map(|v| v.len()).unwrap_or(0);

            // Clear old filtered list
            thread_indices.write().clear();

            loop {
                if thread_cancel.load(Ordering::Relaxed) {
                    thread_filtering.store(false, Ordering::Relaxed);
                    return;
                }

                let total = thread_reader.line_count();
                if current >= total {
                    if thread_reader.is_indexing.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(15));
                        continue;
                    } else {
                        break;
                    }
                }

                let lines = thread_reader.read_range(current, chunk_size);
                if lines.is_empty() {
                    if thread_reader.is_indexing.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(15));
                        continue;
                    } else {
                        break;
                    }
                }

                let read_count = lines.len();
                for (idx, line) in lines {
                    if pending_after.is_some_and(|end| idx <= end) {
                        append_context(&mut matched, idx, 0, 0, total);
                    }
                    let mut has_includes = false;
                    let mut matches_all_includes = true;
                    let mut excluded = false;

                    for rule in &rules_snapshot {
                        if !rule.enabled {
                            continue;
                        }
                        match rule.kind {
                            FilterKind::Exclude => {
                                if rule.matches(&line) {
                                    excluded = true;
                                    break;
                                }
                            }
                            FilterKind::Include => {
                                has_includes = true;
                                if !rule.matches(&line) {
                                    matches_all_includes = false;
                                }
                            }
                        }
                    }

                    if !excluded && (!has_includes || matches_all_includes) {
                        if has_field_filter {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                continue;
                            }
                            if trimmed == "{"
                                || trimmed == "}"
                                || trimmed == "},"
                                || trimmed == "["
                                || trimmed == "]"
                                || trimmed == "],"
                            {
                                continue;
                            }
                            if trimmed.starts_with('{') {
                                let has_any = vis_set.iter().any(|vf| {
                                    let p1 = format!("\"{}\":", vf);
                                    let p2 = format!("\"{}\" :", vf);
                                    let p3 = format!("\"{}\":", vf.to_lowercase());
                                    trimmed.contains(&p1)
                                        || trimmed.contains(&p2)
                                        || trimmed.to_lowercase().contains(&p3)
                                });
                                if !has_any {
                                    continue;
                                }
                            } else if let Some(pos) = trimmed.find(':') {
                                let left = trimmed[..pos].trim();
                                let key = left
                                    .trim_matches('"')
                                    .trim_matches('\'')
                                    .trim_start_matches('{')
                                    .trim();
                                if !key.is_empty()
                                    && !vis_set.iter().any(|v| v.eq_ignore_ascii_case(key))
                                {
                                    continue;
                                }
                            } else {
                                continue;
                            }
                        }
                        append_context(&mut matched, idx, before_context, after_context, total);
                        pending_after = Some(idx.saturating_add(after_context));
                    }
                }

                current += read_count;
                thread_scanned.store(current, Ordering::Relaxed);

                // Progressively update filtered indices so UI shows early matches instantly
                if matched.len() >= 500 || current >= total {
                    let mut w = thread_indices.write();
                    merge_indices(&mut w, &matched);
                    matched.clear();
                }
            }

            if !matched.is_empty() {
                let mut w = thread_indices.write();
                merge_indices(&mut w, &matched);
            }

            thread_scanned.store(current, Ordering::Relaxed);
            thread_filtering.store(false, Ordering::Relaxed);
        }));
    }

    /// Evaluates newly appended stream lines against active filters in real-time.
    /// Returns the number of newly matched lines appended to filtered_indices.
    pub fn process_incoming_lines(&mut self, reader: &LogReader) -> usize {
        if !self.has_active_filters() {
            return 0;
        }
        if reader.revision.load(Ordering::Acquire) != self.reader_revision {
            self.trigger_refilter(reader);
            return 0;
        }
        if self.is_filtering.load(Ordering::Relaxed) {
            return 0;
        }

        let total = reader.line_count();
        let scanned = self.scanned_lines.load(Ordering::Relaxed);
        if total < scanned {
            // File was truncated or rotated
            self.trigger_refilter(reader);
            return 0;
        }
        if total == scanned {
            return 0;
        }

        let chunk_size = 1000;
        let mut newly_matched = Vec::new();
        let (before_context, after_context) = context_sizes(&self.rules);
        // Revisit pending after-context when a matching line was at the live edge.
        let mut cur = scanned.saturating_sub(after_context);

        while cur < total && cur < scanned.saturating_add(chunk_size) {
            let count = (total - cur).min(chunk_size);
            let lines = reader.read_range(cur, count);
            if lines.is_empty() {
                break;
            }
            let read_count = lines.len();
            for (idx, line) in lines {
                if self.line_matches(&line) {
                    append_context(&mut newly_matched, idx, before_context, after_context, total);
                }
            }
            cur += read_count;
        }

        let mut added_count = 0;
        if !newly_matched.is_empty() {
            let mut w = self.filtered_indices.write();
            let previous = w.len();
            merge_indices(&mut w, &newly_matched);
            added_count = w.len() - previous;
        }
        self.scanned_lines.store(cur, Ordering::Relaxed);
        added_count
    }

    pub fn filtered_count(&self, reader: &LogReader) -> usize {
        if self.has_active_filters() {
            self.filtered_indices.read().len()
        } else {
            reader.line_count()
        }
    }

    pub fn get_original_line_index(
        &self,
        display_index: usize,
        reader: &LogReader,
    ) -> Option<usize> {
        if self.has_active_filters() {
            let indices = self.filtered_indices.read();
            indices.get(display_index).copied()
        } else {
            if display_index < reader.line_count() {
                Some(display_index)
            } else {
                None
            }
        }
    }
}

impl Drop for FilterEngine {
    fn drop(&mut self) {
        self.cancel_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.filter_thread.take() {
            let _ = handle.join();
        }
    }
}
