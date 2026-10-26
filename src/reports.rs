//! Streaming reports over a stable source snapshot; retain counts, never all log text.
use crate::{
    analytics::{PatternClusterer, extract_timestamp},
    investigation::Source,
};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

fn scan(
    source: &Source,
    cancel: &AtomicBool,
    scanned: &AtomicUsize,
    mut visit: impl FnMut(usize, &str),
) -> Result<(), String> {
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
        for (index, text) in batch {
            if matcher.line_matches(&text) {
                visit(index, &text);
            }
            scanned.fetch_add(1, Ordering::Relaxed);
        }
    }
    source.validate()
}

pub fn frequency(
    source: &Source,
    seconds: i64,
    cancel: &AtomicBool,
    scanned: &AtomicUsize,
) -> Result<Vec<String>, String> {
    if ![60, 3600, 86400].contains(&seconds) {
        return Err("Use histogram minute|hour|day".into());
    }
    let mut counts = BTreeMap::<i64, usize>::new();
    let mut unknown = 0;
    let mut overflow = false;
    scan(source, cancel, scanned, |_, text| {
        if let Some(time) = extract_timestamp(text) {
            let bucket = time.timestamp().div_euclid(seconds) * seconds;
            if counts.len() >= 10000 && !counts.contains_key(&bucket) {
                overflow = true;
                return;
            }
            *counts.entry(bucket).or_default() += 1;
        } else {
            unknown += 1;
        }
    })?;
    if overflow {
        return Err(
            "More than 10000 time buckets; choose a larger interval or narrow the time filter"
                .into(),
        );
    }
    if let (Some(first), Some(last)) = (
        counts.keys().next().copied(),
        counts.keys().next_back().copied(),
    ) {
        if (last - first) / seconds < 10000 {
            for bucket in (first..=last).step_by(seconds as usize) {
                counts.entry(bucket).or_default();
            }
        }
    }
    let total: usize = counts.values().sum();
    let mut lines = vec![
        format!(
            "{} — active filters; actual matches only (context excluded)",
            source.path.display()
        ),
        format!(
            "{} timed matches | {} without a recognized timestamp | UTC",
            total, unknown
        ),
        format!(
            "Interval: {} | snapshot; H refreshes after closing",
            match seconds {
                60 => "minute",
                3600 => "hour",
                _ => "day",
            }
        ),
        String::new(),
    ];
    let max = counts.values().copied().max().unwrap_or(1).max(1);
    for (bucket, count) in counts {
        let label = chrono::DateTime::from_timestamp(bucket, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M")
            .to_string();
        lines.push(format!(
            "{}  {:>10}  {}",
            label,
            count,
            "█".repeat(count * 35 / max)
        ));
    }
    if total == 0 {
        lines.push("No timestamped matches. Check your filters and timestamp format.".into());
    }
    Ok(lines)
}

/// Split one file around a timestamp, or compare two files, respecting each source's filters.
pub fn compare(
    sources: &[Source],
    window: Option<(i64, i64)>,
    cancel: &AtomicBool,
    scanned: &AtomicUsize,
) -> Result<Vec<String>, String> {
    if window.is_none() && sources.len() != 2 {
        return Err("Open two files for compare files".into());
    }
    let mut counts = BTreeMap::<String, [usize; 2]>::new();
    let mut totals = [0usize; 2];
    let mut unknown = 0;
    let mut overflow = false;
    for (source_index, source) in sources.iter().enumerate() {
        scan(source, cancel, scanned, |_, text| {
            let side = if let Some((pivot, radius)) = window {
                let Some(time) = extract_timestamp(text).map(|t| t.timestamp_millis()) else {
                    unknown += 1;
                    return;
                };
                if time < pivot - radius || time >= pivot + radius {
                    return;
                }
                usize::from(time >= pivot)
            } else {
                source_index
            };
            let pattern = PatternClusterer::abstract_line(text);
            if counts.len() >= 100000 && !counts.contains_key(&pattern) {
                overflow = true;
                return;
            }
            counts.entry(pattern).or_default()[side] += 1;
            totals[side] += 1;
        })?;
    }
    if overflow {
        return Err("More than 100000 patterns; narrow your filters or time range".into());
    }
    let mut patterns: Vec<_> = counts.into_iter().collect();
    patterns.sort_by(|(pa, a), (pb, b)| {
        let delta = |v: &[usize; 2]| {
            (v[1] as f64 / totals[1].max(1) as f64 - v[0] as f64 / totals[0].max(1) as f64).abs()
        };
        delta(b).total_cmp(&delta(a)).then_with(|| pa.cmp(pb))
    });
    let mut lines = vec![
        "Active filters; actual matching lines; patterns normalize timestamps, IDs and numbers."
            .into(),
    ];
    if let Some((pivot, radius)) = window {
        let time = |ms| {
            chrono::DateTime::from_timestamp_millis(ms)
                .unwrap()
                .to_rfc3339()
        };
        lines.push(format!(
            "Before: [{} .. {})",
            time(pivot - radius),
            time(pivot)
        ));
        lines.push(format!(
            "After:  [{} .. {})",
            time(pivot),
            time(pivot + radius)
        ));
    } else {
        lines.push(format!("Before/A: {}", sources[0].path.display()));
        lines.push(format!("After/B:  {}", sources[1].path.display()));
    }
    lines.push(format!(
        "Before/A: {} matches | After/B: {} | {} untimed excluded",
        totals[0], totals[1], unknown
    ));
    lines.push(
        "Sorted by absolute change in share. pp = percentage points; counts are not rates.".into(),
    );
    lines.push("   Before     After    Delta pp  Change       Pattern".into());
    for (pattern, values) in patterns.into_iter().take(100) {
        let delta = 100.0
            * (values[1] as f64 / totals[1].max(1) as f64
                - values[0] as f64 / totals[0].max(1) as f64);
        let label = match values {
            [0, _] => "NEW",
            [_, 0] => "DISAPPEARED",
            _ => "",
        };
        lines.push(format!(
            "{:>9} {:>9} {:+11.2}  {:<12} {}",
            values[0], values[1], delta, label, pattern
        ));
    }
    Ok(lines)
}
