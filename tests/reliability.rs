use flik::{
    export::{ExportEngine, ExportFormat, ExportScope},
    filter::{FilterEngine, FilterKind},
    format::FormatManager,
    reader::LogReader,
};
use std::{
    io::Write,
    sync::atomic::Ordering,
    thread::JoinHandle,
    time::{Duration, Instant},
};

struct LiveReader(LogReader, Option<JoinHandle<()>>);
impl LiveReader {
    fn open(path: &std::path::Path) -> Self {
        let (reader, handle) = LogReader::open(path).unwrap();
        let this = Self(reader, Some(handle));
        wait(|| !this.0.is_indexing.load(Ordering::Acquire));
        this
    }
}
impl Drop for LiveReader {
    fn drop(&mut self) {
        self.0.stop();
        self.1.take().unwrap().join().unwrap();
    }
}
fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(Instant::now() < deadline, "background operation timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn append(path: &std::path::Path, content: &[u8]) {
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(content)
        .unwrap();
}
#[test]
fn append_partial_empty_and_large_backlog() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log");
    std::fs::write(&path, "").unwrap();
    let reader = LiveReader::open(&path);
    append(&path, b"hel");
    wait(|| reader.0.line_count() == 1);
    append(&path, b"lo\n");
    wait(|| reader.0.file_size.load(Ordering::Acquire) == 6);
    assert_eq!(reader.0.read_line(0).as_deref(), Some("hello"));
    assert_eq!(reader.0.line_count(), 1);
    let backlog = "another event\n".repeat(20000);
    append(&path, backlog.as_bytes());
    wait(|| reader.0.line_count() == 20001);
    assert_eq!(reader.0.read_line(20000).as_deref(), Some("another event"));
    assert_eq!(reader.0.read_range(0, usize::MAX).len(), 20001);
}
#[test]
fn rotation_to_larger_file_and_truncation_rebuild_filters() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log");
    std::fs::write(&path, "old\n").unwrap();
    let reader = LiveReader::open(&path);
    let mut filter = FilterEngine::new();
    filter
        .add_rule("match".into(), FilterKind::Include, false, false, &reader.0)
        .unwrap();
    wait(|| !filter.is_filtering.load(Ordering::Acquire));
    let revision = reader.0.revision.load(Ordering::Acquire);
    std::fs::rename(&path, dir.path().join("rotated")).unwrap();
    std::fs::write(&path, "match first\nmatch second\n").unwrap();
    wait(|| reader.0.revision.load(Ordering::Acquire) > revision && reader.0.line_count() == 2);
    wait(|| {
        filter.process_incoming_lines(&reader.0);
        filter.filtered_count(&reader.0) == 2
    });
    assert_eq!(reader.0.read_line(0).as_deref(), Some("match first"));
    std::fs::write(&path, "new\n").unwrap();
    wait(|| reader.0.line_count() == 1);
    wait(|| {
        filter.process_incoming_lines(&reader.0);
        !filter.is_filtering.load(Ordering::Acquire) && filter.filtered_count(&reader.0) == 0
    });
    assert_eq!(reader.0.read_line(0).as_deref(), Some("new"));
}
#[test]
fn partial_line_is_refiltered_when_completed() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"hel").unwrap();
    let reader = LiveReader::open(file.path());
    let mut filter = FilterEngine::new();
    filter
        .add_rule("hello".into(), FilterKind::Include, false, false, &reader.0)
        .unwrap();
    wait(|| !filter.is_filtering.load(Ordering::Acquire));
    append(file.path(), b"lo\n");
    wait(|| {
        filter.process_incoming_lines(&reader.0);
        filter.filtered_count(&reader.0) == 1
    });
    assert_eq!(reader.0.line_count(), 1);
}
#[test]
fn export_refuses_existing_source_and_links_and_preserves_long_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log");
    let content = format!("{}\nend\n", "x".repeat(2 * 1024 * 1024));
    std::fs::write(&path, &content).unwrap();
    let reader = LiveReader::open(&path);
    let filter = FilterEngine::new();
    let format = FormatManager::new();
    let export = |out: &std::path::Path| {
        ExportEngine::export_to_file(
            out,
            &reader.0,
            &filter,
            &format,
            ExportScope::AllFiltered,
            ExportFormat::Raw,
        )
    };
    assert!(export(&path).is_err());
    let existing = dir.path().join("existing");
    std::fs::write(&existing, "keep").unwrap();
    assert!(export(&existing).is_err());
    assert_eq!(std::fs::read_to_string(existing).unwrap(), "keep");
    #[cfg(unix)]
    {
        let link = dir.path().join("symlink");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(export(&link).is_err());
    }
    let hardlink = dir.path().join("hardlink");
    std::fs::hard_link(&path, &hardlink).unwrap();
    assert!(export(&hardlink).is_err());
    let out = dir.path().join("out");
    assert_eq!(export(&out).unwrap(), 2);
    assert_eq!(std::fs::read_to_string(out).unwrap(), content);
    assert_eq!(std::fs::read_to_string(path).unwrap(), content);
}

#[test]
fn smart_find_keeps_previous_lines_without_duplicates_and_follows_appends() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("smart.log");
    std::fs::write(&path, "error first\npreparation\nerror second\nerror third\nunrelated\ncause\n").unwrap();
    let reader = LiveReader::open(&path);
    let mut filter = FilterEngine::new();
    filter.add_rule("//error".into(), FilterKind::Include, false, false, &reader.0).unwrap();
    wait(|| !filter.is_filtering.load(Ordering::Acquire));
    assert_eq!(*filter.filtered_indices.read(), vec![0, 1, 2, 3]);
    append(&path, b"error appended\n");
    wait(|| reader.0.line_count() == 7);
    wait(|| {
        filter.process_incoming_lines(&reader.0);
        *filter.filtered_indices.read() == vec![0, 1, 2, 3, 5, 6]
    });
    filter.pop_last_rule(&reader.0);
    filter.add_rule("error".into(), FilterKind::Include, false, false, &reader.0).unwrap();
    wait(|| !filter.is_filtering.load(Ordering::Acquire));
    assert_eq!(*filter.filtered_indices.read(), vec![0, 2, 3, 6]);
}
