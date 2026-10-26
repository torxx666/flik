//! Bounded field/value completion from the beginning and live edge of a log.
use crate::{format::FormatManager, reader::LogReader};
use std::collections::BTreeSet;

pub fn candidates(input: &str, reader: &LogReader, format: &FormatManager) -> Vec<String> {
    // Complete the final expression term; keep boolean expressions intact.
    let start = input
        .rfind(|c: char| c.is_whitespace() || c == '(')
        .map_or(0, |i| i + 1);
    let (head, term) = input.split_at(start);
    let (prefix, term) = if let Some(term) = term.strip_prefix("//") {
        ("//", term)
    } else if let Some(term) = term.strip_prefix('/') {
        ("/", term)
    } else {
        ("", term)
    };
    let field_value = term.split_once(':');
    let mut values = BTreeSet::new();
    if field_value.is_none() {
        for field in &format.available_fields {
            if !field.starts_with("field_")
                && field.to_lowercase().starts_with(&term.to_lowercase())
            {
                values.insert(format!("{head}{prefix}{field}:"));
            }
        }
    }
    let total = reader.line_count();
    let mut sample = reader.read_range(0, total.min(250));
    if total > 250 {
        sample.extend(reader.read_range(total.saturating_sub(250).max(250), 250));
    }
    for (_, line) in sample {
        let record = format.parse_line(&line);
        let mut fields = record.fields;
        if let Some(level) = record.level {
            fields.push(crate::format::ParsedField {
                key: "level".into(),
                value: level,
            });
        }
        if let Some(status) = record.status {
            fields.push(crate::format::ParsedField {
                key: "status".into(),
                value: status.to_string(),
            });
        }
        for field in fields {
            if field.key.starts_with("field_") {
                continue;
            }
            if let Some((key, partial)) = field_value {
                if field.key.eq_ignore_ascii_case(key)
                    && !field.value.is_empty()
                    && field.value.len() <= 200
                    && !field.value.chars().any(char::is_control)
                    && field
                        .value
                        .to_lowercase()
                        .starts_with(&partial.trim_matches('"').to_lowercase())
                {
                    // Escape-free quoted values only; avoid producing invalid query syntax.
                    if field.value.contains('"') || field.value.contains('\\') {
                        continue;
                    }
                    let value = if field
                        .value
                        .chars()
                        .any(|c| c.is_whitespace() || c == '(' || c == ')')
                    {
                        format!("\"{}\"", field.value)
                    } else {
                        field.value
                    };
                    values.insert(format!("{head}{prefix}{key}:{value}"));
                }
            } else if field.key.to_lowercase().starts_with(&term.to_lowercase()) {
                values.insert(format!("{head}{prefix}{}:", field.key));
            }
        }
    }
    values.into_iter().take(100).collect()
}
