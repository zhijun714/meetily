//! Deterministic generated titles shared by summary persistence and document exports.
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use std::path::Path;

fn instant(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
        .or_else(|| {
            ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"]
                .iter()
                .find_map(|format| NaiveDateTime::parse_from_str(value, format).ok())
                .map(|time| time.and_utc())
        })
}

fn date_in<T: TimeZone>(value: &str, timezone: &T) -> Option<NaiveDate> {
    instant(value)
        .map(|time| time.with_timezone(timezone).date_naive())
        .or_else(|| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

/// Recording metadata records the start instant; database creation can be hours later.
/// Missing or malformed legacy metadata falls back to the saved creation timestamp.
pub fn recording_date(folder: Option<&str>, created: &str) -> Result<NaiveDate, String> {
    let recorded = folder
        .and_then(|folder| std::fs::read(Path::new(folder).join("metadata.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|metadata| {
            metadata
                .get("created_at")
                .and_then(|value| value.as_str())
                .and_then(|value| date_in(value, &Local))
        });
    recorded
        .or_else(|| date_in(created, &Local))
        .ok_or_else(|| "Meeting has no valid recording or creation date".into())
}

fn strip_date_prefix(value: &str) -> Option<&str> {
    for (length, format) in [(8, "%Y%m%d"), (10, "%Y-%m-%d")] {
        let Some(prefix) = value.get(..length) else {
            continue;
        };
        if NaiveDate::parse_from_str(prefix, format).is_ok() {
            if let Some(rest) = value
                .get(length..)
                .and_then(|rest| rest.strip_prefix(['-', '_', ' ']))
            {
                return Some(rest.trim_start());
            }
        }
    }
    None
}

pub fn canonical_title(date: NaiveDate, short_title: &str) -> String {
    let mut title = short_title
        .trim()
        .trim_matches(['#', '`', '"', '“', '”'])
        .trim();
    while let Some(rest) = strip_date_prefix(title) {
        title = rest;
    }
    let title = title
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let title: String = title.chars().take(20).collect();
    let title = title.trim_matches([' ', '.', '-']);
    format!(
        "{}-{}",
        date.format("%Y%m%d"),
        if title.is_empty() {
            "会议纪要"
        } else {
            title
        }
    )
}

/// The meeting title owns the single document H1. Preserve headings inside code fences
/// and demote other H1 content rather than dropping a user's substantive text.
pub fn with_title(markdown: &str, title: &str) -> String {
    let mut lines = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut leading = true;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = trimmed.chars().next().unwrap();
            let count = trimmed
                .chars()
                .take_while(|character| *character == marker)
                .count();
            match fence {
                None => fence = Some((marker, count)),
                Some((open, size)) if open == marker && count >= size => fence = None,
                _ => {}
            }
        }
        if fence.is_none() && trimmed.starts_with("# ") {
            if leading {
                continue;
            }
            if trimmed.trim_start_matches("# ").trim() == title {
                continue;
            }
            lines.push(format!("#{}", trimmed));
        } else {
            if !trimmed.is_empty() {
                leading = false;
            }
            lines.push(line.to_owned());
        }
    }
    // Setext H1 is also a document title when it precedes the body.
    while lines.len() >= 2
        && !lines[0].trim().is_empty()
        && lines[1].trim().chars().all(|c| c == '=')
        && lines[1].trim().len() >= 3
    {
        lines.drain(..2);
    }
    let body = lines.join("\n");
    let title = title.replace(['\r', '\n'], " ");
    if body.trim().is_empty() {
        format!("# {}\n", title)
    } else {
        format!("# {}\n\n{}\n", title, body.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_limit_and_repeated_dates_are_idempotent() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let title = canonical_title(
            date,
            "20261021-2026-10-20-这是一个用于验证中文字符截断不会破坏编码的长会议标题",
        );
        assert_eq!(title.strip_prefix("20261022-").unwrap().chars().count(), 20);
        assert_eq!(canonical_title(date, &title), title);
        assert_eq!(canonical_title(date, "\n# \"<>/\\\""), "20261022-会议纪要");
    }
    #[test]
    fn timestamps_use_local_calendar_day_and_metadata_precedes_creation() {
        let east = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
        let west = chrono::FixedOffset::west_opt(7 * 3600).unwrap();
        assert_eq!(
            date_in("2026-10-21T23:30:00Z", &east).unwrap().to_string(),
            "2026-10-22"
        );
        assert_eq!(
            date_in("2026-10-22T01:00:00Z", &west).unwrap().to_string(),
            "2026-10-21"
        );
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join("metadata.json"),
            r#"{"created_at":"2026-10-01T12:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(
            recording_date(folder.path().to_str(), "2026-10-22T12:00:00Z").unwrap(),
            date_in("2026-10-01T12:00:00Z", &Local).unwrap()
        );
        std::fs::write(folder.path().join("metadata.json"), "broken").unwrap();
        assert_eq!(
            recording_date(folder.path().to_str(), "2026-10-22 12:00:00").unwrap(),
            date_in("2026-10-22T12:00:00Z", &Local).unwrap()
        );
        assert!(recording_date(None, "invalid").is_err());
    }
    #[test]
    fn title_is_once_and_code_is_preserved() {
        let body = with_title(
            "# Old\n\n# Duplicate\n\n## 结论\n\nText\n\n# Detail\n\n```md\n# code\n```",
            "20261022-项目讨论",
        );
        assert!(body.starts_with("# 20261022-项目讨论\n\n## 结论"));
        assert!(body.contains("## Detail"));
        assert!(body.contains("```md\n# code\n```"));
        assert_eq!(with_title(&body, "20261022-项目讨论"), body);
        assert_eq!(with_title("Old\n===\n\nText", "New"), "# New\n\nText\n");
    }
}
