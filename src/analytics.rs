use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;
use chrono::{DateTime, NaiveDateTime, Utc};
use regex::Regex;

use crate::format::FormatManager;
use crate::reader::LogReader;

static IPV4_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap());
static UUID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b").unwrap());
static HEX_HASH_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b[0-9a-fA-F]{16,64}\b").unwrap());
static ISO_TIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?").unwrap());
static APACHE_TIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\d{2}/[A-Za-z]{3}/\d{4}:\d{2}:\d{2}:\d{2} [+-]\d{4}\]").unwrap());
static NGINX_TIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2}").unwrap());
static NUM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d+(?:\.\d+)?\b").unwrap());
static CORRELATION_KEY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)"?(trace_id|request_id|req_id|txn_id|transaction_id|account_id|acc_id|user_id|order_id|session_id|correlation_id|span_id)"?\s*[:=]\s*"?([a-zA-Z0-9_\-\.]+)"?"#).unwrap());

// -----------------------------------------------------------------------------
// 1. Stats Dashboard Aggregator
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatsSummary {
    pub total_analyzed: usize,
    pub level_counts: BTreeMap<String, usize>,
    pub status_total: usize,
    pub status_counts: BTreeMap<u16, usize>, // e.g. 200 => 450, 500 => 32
    pub status_groups: [usize; 4],          // [2xx, 3xx, 4xx, 5xx]
    pub top_ips: Vec<(String, usize)>,
    pub top_endpoints: Vec<(String, usize)>,
    pub top_errors: Vec<(String, usize)>,
    pub avg_latency_ms: Option<f64>,
    pub p95_latency_ms: Option<f64>,
}

impl StatsSummary {
    pub fn compute(reader: &LogReader, max_samples: usize) -> Self {
        let total_lines = reader.line_count();
        let sample_count = total_lines.min(max_samples).max(1);
        let lines = reader.read_range(0, sample_count);

        let mut level_counts = BTreeMap::new();
        let mut status_counts: HashMap<u16, usize> = HashMap::new();
        let mut ip_counts: HashMap<String, usize> = HashMap::new();
        let mut endpoint_counts: HashMap<String, usize> = HashMap::new();
        let mut error_counts: HashMap<String, usize> = HashMap::new();
        let mut latencies: Vec<f64> = Vec::new();

        let mut format_mgr = FormatManager::new();
        let sample_texts: Vec<String> = lines.iter().take(50).map(|(_, l)| l.clone()).collect();
        format_mgr.detect_and_index_fields(&sample_texts);

        for (_, line) in &lines {
            let record = format_mgr.parse_line(line);

            if let Some(level) = record.level.as_deref() {
                *level_counts.entry(level.to_uppercase()).or_insert(0) += 1;
            }
            // 1. Status code
            if let Some(status) = record.status {
                *status_counts.entry(status).or_insert(0) += 1;
            }

            // 2. IP address
            if let Some(ip) = record.get_field_value("ip").or_else(|| record.get_field_value("client_ip")) {
                if !ip.is_empty() && ip != "-" {
                    *ip_counts.entry(ip.to_string()).or_insert(0) += 1;
                }
            } else if let Some(m) = IPV4_RE.find(line) {
                *ip_counts.entry(m.as_str().to_string()).or_insert(0) += 1;
            }

            // 3. Endpoint / Request
            if let Some(req) = record.get_field_value("request").or_else(|| record.get_field_value("path")) {
                let clean_req = req.split_whitespace().nth(1).unwrap_or(req);
                *endpoint_counts.entry(clean_req.to_string()).or_insert(0) += 1;
            }

            // 4. Errors
            let is_error_level = record.level.as_deref().is_some_and(|level| {
                level.eq_ignore_ascii_case("ERROR")
                    || level.eq_ignore_ascii_case("FATAL")
                    || level.eq_ignore_ascii_case("CRITICAL")
                    || level.eq_ignore_ascii_case("CRIT")
                    || level.eq_ignore_ascii_case("ERR")
                    || level.eq_ignore_ascii_case("ALERT")
                    || level.eq_ignore_ascii_case("EMERG")
                    || level.eq_ignore_ascii_case("EMERGENCY")
            });
            let is_server_error = record.status.is_some_and(|status| status >= 500);
            if is_error_level || is_server_error {
                let err_msg = record
                    .get_field_value("error")
                    .or_else(|| record.get_field_value("msg"))
                    .or_else(|| record.get_field_value("message"))
                    .unwrap_or(&record.message);
                let short_err: String = err_msg.chars().take(120).collect();
                if !short_err.trim().is_empty() {
                    *error_counts.entry(short_err).or_insert(0) += 1;
                }
            }

            // 5. Latency / duration_ms
            if let Some(dur_str) = record.get_field_value("duration_ms").or_else(|| record.get_field_value("latency")) {
                if let Ok(val) = dur_str.parse::<f64>() {
                    latencies.push(val);
                }
            }
        }

        // Aggregate 2xx, 3xx, 4xx, 5xx
        let status_total = status_counts.values().sum();
        let mut status_groups = [0usize; 4];
        for (&code, &cnt) in &status_counts {
            match code {
                200..=299 => status_groups[0] += cnt,
                300..=399 => status_groups[1] += cnt,
                400..=499 => status_groups[2] += cnt,
                500..=599 => status_groups[3] += cnt,
                _ => {}
            }
        }

        let mut top_ips: Vec<(String, usize)> = ip_counts.into_iter().collect();
        top_ips.sort_by(|a, b| b.1.cmp(&a.1));
        top_ips.truncate(10);

        let mut top_endpoints: Vec<(String, usize)> = endpoint_counts.into_iter().collect();
        top_endpoints.sort_by(|a, b| b.1.cmp(&a.1));
        top_endpoints.truncate(10);

        let mut top_errors: Vec<(String, usize)> = error_counts.into_iter().collect();
        top_errors.sort_by(|a, b| b.1.cmp(&a.1));
        top_errors.truncate(10);

        let (avg_latency_ms, p95_latency_ms) = if !latencies.is_empty() {
            latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let sum: f64 = latencies.iter().sum();
            let avg = sum / (latencies.len() as f64);
            let p95_idx = ((latencies.len() as f64) * 0.95).min((latencies.len() - 1) as f64) as usize;
            (Some(avg), Some(latencies[p95_idx]))
        } else {
            (None, None)
        };

        let sorted_status: BTreeMap<u16, usize> = status_counts.into_iter().collect();

        Self {
            total_analyzed: lines.len(),
            level_counts,
            status_total,
            status_counts: sorted_status,
            status_groups,
            top_ips,
            top_endpoints,
            top_errors,
            avg_latency_ms,
            p95_latency_ms,
        }
    }
}

// -----------------------------------------------------------------------------
// 2. Timeline Histogram
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HistogramBucket {
    pub label: String,
    pub total_count: usize,
    pub error_count: usize,
    pub warn_count: usize,
    pub first_line_idx: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimelineHistogram {
    pub buckets: Vec<HistogramBucket>,
    pub max_bucket_count: usize,
    pub total_errors: usize,
    pub total_scanned: usize,
}

impl TimelineHistogram {
    pub fn compute(reader: &LogReader, num_buckets: usize) -> Self {
        let total_lines = reader.line_count();
        if total_lines == 0 {
            return Self::default();
        }

        let bucket_count = num_buckets.max(1);
        let first_timestamp = reader.read_line(0).and_then(|line| extract_timestamp(&line));
        let last_timestamp = reader
            .read_line(total_lines - 1)
            .and_then(|line| extract_timestamp(&line));
        let time_range = first_timestamp.zip(last_timestamp).map(|(first, last)| {
            let first_ms = first.timestamp_millis();
            let last_ms = last.timestamp_millis();
            (first_ms.min(last_ms), first_ms.max(last_ms))
        });

        let mut buckets: Vec<HistogramBucket> = (0..bucket_count)
            .map(|bucket_idx| {
                let label = if let Some((start_ms, end_ms)) = time_range {
                    let span = (end_ms - start_ms).max(1);
                    let bucket_ms = start_ms + (span * bucket_idx as i64) / bucket_count as i64;
                    DateTime::<Utc>::from_timestamp_millis(bucket_ms)
                        .map(|timestamp| timestamp.format("%H:%M:%S").to_string())
                        .unwrap_or_else(|| format!("T{}", bucket_idx + 1))
                } else {
                    format!("L{}", bucket_idx + 1)
                };
                HistogramBucket {
                    label,
                    first_line_idx: usize::MAX,
                    ..HistogramBucket::default()
                }
            })
            .collect();

        let mut max_bucket_count = 0usize;
        let mut total_errors = 0usize;
        let mut total_scanned = 0usize;
        let chunk_size = 500;

        let mut current = 0;
        while current < total_lines {
            let count = (total_lines - current).min(chunk_size);
            let lines = reader.read_range(current, count);
            if lines.is_empty() {
                break;
            }

            for (line_idx, line) in &lines {
                let bucket_idx = if let (Some((start_ms, end_ms)), Some(timestamp)) =
                    (time_range, extract_timestamp(line))
                {
                    let span = (end_ms - start_ms).max(1) as i128;
                    let offset = (timestamp.timestamp_millis() - start_ms).clamp(0, end_ms - start_ms) as i128;
                    ((offset * bucket_count as i128) / span) as usize
                } else {
                    (line_idx * bucket_count / total_lines).min(bucket_count - 1)
                }
                .min(bucket_count - 1);
                let bucket = &mut buckets[bucket_idx];
                bucket.total_count += 1;
                bucket.first_line_idx = bucket.first_line_idx.min(*line_idx);
                total_scanned += 1;

                let upper = line.to_uppercase();
                if upper.contains("ERROR")
                    || upper.contains("FATAL")
                    || upper.contains(" 500 ")
                    || upper.contains(" 502 ")
                    || upper.contains(" 503 ")
                    || upper.contains("\"status\":50")
                {
                    bucket.error_count += 1;
                    total_errors += 1;
                } else if upper.contains("WARN")
                    || upper.contains(" 404 ")
                    || upper.contains(" 400 ")
                    || upper.contains(" 429 ")
                {
                    bucket.warn_count += 1;
                }
            }
            current += lines.len();
        }

        for (idx, bucket) in buckets.iter_mut().enumerate() {
            if bucket.first_line_idx == usize::MAX {
                bucket.first_line_idx = 0;
                if time_range.is_none() {
                    bucket.label = format!("L{}", (idx * total_lines / bucket_count) + 1);
                }
            } else if time_range.is_none() {
                bucket.label = format!("L{}", bucket.first_line_idx + 1);
            }
            max_bucket_count = max_bucket_count.max(bucket.total_count);
        }

        Self {
            buckets,
            max_bucket_count: max_bucket_count.max(1),
            total_errors,
            total_scanned,
        }
    }
}

pub fn extract_timestamp(line: &str) -> Option<DateTime<Utc>> {
    if let Some(m) = ISO_TIME_RE.find(line) {
        let value = m.as_str();
        if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
            return Some(timestamp.with_timezone(&Utc));
        }
        for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
            if let Ok(timestamp) = NaiveDateTime::parse_from_str(value, format) {
                return Some(timestamp.and_utc());
            }
        }
    }
    if let Some(value) = line.strip_prefix('[').and_then(|v| v.split(']').next()) {
        for format in ["%a %b %d %H:%M:%S%.f %Y", "%a %b %e %H:%M:%S%.f %Y"] {
            if let Ok(timestamp) = NaiveDateTime::parse_from_str(value, format) {
                return Some(timestamp.and_utc());
            }
        }
    }
    if let Some(m) = APACHE_TIME_RE.find(line) {
        let value = m.as_str().trim_start_matches('[').trim_end_matches(']');
        if let Ok(timestamp) = DateTime::parse_from_str(value, "%d/%b/%Y:%H:%M:%S %z") {
            return Some(timestamp.with_timezone(&Utc));
        }
    }
    if let Some(m) = NGINX_TIME_RE.find(line) {
        if let Ok(timestamp) = NaiveDateTime::parse_from_str(m.as_str(), "%Y/%m/%d %H:%M:%S") {
            return Some(timestamp.and_utc());
        }
    }
    None
}

// -----------------------------------------------------------------------------
// 3. Log Pattern Clustering / Anomaly Detection
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LogCluster {
    pub pattern_template: String,
    pub count: usize,
    pub percentage: f64,
    pub sample_line: String,
    pub is_anomaly: bool, // rare pattern (< 2%)
}

pub struct PatternClusterer;

impl PatternClusterer {
    pub fn cluster(reader: &LogReader, max_lines: usize) -> Vec<LogCluster> {
        let total = reader.line_count().min(max_lines);
        if total == 0 {
            return Vec::new();
        }

        let lines = reader.read_range(0, total);
        let mut cluster_map: HashMap<String, (usize, String)> = HashMap::new();

        for (_, line) in &lines {
            let template = Self::abstract_line(line);
            let entry = cluster_map.entry(template).or_insert((0, line.clone()));
            entry.0 += 1;
        }

        let total_f = total as f64;
        let mut clusters: Vec<LogCluster> = cluster_map
            .into_iter()
            .map(|(template, (cnt, sample))| {
                let pct = (cnt as f64) / total_f * 100.0;
                let is_anomaly = pct < 2.0 && cnt <= 5;
                LogCluster {
                    pattern_template: template,
                    count: cnt,
                    percentage: pct,
                    sample_line: sample,
                    is_anomaly,
                }
            })
            .collect();

        // Sort descending by count, but anomalies can be identified
        clusters.sort_by(|a, b| b.count.cmp(&a.count));
        clusters
    }

    pub fn abstract_line(line: &str) -> String {
        let mut s = line.trim().to_string();

        // 1. Replace UUIDs
        s = UUID_RE.replace_all(&s, "<UUID>").to_string();

        // 2. Replace IP addresses
        s = IPV4_RE.replace_all(&s, "<IP>").to_string();

        // 3. Replace ISO and Apache Timestamps
        s = ISO_TIME_RE.replace_all(&s, "<TIME>").to_string();
        s = APACHE_TIME_RE.replace_all(&s, "<TIME>").to_string();

        if let Some(end) = s.find(']') {
            if s.starts_with('[') && extract_timestamp(&s[..=end]).is_some() {
                s.replace_range(..=end, "<TIME>");
            }
        }
        // 4. Replace Hex hashes
        s = HEX_HASH_RE.replace_all(&s, "<HEX>").to_string();

        // 5. Replace Long numbers (keep small status codes intact if possible)
        s = NUM_RE.replace_all(&s, "<NUM>").to_string();

        // Shorten long strings
        if s.len() > 120 {
            let end = s.char_indices().map(|(i, _)| i).take_while(|i| *i <= 117).last().unwrap_or(0);
            s.truncate(end);
            s.push_str("...");
        }

        s
    }
}

// -----------------------------------------------------------------------------
// 4. Trace Correlation Extractor
// -----------------------------------------------------------------------------

pub struct TraceCorrelator;

impl TraceCorrelator {
    /// Extracts correlation identifiers present in a log line
    pub fn extract_identifiers(line: &str) -> Vec<(String, String)> {
        let mut ids = Vec::new();

        for caps in CORRELATION_KEY_RE.captures_iter(line) {
            if let (Some(k), Some(v)) = (caps.get(1), caps.get(2)) {
                let key = k.as_str().to_lowercase();
                let val = v.as_str().to_string();
                if !val.is_empty() && val != "-" && val != "null" {
                    ids.push((key, val));
                }
            }
        }

        // Also check if line contains a standalone UUID
        if ids.is_empty() {
            if let Some(m) = UUID_RE.find(line) {
                ids.push(("uuid".into(), m.as_str().to_string()));
            }
        }

        // Also check IP address
        if let Some(m) = IPV4_RE.find(line) {
            ids.push(("ip".into(), m.as_str().to_string()));
        }

        ids
    }
}
