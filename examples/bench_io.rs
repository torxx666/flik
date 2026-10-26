//! Reproducible warm-cache I/O benchmark: cargo run --release --example bench_io -- 100000
use flik::{
    export::{ExportEngine, ExportFormat, ExportScope},
    filter::FilterEngine,
    format::FormatManager,
    reader::LogReader,
};
use std::{
    io::Write,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "100000".into())
        .parse()?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("input.log");
    let mut file = std::io::BufWriter::new(std::fs::File::create(&path)?);
    for i in 0..count {
        writeln!(
            file,
            "{{\"timestamp\":\"2026-10-21T10:00:00Z\",\"status\":200,\"msg\":\"request {i} completed successfully\"}}"
        )?;
    }
    file.flush()?;
    let start = Instant::now();
    let (reader, handle) = LogReader::open(&path)?;
    let preview = start.elapsed();
    while reader.is_indexing.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(1));
    }
    let indexed = start.elapsed();
    let start = Instant::now();
    let mut single_bytes = 0;
    for i in 0..count {
        single_bytes += reader.read_line(i).ok_or("missing line")?.len();
    }
    let single = start.elapsed();
    let start = Instant::now();
    let mut batch_bytes = 0;
    for offset in (0..count).step_by(500) {
        let indices: Vec<_> = (offset..offset.saturating_add(500).min(count)).collect();
        for line in reader.read_lines(&indices) {
            batch_bytes += line.ok_or("missing line")?.len();
        }
    }
    let batched = start.elapsed();
    let start = Instant::now();
    let exported = ExportEngine::export_to_file(
        dir.path().join("output.log"),
        &reader,
        &FilterEngine::new(),
        &FormatManager::new(),
        ExportScope::AllFiltered,
        ExportFormat::Raw,
    )?;
    let export_time = start.elapsed();
    reader.stop();
    handle.join().map_err(|_| "indexer panicked")?;
    assert_eq!(single_bytes, batch_bytes);
    assert_eq!(exported, count);
    assert_eq!(
        std::fs::read(dir.path().join("output.log"))?,
        std::fs::read(&path)?
    );
    println!(
        "{count} JSON lines; {} bytes; warm cache; {} / {}",
        std::fs::metadata(&path)?.len(),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("open/preview: {preview:?}; complete indexing: {indexed:?}");
    println!("per-line reads: {single:?}; batched reads: {batched:?}; raw export: {export_time:?}");
    println!(
        "batch/per-line ratio: {:.2}x",
        single.as_secs_f64() / batched.as_secs_f64()
    );
    Ok(())
}
