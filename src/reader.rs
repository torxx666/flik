use parking_lot::RwLock;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Stores line offsets for fast O(1) random access into arbitrarily large files.
/// Lines are indexed in the background without blocking the UI.
#[derive(Clone)]
pub struct LogReader {
    /// Changes when existing indexed content must be filtered again.
    pub revision: Arc<AtomicU64>,
    pub path: PathBuf,
    pub file_size: Arc<AtomicU64>,
    /// Byte offset in the file for the start of each line
    offsets: Arc<RwLock<Vec<u64>>>,
    /// True while background indexing thread is scanning
    pub is_indexing: Arc<AtomicBool>,
    /// Request the background indexing/tail thread to stop
    stop_signal: Arc<AtomicBool>,
    /// Whether tail -F follow mode is enabled
    pub tail_mode: Arc<AtomicBool>,
    /// Total count of indexed lines so far
    pub total_lines: Arc<AtomicUsize>,
    /// Timestamp in milliseconds (UNIX epoch) of when new bytes were last appended
    pub last_activity_time: Arc<AtomicU64>,
    /// Total lines added dynamically since the file was opened
    pub incoming_lines_count: Arc<AtomicUsize>,
}

impl LogReader {
    pub fn open<P: AsRef<Path>>(path: P) -> std::io::Result<(Self, JoinHandle<()>)> {
        let path_buf = path.as_ref().to_path_buf();
        let file = File::open(&path_buf)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Choose a regular log file",
            ));
        }
        let file_size = Arc::new(AtomicU64::new(metadata.len()));
        let offsets = Arc::new(RwLock::new(Vec::with_capacity(10_000)));
        let is_indexing = Arc::new(AtomicBool::new(true));
        let stop_signal = Arc::new(AtomicBool::new(false));
        let tail_mode = Arc::new(AtomicBool::new(false));
        let total_lines = Arc::new(AtomicUsize::new(0));

        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let last_activity_initial = if let Ok(mtime) = metadata.modified() {
            if let Ok(duration) = mtime.elapsed() {
                if duration.as_secs() < 10 {
                    now_millis
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };

        let last_activity_time = Arc::new(AtomicU64::new(last_activity_initial));
        let incoming_lines_count = Arc::new(AtomicUsize::new(0));

        // Always the first line starts at offset 0 if file is not empty
        if metadata.len() > 0 {
            offsets.write().push(0);
            total_lines.store(1, Ordering::Relaxed);
        }

        // Fast synchronous preview: index first 2,000 lines or first 256KB immediately
        // with a strict byte budget, including files containing very long lines.
        let mut reader = BufReader::with_capacity(64 * 1024, file);
        let mut current_offset: u64 = 0;
        let mut initial_scanned_bytes: u64 = 0;
        let mut initial_lines = 0;
        let max_initial_bytes = 256 * 1024;
        let max_initial_lines = 2000;

        let mut buf = Vec::with_capacity(1024);
        let mut ends_with_newline = false;
        while initial_scanned_bytes < max_initial_bytes && initial_lines < max_initial_lines {
            buf.clear();
            match reader
                .by_ref()
                .take(max_initial_bytes - initial_scanned_bytes)
                .read_until(b'\n', &mut buf)
            {
                Ok(0) => break,
                Ok(n) => {
                    ends_with_newline = buf.last() == Some(&b'\n');
                    current_offset += n as u64;
                    initial_scanned_bytes += n as u64;
                    if ends_with_newline && initial_scanned_bytes < metadata.len() {
                        offsets.write().push(current_offset);
                        initial_lines += 1;
                        total_lines.store(initial_lines + 1, Ordering::Relaxed);
                    }
                }
                Err(_) => break,
            }
        }

        let revision = Arc::new(AtomicU64::new(0));
        let reader_clone = Self {
            revision: Arc::clone(&revision),
            path: path_buf.clone(),
            file_size: Arc::clone(&file_size),
            offsets: Arc::clone(&offsets),
            is_indexing: Arc::clone(&is_indexing),
            stop_signal: Arc::clone(&stop_signal),
            tail_mode: Arc::clone(&tail_mode),
            total_lines: Arc::clone(&total_lines),
            last_activity_time: Arc::clone(&last_activity_time),
            incoming_lines_count: Arc::clone(&incoming_lines_count),
        };

        // Spawn background worker for indexing the rest of the file + live file monitoring
        let thread_path = path_buf;
        let thread_offsets = Arc::clone(&offsets);
        let thread_indexing = Arc::clone(&is_indexing);
        let thread_stop = Arc::clone(&stop_signal);
        let thread_tail = Arc::clone(&tail_mode);
        let thread_total = Arc::clone(&total_lines);
        let thread_file_size = Arc::clone(&file_size);
        let thread_last_activity = Arc::clone(&last_activity_time);
        let thread_incoming_lines = Arc::clone(&incoming_lines_count);

        let handle = thread::spawn(move || {
            Self::background_indexer(
                thread_path,
                reader,
                ends_with_newline,
                revision,
                current_offset,
                thread_offsets,
                thread_indexing,
                thread_stop,
                thread_tail,
                thread_total,
                thread_file_size,
                thread_last_activity,
                thread_incoming_lines,
            );
        });

        Ok((reader_clone, handle))
    }

    #[allow(clippy::too_many_arguments)]
    fn background_indexer(
        path: PathBuf,
        mut reader: BufReader<File>,
        mut ends_with_newline: bool,
        revision: Arc<AtomicU64>,
        start_offset: u64,
        offsets: Arc<RwLock<Vec<u64>>>,
        is_indexing: Arc<AtomicBool>,
        stop_signal: Arc<AtomicBool>,
        _tail_mode: Arc<AtomicBool>,
        total_lines: Arc<AtomicUsize>,
        file_size: Arc<AtomicU64>,
        last_activity_time: Arc<AtomicU64>,
        incoming_lines_count: Arc<AtomicUsize>,
    ) {
        let mut current_offset = start_offset;
        let mut buffer = [0u8; 64 * 1024];
        let mut initial = true;
        loop {
            if stop_signal.load(Ordering::Relaxed) {
                is_indexing.store(false, Ordering::Release);
                return;
            }
            if let Ok(meta) = std::fs::metadata(&path) {
                #[cfg(unix)]
                let replaced = {
                    use std::os::unix::fs::MetadataExt;
                    reader
                        .get_ref()
                        .metadata()
                        .map(|old| old.dev() != meta.dev() || old.ino() != meta.ino())
                        .unwrap_or(true)
                };
                #[cfg(not(unix))]
                let replaced = false;
                if replaced || meta.len() < current_offset {
                    if let Ok(file) = File::open(&path) {
                        reader = BufReader::with_capacity(128 * 1024, file);
                        current_offset = 0;
                        ends_with_newline = false;
                        let mut index = offsets.write();
                        index.clear();
                        total_lines.store(0, Ordering::Release);
                        revision.fetch_add(1, Ordering::AcqRel);
                        is_indexing.store(true, Ordering::Release);
                    }
                }
                let len = reader
                    .get_ref()
                    .metadata()
                    .map(|m| m.len())
                    .unwrap_or(current_offset);
                file_size.store(len, Ordering::Release);
                if len > current_offset {
                    // Drain backlog immediately; never throttle indexing to one chunk per poll.
                    let available = (len - current_offset).min(buffer.len() as u64) as usize;
                    match reader.read(&mut buffer[..available]) {
                        Ok(n) if n > 0 => {
                            let mut index = offsets.write();
                            let before = index.len();
                            if current_offset == 0 {
                                index.push(0);
                            } else if ends_with_newline && index.last() != Some(&current_offset) {
                                index.push(current_offset);
                            } else if !initial && !ends_with_newline {
                                // Appending can change whether the previous last line matches.
                                revision.fetch_add(1, Ordering::AcqRel);
                            }
                            for (i, byte) in buffer[..n].iter().enumerate() {
                                let next = current_offset + i as u64 + 1;
                                if *byte == b'\n' && next < len {
                                    index.push(next);
                                }
                            }
                            current_offset += n as u64;
                            ends_with_newline = buffer[n - 1] == b'\n';
                            total_lines.store(index.len(), Ordering::Release);
                            let added = index.len() - before;
                            drop(index);
                            if !initial {
                                incoming_lines_count.fetch_add(added, Ordering::Relaxed);
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64;
                                last_activity_time.store(now, Ordering::Relaxed);
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            initial = false;
            is_indexing.store(false, Ordering::Release);
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// Read a single line at the given line index
    pub fn read_line(&self, line_index: usize) -> Option<String> {
        self.read_lines(&[line_index]).pop().flatten()
    }

    /// Read multiple lines while opening the file only once.
    pub fn read_lines(&self, line_indices: &[usize]) -> Vec<Option<String>> {
        let file_size = self.file_size.load(Ordering::Relaxed);
        let ranges: Vec<Option<(u64, u64)>> = {
            let offsets = self.offsets.read();
            line_indices
                .iter()
                .map(|&line_index| {
                    offsets.get(line_index).map(|&start| {
                        let end = offsets.get(line_index + 1).copied().unwrap_or(file_size);
                        (start, end)
                    })
                })
                .collect()
        };

        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(_) => return vec![None; line_indices.len()],
        };

        let mut reader = BufReader::with_capacity(64 * 1024, &mut file);
        let mut position = None;
        ranges
            .into_iter()
            .map(|range| {
                let (start, end) = range?;
                if end <= start {
                    return None;
                }
                if position != Some(start) {
                    reader.seek(SeekFrom::Start(start)).ok()?;
                }
                let mut line = String::new();
                let bytes = reader
                    .by_ref()
                    .take(end - start)
                    .read_line(&mut line)
                    .ok()?;
                if bytes == 0 {
                    return None;
                }
                position = Some(start + bytes as u64);

                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Some(line)
            })
            .collect()
    }

    /// Read a contiguous slice of lines starting at start_index
    pub fn read_range(&self, start_index: usize, count: usize) -> Vec<(usize, String)> {
        let offsets = self.offsets.read();
        let total = offsets.len();
        if start_index >= total || count == 0 {
            return Vec::new();
        }

        let end_index = start_index.saturating_add(count).min(total);
        let start_offset = offsets[start_index];
        let end_offset = if end_index < total {
            offsets[end_index]
        } else {
            self.file_size.load(Ordering::Relaxed)
        };
        drop(offsets);

        if end_offset <= start_offset {
            return Vec::new();
        }

        let mut file = match File::open(&self.path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        if file.seek(SeekFrom::Start(start_offset)).is_err() {
            return Vec::new();
        }

        let bytes_to_read = end_offset - start_offset;
        let mut handle = (&mut file).take(bytes_to_read);
        let reader = BufReader::with_capacity(64 * 1024, &mut handle);

        let mut results = Vec::with_capacity(end_index - start_index);
        let mut current_idx = start_index;

        for line_res in reader.lines() {
            if current_idx >= end_index {
                break;
            }
            if let Ok(line) = line_res {
                results.push((current_idx, line));
                current_idx += 1;
            } else {
                break;
            }
        }

        results
    }

    pub fn line_count(&self) -> usize {
        self.total_lines.load(Ordering::Relaxed)
    }

    pub fn is_tail_enabled(&self) -> bool {
        self.tail_mode.load(Ordering::Relaxed)
    }

    pub fn set_tail_enabled(&self, enabled: bool) {
        self.tail_mode.store(enabled, Ordering::Relaxed);
    }

    pub fn toggle_tail(&self) -> bool {
        let new_val = !self.is_tail_enabled();
        self.set_tail_enabled(new_val);
        new_val
    }

    pub fn is_actively_growing(&self) -> bool {
        let last = self.last_activity_time.load(Ordering::Relaxed);
        if last == 0 {
            return false;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now.saturating_sub(last) < 3000
    }

    pub fn incoming_lines_count(&self) -> usize {
        self.incoming_lines_count.load(Ordering::Relaxed)
    }

    pub fn reset_incoming_lines(&self) {
        self.incoming_lines_count.store(0, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::Relaxed);
    }
}
