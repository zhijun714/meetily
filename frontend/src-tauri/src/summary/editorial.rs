//! Scoped conclusion editing and stable section boundaries.
use std::ops::Range;

pub(crate) const POLICY_VERSION: &str = "summary-2026-10-22-markdown-humanizer-v1";
pub(crate) const HUMANIZER_RULES: &str = include_str!("humanizer-zh.md");

fn heading(line: &str) -> Option<(usize, &str)> {
    let line = line.trim();
    let level = line.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&level) && line.as_bytes().get(level) == Some(&b' ') {
        Some((level, line[level + 1..].trim().trim_end_matches('#').trim()))
    } else if line.starts_with("**") && line.ends_with("**") && line.len() > 4 {
        Some((2, line[2..line.len() - 2].trim()))
    } else {
        None
    }
}

fn is_conclusion(title: &str) -> bool {
    matches!(
        title.trim_end_matches([':', '：']).to_lowercase().as_str(),
        "结论" | "結論" | "conclusion" | "conclusions"
    )
}

/// Ignore fenced examples and stop at the next sibling/parent section.
pub(crate) fn conclusion_range(markdown: &str) -> Option<Range<usize>> {
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    let mut start = None;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim();
        let marker = trimmed.as_bytes().first().copied();
        let count = marker.map_or(0, |m| trimmed.bytes().take_while(|b| *b == m).count());
        if matches!(marker, Some(b'`' | b'~')) && count >= 3 {
            match fence {
                Some((m, n))
                    if marker == Some(m) && count >= n && trimmed[count..].trim().is_empty() =>
                {
                    fence = None
                }
                None => fence = marker.map(|m| (m, count)),
                _ => {}
            }
        } else if fence.is_none() {
            if let Some((level, title)) = heading(trimmed) {
                if let Some((body_start, section_level)) = start {
                    if level <= section_level {
                        return trim_range(markdown, body_start..offset);
                    }
                } else if is_conclusion(title) {
                    start = Some((offset + line.len(), level));
                }
            }
        }
        offset += line.len();
    }
    start.and_then(|(body_start, _)| trim_range(markdown, body_start..markdown.len()))
}

fn trim_range(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let body = &text[range.clone()];
    let start = range.start + body.len() - body.trim_start().len();
    let end = range.end - (body.len() - body.trim_end().len());
    (start < end).then_some(start..end)
}

fn matching_section_title(actual: &str, expected: &str) -> bool {
    let actual = actual.trim().trim_end_matches([':', '：']).to_lowercase();
    let expected = expected.trim().trim_end_matches([':', '：']).to_lowercase();
    if actual == expected {
        return true;
    }
    let translations: &[(&str, &[&str])] = &[
        ("重点事项", &["highlights", "key points", "key items"]),
        ("問題", &["issues", "problems"]),
        ("问题", &["issues", "problems"]),
        ("待办", &["action items", "to-do", "todo"]),
        ("结论", &["conclusion", "conclusions"]),
        ("結論", &["conclusion", "conclusions"]),
    ];
    translations.iter().any(|(title, aliases)| {
        (expected == *title && aliases.contains(&actual.as_str()))
            || (actual == *title && aliases.contains(&expected.as_str()))
    })
}

/// A conclusion may be polished only when every template section still maps to
/// the generated section in its original position.
pub(crate) fn section_titles_match(markdown: &str, titles: &[String]) -> bool {
    let mut fence: Option<(u8, usize)> = None;
    let mut actual = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        let marker = trimmed.as_bytes().first().copied();
        let count = marker.map_or(0, |m| trimmed.bytes().take_while(|b| *b == m).count());
        if matches!(marker, Some(b'`' | b'~')) && count >= 3 {
            match fence {
                Some((m, n))
                    if marker == Some(m) && count >= n && trimmed[count..].trim().is_empty() =>
                {
                    fence = None
                }
                None => fence = marker.map(|m| (m, count)),
                _ => {}
            }
        } else if fence.is_none() && trimmed.starts_with("## ") {
            actual.push(trimmed[3..].trim());
        }
    }
    !titles.is_empty()
        && actual.len() == titles.len()
        && actual
            .iter()
            .zip(titles)
            .all(|(a, e)| matching_section_title(a, e))
}

/// Restore user-defined headings after translation only when all boundaries agree.
pub(crate) fn restore_section_titles(markdown: &str, titles: &[String]) -> String {
    let mut fence: Option<(u8, usize)> = None;
    let mut positions = Vec::new();
    let mut offset = 0;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim();
        let marker = trimmed.as_bytes().first().copied();
        let count = marker.map_or(0, |m| trimmed.bytes().take_while(|b| *b == m).count());
        if matches!(marker, Some(b'`' | b'~')) && count >= 3 {
            match fence {
                Some((m, n))
                    if marker == Some(m) && count >= n && trimmed[count..].trim().is_empty() =>
                {
                    fence = None
                }
                None => fence = marker.map(|m| (m, count)),
                _ => {}
            }
        } else if fence.is_none() && trimmed.starts_with("## ") {
            positions.push((
                offset,
                offset + line.len(),
                line.ends_with('\n'),
                trimmed[3..].trim(),
            ));
        }
        offset += line.len();
    }
    if titles.is_empty()
        || positions.len() != titles.len()
        || positions
            .iter()
            .zip(titles)
            .any(|((_, _, _, actual), expected)| !matching_section_title(actual, expected))
    {
        return markdown.to_string();
    }
    let mut result = String::with_capacity(markdown.len());
    let mut cursor = 0;
    for ((start, end, newline, _), title) in positions.into_iter().zip(titles) {
        result.push_str(&markdown[cursor..start]);
        result.push_str("## ");
        result.push_str(title);
        if newline {
            result.push('\n');
        }
        cursor = end;
    }
    result.push_str(&markdown[cursor..]);
    result
}

fn list_shape(text: &str) -> Vec<(usize, &str)> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            let marker = if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") {
                "checkbox"
            } else if trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("+ ")
            {
                "bullet"
            } else if trimmed.split_once(". ").is_some_and(|(prefix, _)| {
                !prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_digit())
            }) {
                "numbered"
            } else {
                return None;
            };
            Some((indent, marker))
        })
        .collect()
}

pub(crate) fn replace_conclusion(
    markdown: &str,
    range: Range<usize>,
    replacement: &str,
) -> Result<String, String> {
    let replacement = replacement.trim();
    if list_shape(&markdown[range.clone()]) != list_shape(replacement) {
        return Err("Conclusion edit changed the list structure".into());
    }
    if replacement.is_empty()
        || replacement.lines().any(|line| {
            let text = line.trim();
            text.starts_with('#')
                || text.starts_with("```")
                || text.starts_with("~~~")
                || heading(line).is_some()
        })
    {
        return Err("Conclusion edit was empty or introduced section boundaries".into());
    }
    // Preserve literal facts that a style edit must never change.
    let protected = regex::Regex::new(r"`[^`]+`|https?://[^\s)\]>]+|\d+(?:[./:%-]\d+)*").unwrap();
    let original_tokens: Vec<_> = protected
        .find_iter(&markdown[range.clone()])
        .map(|m| m.as_str().to_string())
        .collect();
    let revised_tokens: Vec<_> = protected
        .find_iter(replacement)
        .map(|m| m.as_str().to_string())
        .collect();
    if original_tokens != revised_tokens {
        return Err("Conclusion edit changed protected literals".into());
    }
    let names = regex::Regex::new(r"\[[^\]]+\]\([^\)]+\)").unwrap();
    let old_links: Vec<_> = names
        .find_iter(&markdown[range.clone()])
        .map(|m| m.as_str())
        .collect();
    let new_links: Vec<_> = names.find_iter(replacement).map(|m| m.as_str()).collect();
    if old_links != new_links {
        return Err("Conclusion edit changed Markdown links".into());
    }
    let mut result = String::with_capacity(markdown.len() + replacement.len());
    result.push_str(&markdown[..range.start]);
    result.push_str(replacement);
    result.push_str(&markdown[range.end..]);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_only_conclusion_preserving_other_bytes() {
        let doc =
            "# 标题\r\n\r\n## 重点事项\r\n- 内容\r\n\r\n## 结论\r\n原结论。\r\n\r\n## 附录\r\n保留";
        let range = conclusion_range(doc).unwrap();
        assert_eq!(&doc[range.clone()], "原结论。");
        assert_eq!(
            replace_conclusion(doc, range, "新表达。").unwrap(),
            doc.replace("原结论。", "新表达。")
        );
    }
    #[test]
    fn ignores_fences_and_supports_legacy_headings() {
        let doc = "```md\n## 结论\nexample\n```\n**结论**\n- 已确认。\n**待办**\n- 保留。";
        let range = conclusion_range(doc).unwrap();
        assert_eq!(&doc[range], "- 已确认。");
        assert!(conclusion_range("## 结论\n \n").is_none());
        assert!(conclusion_range("## 问题\n未定").is_none());
    }
    #[test]
    fn rejects_structural_changes_and_changed_literals() {
        let doc = "## 结论\n周五完成 2 项 `api`，见 https://example.com。";
        for invalid in ["", "## 新章节\n内容", "完成 3 项 `api`。", "<think>reason"] {
            // Reasoning is additionally rejected by require_visible_markdown in the caller.
            if invalid.starts_with("<think>") {
                continue;
            }
            assert!(replace_conclusion(doc, conclusion_range(doc).unwrap(), invalid).is_err());
        }
    }
    #[test]
    fn rejects_changed_link_targets_and_keeps_original_section_intact() {
        let doc = "## 结论\n- 详情见 [原文](https://example.com)。\n\n## 待办\n- 保留";
        assert!(replace_conclusion(
            doc,
            conclusion_range(doc).unwrap(),
            "- 详情见 [原文](https://other.com)。"
        )
        .is_err());
        assert_eq!(
            &doc[conclusion_range(doc).unwrap()],
            "- 详情见 [原文](https://example.com)。"
        );
    }
    #[test]
    fn restores_template_heading_names_only_for_matching_structure() {
        let titles = vec!["重点事项".into(), "结论".into()];
        assert_eq!(
            restore_section_titles("# 标题\n## Highlights\nx\n## Conclusion\ny", &titles),
            "# 标题\n## 重点事项\nx\n## 结论\ny"
        );
        assert_eq!(
            restore_section_titles("## Extra\nx", &titles),
            "## Extra\nx"
        );
        assert!(!section_titles_match(
            "## Highlights\nx\n## Appendix\ny",
            &titles
        ));
        assert_eq!(
            restore_section_titles("## Highlights\nx\n## Appendix\ny", &titles),
            "## Highlights\nx\n## Appendix\ny"
        );
        assert!(section_titles_match(
            "## Highlights\nx\n## Conclusion\ny",
            &titles
        ));
    }
    #[test]
    fn conclusion_edit_preserves_list_structure() {
        let doc = "## 结论\n- 决定一\n  - 子项\n- 决定二";
        let range = conclusion_range(doc).unwrap();
        assert!(replace_conclusion(doc, range.clone(), "决定一。决定二。").is_err());
        assert!(replace_conclusion(doc, range.clone(), "- 决定一\n- 子项\n- 决定二").is_err());
        assert!(replace_conclusion(doc, range, "- 已决定一\n  - 子项\n- 已决定二").is_ok());
    }
}
