//! Read-only source snapshots, chronological results and redacted incident bundles.
use crate::{
    analytics::extract_timestamp,
    app::App,
    filter::{FilterEngine, FilterRule},
    reader::LogReader,
};
use regex::Regex;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Clone)]
pub struct Source {
    pub reader: LogReader,
    rules: Vec<FilterRule>,
    fields: Option<Vec<String>>,
    json: bool,
    pub total: usize,
    revision: u64,
    metadata: Option<std::fs::Metadata>,
    pub path: PathBuf,
}
impl Source {
    pub fn capture(app: &App) -> Self {
        Self {
            reader: app.reader.clone(),
            rules: app.filter.rules.clone(),
            fields: app.filter.visible_fields.clone(),
            json: app.filter.is_json_filter_active,
            total: app.reader.line_count(),
            revision: app.reader.revision.load(Ordering::Acquire),
            metadata: std::fs::metadata(&app.reader.path).ok(),
            path: std::fs::canonicalize(&app.reader.path)
                .unwrap_or_else(|_| app.reader.path.clone()),
        }
    }
    pub(crate) fn matcher(&self) -> FilterEngine {
        let mut filter = FilterEngine::new();
        filter.rules = self.rules.clone();
        filter.visible_fields = self.fields.clone();
        filter.is_json_filter_active = self.json;
        filter
    }
    pub fn validate(&self) -> Result<(), String> {
        let current = std::fs::metadata(&self.reader.path).map_err(|e| e.to_string())?;
        let old = self
            .metadata
            .as_ref()
            .ok_or("Cannot identify source file")?;
        #[cfg(unix)]
        let replaced = {
            use std::os::unix::fs::MetadataExt;
            current.ino() != old.ino() || current.dev() != old.dev()
        };
        #[cfg(not(unix))]
        let replaced = false;
        if replaced
            || current.len() < old.len()
            || (current.len() == old.len() && current.modified().ok() != old.modified().ok())
            || self.reader.revision.load(Ordering::Acquire) != self.revision
        {
            Err("Source changed during investigation; refresh the snapshot".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct EventRow {
    pub source: usize,
    pub line: usize,
    pub timestamp_ms: Option<i64>,
    pub matched: bool,
    pub text: String,
}

pub fn context(
    source: &Source,
    source_id: usize,
    line: usize,
    radius: usize,
) -> Result<Vec<EventRow>, String> {
    let start = line.saturating_sub(radius);
    let end = line
        .saturating_add(radius)
        .saturating_add(1)
        .min(source.total);
    let matcher = source.matcher();
    let rows = source
        .reader
        .read_range(start, end.saturating_sub(start))
        .into_iter()
        .map(|(line, text)| EventRow {
            source: source_id,
            line,
            timestamp_ms: extract_timestamp(&text).map(|t| t.timestamp_millis()),
            matched: matcher.line_matches(&text),
            text,
        })
        .collect();
    source.validate()?;
    Ok(rows)
}

pub fn collect(
    sources: &[Source],
    cancel: &AtomicBool,
    scanned: &AtomicUsize,
) -> Result<Vec<EventRow>, String> {
    let mut rows = Vec::new();
    let mut result_bytes: usize = 0;
    for (id, source) in sources.iter().enumerate() {
        let matcher = source.matcher();
        for offset in (0..source.total).step_by(500) {
            if cancel.load(Ordering::Acquire) {
                return Err("Cancelled".into());
            }
            let count = 500.min(source.total - offset);
            let batch = source.reader.read_range(offset, count);
            if batch.len() != count {
                return Err("Cannot read all indexed lines".into());
            }
            for (line, text) in batch {
                scanned.fetch_add(1, Ordering::Relaxed);
                if matcher.line_matches(&text) {
                    // Store provenance and text so the view remains a stable snapshot.
                    result_bytes = result_bytes.saturating_add(text.len());
                    if rows.len() >= 500_000 || result_bytes > 256 * 1024 * 1024 {
                        return Err("Investigation exceeds 500000 results or 256 MiB of text; narrow the filters or time range".into());
                    }
                    rows.push(EventRow {
                        source: id,
                        line,
                        timestamp_ms: extract_timestamp(&text).map(|t| t.timestamp_millis()),
                        matched: true,
                        text,
                    });
                }
            }
        }
        source.validate()?;
    }
    rows.sort_by_key(|r| {
        (
            r.timestamp_ms.is_none(),
            r.timestamp_ms.unwrap_or_default(),
            r.source,
            r.line,
        )
    });
    Ok(rows)
}

/// Find the nearest matching timestamp without retaining or sorting the whole file.
pub fn find_timestamp(
    source: &Source,
    source_id: usize,
    target: i64,
    cancel: &AtomicBool,
    scanned: &AtomicUsize,
) -> Result<EventRow, String> {
    let matcher = source.matcher();
    let mut best: Option<EventRow> = None;
    for offset in (0..source.total).step_by(500) {
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        let count = 500.min(source.total - offset);
        let batch = source.reader.read_range(offset, count);
        if batch.len() != count {
            return Err("Cannot read all indexed lines".into());
        }
        for (line, text) in batch {
            scanned.fetch_add(1, Ordering::Relaxed);
            if !matcher.line_matches(&text) {
                continue;
            }
            let Some(time) = extract_timestamp(&text).map(|t| t.timestamp_millis()) else {
                continue;
            };
            if best.as_ref().is_none_or(|row| {
                time.abs_diff(target) < row.timestamp_ms.unwrap().abs_diff(target)
            }) {
                best = Some(EventRow {
                    source: source_id,
                    line,
                    timestamp_ms: Some(time),
                    matched: true,
                    text,
                });
            }
        }
    }
    source.validate()?;
    best.ok_or("No timestamped results in the active pane".into())
}

#[derive(Clone)]
pub struct Redactor {
    keys: BTreeSet<String>,
    assignments: Regex,
    auth: Regex,
    headers: Regex,
}
impl Redactor {
    pub fn new(extra: &[String]) -> Result<Self, String> {
        let mut keys: BTreeSet<String> = [
            "password",
            "passwd",
            "secret",
            "token",
            "access_token",
            "refresh_token",
            "api_key",
            "apikey",
            "authorization",
            "cookie",
            "set-cookie",
            "client_secret",
        ]
        .iter()
        .map(|v| v.to_string())
        .collect();
        keys.extend(extra.iter().map(|k| k.to_lowercase()));
        let alternatives = keys
            .iter()
            .map(|k| regex::escape(k))
            .collect::<Vec<_>>()
            .join("|");
        let assignments = Regex::new(&format!(r#"(?i)(?P<key>\b(?:{alternatives})\b"?\s*[:=]\s*)(?:"(?:\\.|[^"\\])*"|'[^']*'|[^\s,;}}]+)"#)).map_err(|e| e.to_string())?;
        let auth = Regex::new(r"(?i)(authorization\s*[:=]\s*)(?:bearer|basic)\s+[^\s,;]+")
            .map_err(|e| e.to_string())?;
        let headers = Regex::new(r"(?im)(\b(?:authorization|cookie|set-cookie)\s*:\s*)[^\r\n]+")
            .map_err(|e| e.to_string())?;
        Ok(Self {
            keys,
            assignments,
            auth,
            headers,
        })
    }
    fn value(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, val) in map {
                    if self.keys.contains(&key.to_lowercase()) {
                        *val = serde_json::Value::String("[REDACTED]".into());
                    } else {
                        self.value(val);
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for v in values {
                    self.value(v);
                }
            }
            serde_json::Value::String(text) => *text = self.scrub_text(text),
            _ => {}
        }
    }
    fn scrub_text(&self, text: &str) -> String {
        let text = self.headers.replace_all(text, "${1}[REDACTED]");
        let text = self.auth.replace_all(&text, "${1}[REDACTED]");
        self.assignments
            .replace_all(&text, "${key}\"[REDACTED]\"")
            .into_owned()
    }
    pub fn redact(&self, text: &str) -> String {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) {
            self.value(&mut value);
            return value.to_string();
        }
        if let Some(start) = text.find('{')
            && let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text[start..])
        {
            self.value(&mut value);
            return format!("{}{}", self.scrub_text(&text[..start]), value);
        }
        self.scrub_text(text)
    }
}

/// Atomic publication: errors/cancellation leave no partial destination; existing paths are refused.
pub fn export_bundle(
    path: &Path,
    sources: &[Source],
    rows: &[EventRow],
    radius: usize,
    redactor: &Redactor,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    if path.exists() {
        return Err("Choose a new destination; existing files are never overwritten".into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut windows: Vec<Vec<(usize, usize)>> = vec![Vec::new(); sources.len()];
    let mut matches: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); sources.len()];
    for row in rows {
        let source = sources.get(row.source).ok_or("Unknown event source")?;
        windows[row.source].push((
            row.line.saturating_sub(radius),
            row.line
                .saturating_add(radius)
                .saturating_add(1)
                .min(source.total),
        ));
        if row.matched {
            matches[row.source].insert(row.line);
        }
    }
    let metadata = serde_json::json!({"type":"flik_investigation", "version":1, "created_at":chrono::Utc::now().to_rfc3339(), "context_lines":radius,
        "sources":sources.iter().enumerate().map(|(id,s)| serde_json::json!({"id":id,"path":redactor.redact(&s.path.to_string_lossy()),"indexed_lines":s.total,"filters":s.rules.iter().map(|r|serde_json::json!({"pattern":redactor.redact(&r.pattern),"enabled":r.enabled,"kind":format!("{:?}",r.kind)})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "redaction":"key-based, including nested JSON and text assignments; review free text before sharing"});
    writeln!(file, "{metadata}").map_err(|e| e.to_string())?;
    // Batch reads per source, deduplicating overlapping context windows.
    let mut count = 0;
    for (source_id, source) in sources.iter().enumerate() {
        windows[source_id].sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for &(start, end) in &windows[source_id] {
            if let Some(previous) = merged.last_mut().filter(|previous| start <= previous.1) {
                previous.1 = previous.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        for (start, end) in merged {
            for offset in (start..end).step_by(500) {
                if cancel.load(Ordering::Acquire) {
                    return Err("Cancelled".into());
                }
                let expected = (end - offset).min(500);
                let batch = source.reader.read_range(offset, expected);
                if batch.len() != expected {
                    return Err("Cannot read all investigation lines".into());
                }
                for (line, text) in batch {
                    let matched = matches[source_id].contains(&line);
                    let event = serde_json::json!({"type":"event","source":source_id,"line":line+1,"match":matched,"timestamp":extract_timestamp(&text).map(|t|t.to_rfc3339()),"text":redactor.redact(&text)});
                    writeln!(file, "{event}").map_err(|e| e.to_string())?;
                    count += 1;
                }
            }
        }
        source.validate()?;
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Cancelled".into());
    }
    file.flush().map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist_noclobber(path).map_err(|e| e.to_string())?;
    Ok(count)
}
