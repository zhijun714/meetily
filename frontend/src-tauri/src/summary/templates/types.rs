use serde::{Deserialize, Serialize};

/// Legacy JSON sections remain readable; Markdown is authoritative when present.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSection {
    pub title: String,
    pub instruction: String,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example_item_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sections: Vec<TemplateSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
}

impl Template {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Template name cannot be empty".into());
        }
        if let Some(markdown) = &self.markdown {
            if markdown.trim().is_empty() || self.section_titles().is_empty() {
                return Err(
                    "Template Markdown must contain at least one ## section heading".into(),
                );
            }
            return Ok(());
        }
        if self.description.trim().is_empty() {
            return Err("Template description cannot be empty".into());
        }

        if self.sections.is_empty() {
            return Err("Template must have at least one section".into());
        }

        for (i, section) in self.sections.iter().enumerate() {
            if section.title.trim().is_empty() {
                return Err(format!("Section {} has empty title", i));
            }

            if section.instruction.trim().is_empty() {
                return Err(format!("Section '{}' has empty instruction", section.title));
            }
            if !matches!(section.format.as_str(), "paragraph" | "list" | "string") {
                return Err(format!(
                    "Section '{}' has invalid format '{}'",
                    section.title, section.format
                ));
            }
        }

        Ok(())
    }

    /// Lossless conversion of all legacy generation guidance into editable Markdown.
    pub fn to_markdown_source(&self) -> String {
        if let Some(markdown) = &self.markdown {
            return markdown.clone();
        }
        let mut source = format!("# <Add Title here>\n\n{}\n\nAnalyze the entire transcript and create a concise, descriptive title for the meeting.\n", self.description);
        for section in &self.sections {
            source.push_str(&format!(
                "\n## {}\n\n{}\n\n{}\n",
                section.title,
                section.instruction,
                output_format(&section.format)
            ));
            if let Some(hint) = &section.item_format {
                source.push_str(&format!("\nItem format:\n\n{}\n", hint));
            }
            if let Some(hint) = &section.example_item_format {
                source.push_str(&format!("\nExample item format:\n\n{}\n", hint));
            }
        }
        source
    }

    pub fn to_markdown_structure(&self) -> String {
        if let Some(markdown) = &self.markdown {
            return markdown.clone();
        }
        let mut structure = String::from("# <Add Title here>\n\n");
        for section in &self.sections {
            structure.push_str(&format!("## {}\n\n", section.title));
        }
        structure
    }

    pub fn to_section_instructions(&self) -> String {
        format!("Follow this Markdown template as the authoritative generation guidance. Preserve its section headings verbatim, including their language and order. Replace guidance with meeting content; do not repeat the instructions in the summary.\n\n{}", self.to_markdown_source())
    }

    /// H2 headings drive heading preservation and preview metadata for Markdown templates.
    pub fn section_titles(&self) -> Vec<String> {
        let Some(markdown) = &self.markdown else {
            return self
                .sections
                .iter()
                .map(|section| section.title.clone())
                .collect();
        };
        let mut fence: Option<(char, usize)> = None;
        markdown
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                let marker = line.chars().next().unwrap_or(' ');
                let count = line.chars().take_while(|&c| c == marker).count();
                if matches!(marker, '`' | '~') && count >= 3 {
                    match fence {
                        None => fence = Some((marker, count)),
                        Some((open, length))
                            if marker == open
                                && count >= length
                                && line[count..].trim().is_empty() =>
                        {
                            fence = None
                        }
                        _ => {}
                    }
                    return None;
                }
                if fence.is_some() {
                    return None;
                }
                let title = line
                    .strip_prefix("## ")
                    .or_else(|| line.strip_prefix("##\t"))?
                    .trim();
                let title = title.trim_end_matches('#').trim_end();
                if title.is_empty() {
                    None
                } else {
                    Some(title.to_owned())
                }
            })
            .collect()
    }
}

fn output_format(format: &str) -> &'static str {
    match format {
        "paragraph" => "Write prose paragraphs. Do not use a bullet list.",
        "list" => "Write a Markdown bullet list, with one item per point.",
        "string" => "Write a single concise line of plain text, without bullets or line breaks.",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy() -> Template {
        Template {
            name: "Legacy".into(),
            description: "Preserve uncertain findings".into(),
            markdown: None,
            sections: vec![TemplateSection {
                title: "Summary".into(),
                instruction: "Extract decisions without invention".into(),
                format: "paragraph".into(),
                item_format: Some("| Owner | Task |\n| --- | --- |".into()),
                example_item_format: Some("A separate legacy hint".into()),
            }],
        }
    }

    #[test]
    fn legacy_json_and_all_guidance_survive_markdown_conversion() {
        let template = legacy();
        let json = serde_json::to_string(&template).unwrap();
        assert!(!json.contains("markdown"));
        let loaded: Template = serde_json::from_str(&json).unwrap();
        assert!(loaded.validate().is_ok());
        let source = loaded.to_markdown_source();
        for text in [
            "Preserve uncertain findings",
            "## Summary",
            "Extract decisions without invention",
            "Write prose paragraphs",
            "| Owner | Task |",
            "A separate legacy hint",
            "concise, descriptive title",
        ] {
            assert!(source.contains(text), "Missing guidance: {text}");
            assert!(loaded.to_section_instructions().contains(text));
        }
        let mut converted = loaded;
        converted.markdown = Some(source.clone());
        converted.sections.clear();
        converted.description.clear();
        assert!(converted.validate().is_ok());
        assert_eq!(converted.to_markdown_structure(), source);
    }

    #[test]
    fn markdown_is_authoritative_and_round_trips_without_projection() {
        let source = "# Meeting\n\nKeep unknowns.\n\n## 重点事项\n- Brief points\n\n## 结论\nNo invented decisions.\n";
        let mut template: Template =
            serde_json::from_value(serde_json::json!({"name":"Mine", "markdown": source})).unwrap();
        template.sections = legacy().sections;
        assert!(template.validate().is_ok());
        assert_eq!(template.to_markdown_structure(), source);
        assert_eq!(template.section_titles(), vec!["重点事项", "结论"]);
        assert!(template.to_section_instructions().contains(source));
        assert!(!template
            .to_section_instructions()
            .contains("Extract decisions"));
        let loaded: Template =
            serde_json::from_str(&serde_json::to_string(&template).unwrap()).unwrap();
        assert_eq!(loaded.markdown.as_deref(), Some(source));
    }

    #[test]
    fn headings_ignore_code_examples_and_preserve_order() {
        let mut template = legacy();
        template.markdown = Some("## One\n```md\n## Example\n```\n~~~\n## Also example\n~~~\n## 二 ##\n### Detail\n## Three\n".into());
        assert_eq!(template.section_titles(), vec!["One", "二", "Three"]);
    }

    #[test]
    fn invalid_drafts_are_rejected_and_legacy_formats_are_retained() {
        for field in 0..4 {
            let mut value = legacy();
            match field {
                0 => value.name.clear(),
                1 => value.description.clear(),
                2 => value.sections[0].title.clear(),
                _ => value.sections[0].instruction.clear(),
            }
            assert!(value.validate().is_err());
        }
        let mut value = legacy();
        value.markdown = Some(" \n\t".into());
        assert!(value.validate().is_err());
        value.markdown = Some("Only instructions without ## headings".into());
        assert!(value.validate().is_err());
        value.markdown = None;
        value.sections[0].format = "html".into();
        assert!(value.validate().is_err());
        for (format, expected) in [
            ("paragraph", "prose paragraphs"),
            ("list", "Markdown bullet list"),
            ("string", "single concise line"),
        ] {
            value.sections[0].format = format.into();
            assert!(value.validate().is_ok());
            assert!(value.to_section_instructions().contains(expected));
        }
    }
}
