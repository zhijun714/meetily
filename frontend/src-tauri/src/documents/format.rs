use serde_json::Value;

pub fn filename(title: &str, created: &str, _id: &str, extension: &str) -> String {
    let date = chrono::NaiveDate::parse_from_str(created, "%Y%m%d")
        .ok()
        .or_else(|| crate::meeting_title::recording_date(None, created).ok());
    let existing_date = title
        .get(..8)
        .filter(|_| title.as_bytes().get(8) == Some(&b'-'))
        .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y%m%d").ok());
    let dated = existing_date.or(date).unwrap_or(chrono::NaiveDate::MIN);
    format!(
        "{}.{}",
        crate::meeting_title::canonical_title(dated, title),
        extension
    )
}

pub fn timestamp(seconds: f64) -> String {
    let seconds = if seconds.is_finite() { seconds.max(0.0).floor() as u64 } else { 0 };
    format!("{:02}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
}

fn escape_inline(text: &str) -> String {
    text.replace('\\', "\\\\").replace('*', "\\*").replace('_', "\\_").replace('[', "\\[").replace(']', "\\]").replace('<', "&lt;").replace('>', "&gt;").replace('`', "\\`")
}
fn inline(value: &Value) -> Result<String, String> {
    let array = value.as_array().ok_or(
        "Unsupported summary inline content; save the summary as Markdown before retrying.",
    )?;
    let mut output = String::new();
    for item in array {
        match item.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = item.get("text").and_then(Value::as_str).ok_or("Invalid summary text")?;
                let mut text = escape_inline(text);
                if let Some(styles) = item.get("styles").and_then(Value::as_object) {
                    if styles.iter().any(|(key, val)| {
                        val != &Value::Bool(false)
                            && !val.is_null()
                            && !matches!(
                                key.as_str(),
                                "bold"
                                    | "italic"
                                    | "strike"
                                    | "code"
                                    | "underline"
                                    | "textColor"
                                    | "backgroundColor"
                            )
                    }) {
                        return Err("Unsupported summary text style; save the summary as Markdown before retrying.".into());
                    }
                    if styles.get("code").and_then(Value::as_bool) == Some(true) {
                        let raw = item["text"].as_str().unwrap_or_default();
                        let fence = "`".repeat(raw.chars().filter(|c| *c == '`').count() + 1);
                        text = format!("{} {} {}", fence, raw, fence);
                    }
                    for (style, marker) in [("bold", "**"), ("italic", "*"), ("strike", "~~")] {
                        if styles.get(style).and_then(Value::as_bool) == Some(true) { text = format!("{}{}{}", marker, text, marker); }
                    }
                }
                output.push_str(&text);
            }
            Some("link") => {
                let href = item
                    .get("href")
                    .and_then(Value::as_str)
                    .ok_or("Invalid summary link")?;
                if href.contains(['\n', '\r', '<', '>']) {
                    return Err(
                        "Unsupported summary link; save the summary as Markdown before retrying."
                            .into(),
                    );
                }
                output.push_str(&format!("[{}](<{}>)", inline(&item["content"])?, href));
            }
            _ => return Err(
                "Unsupported summary inline content; save the summary as Markdown before retrying."
                    .into(),
            ),
        }
    }
    Ok(output)
}
fn block_markdown(blocks: &Value, depth: usize) -> Result<String, String> {
    if depth > 32 { return Err("Summary blocks are nested too deeply".into()); }
    let blocks = blocks.as_array().ok_or("Invalid summary blocks")?;
    let mut output = String::new();
    let mut number = 0;
    for block in blocks {
        let kind = block.get("type").and_then(Value::as_str).ok_or("Invalid summary block")?;
        let prefix = match kind {
            "paragraph" => String::new(),
            "heading" => {
                let level = block.pointer("/props/level").and_then(Value::as_u64).unwrap_or(1);
                if !(1..=6).contains(&level) { return Err("Unsupported summary heading level".into()); }
                format!("{} ", "#".repeat(level as usize))
            }
            "bulletListItem" => "- ".into(),
            "numberedListItem" => { number += 1; format!("{}. ", number) }
            "checkListItem" => if block.pointer("/props/checked").and_then(Value::as_bool) == Some(true) { "- [x] ".into() } else { "- [ ] ".into() },
            _ => return Err(format!("Unsupported summary block '{}'; open the summary and save it as Markdown before retrying export.", kind)),
        };
        if kind != "numberedListItem" { number = 0; }
        let body = inline(block.get("content").unwrap_or(&Value::Array(Vec::new())))?;
        if !body.trim().is_empty() {
            output.push_str(&prefix);
            output.push_str(&body.replace('\n', &format!("\n{}", " ".repeat(prefix.len()))));
            output.push_str("\n\n");
        }
        if let Some(children) = block.get("children") {
            let nested = block_markdown(children, depth + 1)?;
            for line in nested.lines() { output.push_str("    "); output.push_str(line); output.push('\n'); }
        }
    }
    Ok(output)
}

/// Never use cached English or an older markdown when the latest edit contains only blocks.
/// Convert only known BlockNote shapes; unsupported content leaves the existing file intact.
pub fn summary_markdown(value: &Value) -> Result<String, String> {
    if let Some(markdown) = value.get("markdown") {
        let markdown = markdown.as_str().unwrap_or("").trim();
        if markdown.is_empty() {
            return Err("The saved summary has no Markdown. Open the summary and save it again before retrying.".into());
        }
        return Ok(format!("{}\n", markdown));
    }
    if let Some(blocks) = value.get("summary_json") {
        let markdown = block_markdown(blocks, 0)?;
        if markdown.trim().is_empty() { return Err("The latest summary edit has no visible content; save the summary again before retrying.".into()); }
        return Ok(markdown);
    }
    // Supported historical section format: preserve explicit section order and visible content.
    let object = value.as_object().ok_or("Unsupported summary format")?;
    let keys: Vec<&str> = if let Some(order) = object.get("_section_order").and_then(Value::as_array) {
        order.iter().filter_map(Value::as_str).collect()
    } else { object.keys().map(String::as_str).collect() };
    let mut output = String::new();
    for key in keys {
        if key == "MeetingName" || key.starts_with('_') { continue; }
        let Some(blocks) = object.get(key).and_then(|v| v.get("blocks")).and_then(Value::as_array) else { continue; };
        let mut section = String::new();
        for block in blocks {
            let Some(content) = block.get("content").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) else { continue; };
            if matches!(block.get("type").and_then(Value::as_str), Some("bullet" | "bulletListItem" | "list")) { section.push_str("- "); }
            section.push_str(content.trim());
            section.push_str("\n\n");
        }
        let title = object.get(key).and_then(|value| value.get("title")).and_then(Value::as_str).unwrap_or(key);
        if !section.is_empty() { output.push_str(&format!("## {}\n\n{}", title, section)); }
    }
    if output.trim().is_empty() {
        Err(
            "The saved summary has no exportable content; save the summary again before retrying."
                .into(),
        )
    } else {
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn safe_unique_names_and_timestamps() {
        let a = filename("../题目: a", "2026-10-21T00:00:00Z", "meeting-a", "md");
        assert!(!a.contains('/'));
        assert_eq!(
            a,
            filename("../题目: a", "2026-10-21T00:00:00Z", "meeting-b", "md")
        );
        assert_eq!(
            filename("20261022-转写优化", "20261021", "id", "md"),
            "20261022-转写优化.md"
        );
        assert_eq!(timestamp(3661.9), "01:01:01");
        assert_eq!(timestamp(-3.0), "00:00:00");
    }
    #[test]
    fn blocknote_and_legacy_preserve_current_edits() {
        let body = summary_markdown(&json!({"summary_json":[
            {"type":"heading","props":{"level":2},"content":[{"type":"text","text":"Decisions"}]},
            {"type":"checkListItem","props":{"checked":true},"content":[{"type":"text","text":"Ship","styles":{"bold":true}}],"children":[
                {"type":"paragraph","content":[{"type":"link","href":"https://example.com","content":[{"type":"text","text":"details"}]}]}
            ]},
            {"type":"numberedListItem","content":[{"type":"text","text":"first"}]},
            {"type":"numberedListItem","content":[{"type":"text","text":"second"}]}
        ]})).unwrap();
        assert!(body.contains("## Decisions"));
        assert!(body.contains("- [x] **Ship**"));
        assert!(body.contains("    [details](<https://example.com>)"));
        assert!(body.contains("2. second"));
        assert!(summary_markdown(&json!({"summary_json":[{"type":"table","content":{}}]})).unwrap_err().contains("Unsupported summary block"));
        assert!(summary_markdown(&json!({"summary_json":[]})).is_err());
        assert_eq!(summary_markdown(&json!({"key":{"title":"Human title","blocks":[{"content":"legacy"}]}})).unwrap(), "## Human title\n\nlegacy\n\n");
    }
    #[test]
    fn summary_formats_never_fall_back_to_cached_or_empty_content() {
        assert_eq!(summary_markdown(&json!({"markdown":"new", "english_cache":{"markdown":"old"}})).unwrap(), "new\n");
        assert!(summary_markdown(&json!({"summary_json":[{"content":"new"}], "english_cache":{"markdown":"old"}})).is_err());
        assert!(summary_markdown(&json!({"markdown":"  ","summary_json":[{"content":"new"}]})).is_err());
        assert_eq!(summary_markdown(&json!({"Notes":{"blocks":[{"content":"legacy"}]}})).unwrap(), "## Notes\n\nlegacy\n\n");
    }
}
