use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::filter::FilterEngine;
use crate::format::FormatManager;
use crate::reader::LogReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Raw,
    JsonLines,
    Csv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScope {
    AllFiltered,
    Range(usize, usize),
}

pub struct ExportEngine;

impl ExportEngine {
    pub fn export_to_file<P: AsRef<Path>>(
        output_path: P,
        reader: &LogReader,
        filter: &FilterEngine,
        format_mgr: &FormatManager,
        scope: ExportScope,
        export_fmt: ExportFormat,
    ) -> Result<usize, String> {
        if reader
            .is_indexing
            .load(std::sync::atomic::Ordering::Acquire)
            || filter
                .is_filtering
                .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err("Wait for indexing and filtering to finish before exporting".into());
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_path)
            .map_err(|e| format!("Failed to create file: {}", e))?;
        let mut writer = BufWriter::new(file);

        let source_revision = reader.revision.load(std::sync::atomic::Ordering::Acquire);
        let total_count = filter.filtered_count(reader);
        let (start_idx, end_idx) = match scope {
            ExportScope::AllFiltered => (0, total_count),
            ExportScope::Range(s, e) => (s.min(total_count), e.min(total_count)),
        };

        if start_idx >= end_idx {
            return Ok(0);
        }

        // CSV Header if CSV format
        if export_fmt == ExportFormat::Csv {
            let header = format_mgr
                .visible_fields
                .iter()
                .map(|field| csv_escape(field))
                .collect::<Vec<_>>()
                .join(",");
            writeln!(writer, "{}", header).map_err(|e| e.to_string())?;
        }

        let mut exported = 0;
        let chunk_size = 500;
        let mut current = start_idx;

        while current < end_idx {
            let limit = (current + chunk_size).min(end_idx);
            let indices: Vec<usize> = (current..limit)
                .filter_map(|i| filter.get_original_line_index(i, reader))
                .collect();
            for line in reader.read_lines(&indices) {
                let line = line.ok_or_else(|| {
                    "Cannot read an indexed log line; export is incomplete".to_string()
                })?;
                {
                    match export_fmt {
                        ExportFormat::Raw => {
                            writeln!(writer, "{}", line).map_err(|e| e.to_string())?;
                        }
                        ExportFormat::JsonLines => {
                            let record = format_mgr.parse_line(&line);
                            let mut map = serde_json::Map::new();
                            for field in &format_mgr.visible_fields {
                                if let Some(val) = record.get_field_value(field) {
                                    map.insert(
                                        field.clone(),
                                        serde_json::Value::String(val.to_string()),
                                    );
                                }
                            }
                            let json_str =
                                serde_json::to_string(&map).map_err(|e| e.to_string())?;
                            writeln!(writer, "{}", json_str).map_err(|e| e.to_string())?;
                        }
                        ExportFormat::Csv => {
                            let record = format_mgr.parse_line(&line);
                            let row_values: Vec<String> = format_mgr
                                .visible_fields
                                .iter()
                                .map(|f| {
                                    let val = record.get_field_value(f).unwrap_or("");
                                    csv_escape(val)
                                })
                                .collect();
                            writeln!(writer, "{}", row_values.join(","))
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    exported += 1;
                }
            }
            current = limit;
        }

        if reader.revision.load(std::sync::atomic::Ordering::Acquire) != source_revision {
            return Err("Source changed during export; export is incomplete".into());
        }
        writer.flush().map_err(|e| e.to_string())?;
        Ok(exported)
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}
