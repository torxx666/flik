//! Live multi-log file merger with chronological interleaving and continuous following.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use tempfile::NamedTempFile;

use crate::analytics::extract_timestamp;

pub struct LiveMergedStream {
    pub temp_file: NamedTempFile,
    pub stop_flag: Arc<AtomicBool>,
    pub handle: Option<JoinHandle<()>>,
}

struct SourceTracker {
    path: PathBuf,
    label: String,
    last_offset: u64,
}

impl LiveMergedStream {
    pub fn start(sources: &[PathBuf]) -> Result<Self, std::io::Error> {
        let temp_file = tempfile::Builder::new()
            .prefix("flik_live_merge_")
            .suffix(".log")
            .tempfile()?;

        let mut trackers = Vec::new();
        for p in sources {
            let label = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "source".to_string());
            trackers.push(SourceTracker {
                path: p.clone(),
                label,
                last_offset: 0,
            });
        }

        // 1. Initial pass: Read existing contents from all sources, extract timestamps,
        // sort chronologically, and write to temp_file.
        struct Entry {
            ts_ms: Option<i64>,
            source_idx: usize,
            line: String,
        }
        let mut initial_entries = Vec::new();

        for (idx, tracker) in trackers.iter_mut().enumerate() {
            if let Ok(file) = File::open(&tracker.path) {
                let mut reader = BufReader::new(file);
                let mut line_buf = String::new();
                let mut last_ts_ms: Option<i64> = None;

                while let Ok(n) = reader.read_line(&mut line_buf) {
                    if n == 0 {
                        break;
                    }
                    let line = line_buf.trim_end_matches(&['\r', '\n'][..]).to_string();
                    let ts_ms = extract_timestamp(&line)
                        .map(|dt| dt.timestamp_millis())
                        .or(last_ts_ms);
                    if ts_ms.is_some() {
                        last_ts_ms = ts_ms;
                    }
                    initial_entries.push(Entry {
                        ts_ms,
                        source_idx: idx,
                        line,
                    });
                    line_buf.clear();
                }
                tracker.last_offset = reader.stream_position().unwrap_or(0);
            }
        }

        // Stable chronological sort: timed events sorted by timestamp, untimed kept in order
        initial_entries.sort_by_key(|e| (e.ts_ms.is_none(), e.ts_ms.unwrap_or(0), e.source_idx));

        let mut out_file = OpenOptions::new()
            .append(true)
            .open(temp_file.path())?;

        for entry in initial_entries {
            let label = &trackers[entry.source_idx].label;
            writeln!(out_file, "[{}] {}", label, entry.line)?;
        }
        out_file.flush()?;

        // 2. Background live tail thread
        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop_flag);
        let out_path = temp_file.path().to_path_buf();

        let handle = std::thread::spawn(move || {
            let mut out = match OpenOptions::new().append(true).open(&out_path) {
                Ok(f) => f,
                Err(_) => return,
            };

            while !thread_stop.load(Ordering::Acquire) {
                let mut new_entries = Vec::new();

                for (idx, tracker) in trackers.iter_mut().enumerate() {
                    if let Ok(mut file) = File::open(&tracker.path) {
                        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
                        if len < tracker.last_offset {
                            // File was truncated or rotated
                            tracker.last_offset = 0;
                        }
                        if len > tracker.last_offset
                            && file.seek(SeekFrom::Start(tracker.last_offset)).is_ok() {
                            let mut reader = BufReader::new(file);
                                let mut line_buf = String::new();
                                while let Ok(n) = reader.read_line(&mut line_buf) {
                                    if n == 0 {
                                        break;
                                    }
                                    let line = line_buf.trim_end_matches(&['\r', '\n'][..]).to_string();
                                    let ts_ms = extract_timestamp(&line).map(|dt| dt.timestamp_millis());
                                    new_entries.push((ts_ms, idx, line));
                                    line_buf.clear();
                                }
                                tracker.last_offset = reader.stream_position().unwrap_or(len);
                            }
                        }
                    }

                if !new_entries.is_empty() {
                    // Chronologically sort batch of live lines
                    new_entries.sort_by_key(|e| (e.0.is_none(), e.0.unwrap_or(0), e.1));
                    for (_ts, idx, line) in new_entries {
                        let label = &trackers[idx].label;
                        let _ = writeln!(out, "[{}] {}", label, line);
                    }
                    let _ = out.flush();
                }

                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        });

        Ok(Self {
            temp_file,
            stop_flag,
            handle: Some(handle),
        })
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for LiveMergedStream {
    fn drop(&mut self) {
        self.stop();
    }
}
