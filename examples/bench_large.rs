use flik::{filter::{FilterEngine, FilterKind}, reader::LogReader};
use std::{
    fs::File,
    io::{BufWriter, Write},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

#[allow(deprecated)]
fn get_rss_bytes() -> usize {
    #[cfg(target_os = "macos")]
    {
        use std::mem;
        let mut task_info: libc::mach_task_basic_info = unsafe { mem::zeroed() };
        let mut count = (mem::size_of::<libc::mach_task_basic_info>() / mem::size_of::<libc::natural_t>()) as libc::mach_msg_type_number_t;
        let kret = unsafe {
            libc::task_info(
                libc::mach_task_self(),
                libc::MACH_TASK_BASIC_INFO,
                &mut task_info as *mut _ as *mut libc::integer_t,
                &mut count,
            )
        };
        if kret == libc::KERN_SUCCESS {
            return task_info.resident_size as usize;
        }
    }
    0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count: usize = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "2000000".into())
        .parse()?;

    println!("============================================================");
    println!("  Flik Large Scale Benchmark: {count} lines");
    println!("============================================================");

    let dir = tempfile::tempdir()?;
    let path = dir.path().join("large_benchmark.log");

    print!("1. Generating {count} realistic log lines on disk... ");
    std::io::stdout().flush()?;
    let gen_start = Instant::now();
    {
        let mut file = BufWriter::with_capacity(1024 * 1024, File::create(&path)?);
        for i in 0..count {
            let status = if i % 100 == 0 { 500 } else if i % 20 == 0 { 404 } else { 200 };
            let level = if status >= 500 { "error" } else if status >= 400 { "warn" } else { "info" };
            writeln!(
                file,
                "2026-11-05T10:14:02.000Z host=prod-api service=billing status={} level={} msg=\"transaction #{} processed\" ip=192.168.1.{}",
                status, level, i, i % 254 + 1
            )?;
        }
        file.flush()?;
    }
    let file_size = std::fs::metadata(&path)?.len();
    println!("Done ({:?}, {:.2} MB)", gen_start.elapsed(), file_size as f64 / 1_048_576.0);

    let rss_before = get_rss_bytes();

    print!("2. Opening and initial preview... ");
    std::io::stdout().flush()?;
    let open_start = Instant::now();
    let (reader, handle) = LogReader::open(&path)?;
    let preview_time = open_start.elapsed();
    println!("Instant! ({:?})", preview_time);

    print!("3. Asynchronous full indexing... ");
    std::io::stdout().flush()?;
    while reader.is_indexing.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(5));
    }
    let index_time = open_start.elapsed();
    let total_indexed = reader.line_count();
    let rss_after_index = get_rss_bytes();
    let index_ram = rss_after_index.saturating_sub(rss_before);
    println!(
        "Done in {:?} ({:.0} lines/sec, ~{:.1} MB RAM)",
        index_time,
        total_indexed as f64 / index_time.as_secs_f64(),
        index_ram as f64 / 1_048_576.0
    );

    print!("4. Filtering full dataset (status=500)... ");
    std::io::stdout().flush()?;
    let filter_start = Instant::now();
    let mut filter = FilterEngine::new();
    filter.add_rule("status=500".into(), FilterKind::Include, false, false, &reader)?;
    while filter.is_filtering.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let filtered_time = filter_start.elapsed();
    let matches = filter.filtered_count(&reader);
    println!(
        "Found {} matches in {:?} ({:.0} lines/sec scanned)",
        matches,
        filtered_time,
        total_indexed as f64 / filtered_time.as_secs_f64()
    );

    print!("5. Random page seeks (reading 100 screen pages of 50 lines)... ");
    std::io::stdout().flush()?;
    let seek_start = Instant::now();
    for p in 0..100 {
        let offset = (p * 19999) % total_indexed.saturating_sub(50);
        let _ = reader.read_range(offset, 50);
    }
    let seek_time = seek_start.elapsed();
    println!("Done in {:?} ({:.2} µs/page)", seek_time, seek_time.as_micros() as f64 / 100.0);

    reader.stop();
    let _ = handle.join();
    println!("============================================================");
    Ok(())
}
