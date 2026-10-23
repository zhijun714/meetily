use super::types::Template;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const FALLBACK_TEMPLATE: &str = "standard_meeting";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TemplateSource {
    Builtin,
    Bundled,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateEditor {
    pub id: String,
    pub source: TemplateSource,
    pub template: Template,
    #[serde(default)]
    pub is_override: bool,
}

/// IDs are filenames, never paths. A conservative ASCII grammar also works on Windows.
pub fn validate_template_id(id: &str) -> Result<(), String> {
    // Existing templates may contain Unicode, spaces or dots. Permit safe single
    // filename components, but reject Windows separators/streams and ambiguous suffixes too.
    if id.trim().is_empty()
        || id.len() > 200
        || id == "."
        || id == ".."
        || id.ends_with('.')
        || id.ends_with(' ')
        || id.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
    {
        return Err(
            "Template ID must be a safe filename without path separators or reserved characters"
                .into(),
        );
    }
    let upper = id.split('.').next().unwrap_or(id).to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") ||
        (upper.len() == 4 && (upper.starts_with("COM") || upper.starts_with("LPT")) && matches!(upper.as_bytes()[3], b'1'..=b'9')) {
        return Err("Template ID is a reserved filename".into());
    }
    Ok(())
}

pub struct TemplateRepository {
    custom: PathBuf,
    bundled: Option<PathBuf>,
}

impl TemplateRepository {
    pub fn new(custom: PathBuf, bundled: Option<PathBuf>) -> Self { Self { custom, bundled } }

    fn regular_file(path: &Path) -> Result<bool, String> {
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_file() => Ok(true),
            Ok(_) => {
                Err("Template path must be a regular file, not a symbolic link or directory".into())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("Cannot inspect template: {e}")),
        }
    }

    fn check_custom_dir(&self) -> Result<(), String> {
        match fs::symlink_metadata(&self.custom) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
            Ok(_) => {
                Err("Custom template directory must be a directory, not a symbolic link".into())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Cannot inspect custom template directory: {e}")),
        }
    }

    pub fn get(&self, id: &str) -> Result<TemplateEditor, String> {
        validate_template_id(id)?;
        self.check_custom_dir()?;
        let filename = format!("{id}.json");
        let custom = self.custom.join(&filename);
        let (source, json) = if Self::regular_file(&custom)? {
            (
                TemplateSource::Custom,
                fs::read_to_string(custom).map_err(|e| e.to_string())?,
            )
        } else if let Some(path) = self
            .bundled
            .as_ref()
            .map(|p| p.join(&filename))
            .filter(|p| p.is_file())
        {
            (
                TemplateSource::Bundled,
                fs::read_to_string(path).map_err(|e| e.to_string())?,
            )
        } else if let Some(json) = super::defaults::get_builtin_template(id) {
            (TemplateSource::Builtin, json.to_owned())
        } else { return Err(format!("Template '{id}' not found")); };
        let template = super::loader::validate_and_parse_template(&json)?;
        let is_override = source == TemplateSource::Custom && self.has_base(id);
        Ok(TemplateEditor {
            id: id.into(),
            source,
            template,
            is_override,
        })
    }

    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = super::defaults::list_builtin_template_ids()
            .into_iter()
            .map(str::to_owned)
            .collect();
        for dir in self.bundled.iter().chain(std::iter::once(&self.custom)) {
            if dir == &self.custom && self.check_custom_dir().is_err() { continue; }
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(id) = entry.path().file_name().and_then(|f| f.to_str()).and_then(|f| f.strip_suffix(".json")) {
                        if validate_template_id(id).is_ok() { ids.push(id.into()); }
                    }
                }
            }
        }
        ids.sort(); ids.dedup(); ids
    }

    fn writable_path(&self, id: &str) -> Result<PathBuf, String> {
        validate_template_id(id)?;
        self.check_custom_dir()?;
        let path = self.custom.join(format!("{id}.json"));
        // Existing overrides belong to the user and can be edited/deleted without
        // touching the bundled or embedded copy beneath them.
        if Self::regular_file(&path)? { return Ok(path); }
        // Protect reserved IDs case-insensitively on every platform.
        if super::defaults::list_builtin_template_ids()
            .iter()
            .any(|known| known.eq_ignore_ascii_case(id))
            || self
                .bundled
                .as_ref()
                .and_then(|dir| fs::read_dir(dir).ok())
                .map(|entries| {
                    entries.flatten().any(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .map(|name| name.eq_ignore_ascii_case(&format!("{id}.json")))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        {
            return Err(
                "Built-in and bundled templates are read-only; duplicate with a new ID".into(),
            );
        }
        Ok(self.custom.join(format!("{id}.json")))
    }

    fn has_base(&self, id: &str) -> bool {
        super::defaults::get_builtin_template(id).is_some()
            || self
                .bundled
                .as_ref()
                .map(|dir| dir.join(format!("{id}.json")).is_file())
                .unwrap_or(false)
    }

    /// Explicit opt-in to create a personal override under an exact existing base ID.
    pub fn save_override(&self, id: &str, template: &Template) -> Result<TemplateEditor, String> {
        validate_template_id(id)?;
        template.validate()?;
        self.check_custom_dir()?;
        if !self.has_base(id) {
            return Err("No built-in or bundled template exists under this ID".into());
        }
        // get() also validates the underlying template before allowing an override.
        let base = self.get(id)?;
        if base.source == TemplateSource::Custom {
            return Err("A personal override already exists; reload before saving".into());
        }
        self.persist(id, template, true, self.custom.join(format!("{id}.json")))
    }

    /// create=false only updates an existing custom file; create=true never overwrites.
    pub fn save(
        &self,
        id: &str,
        template: &Template,
        create: bool,
    ) -> Result<TemplateEditor, String> {
        template.validate()?;
        if create && (id.is_empty() || id.len() > 100 || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')) {
            return Err("New template IDs must contain 1–100 ASCII letters, numbers, underscores or hyphens".into());
        }
        let path = self.writable_path(id)?;
        let exists = Self::regular_file(&path)?;
        if exists == create {
            return Err(if create {
                "Template ID already exists"
            } else {
                "Custom template no longer exists"
            }
            .into());
        }
        self.persist(id, template, create, path)
    }

    fn persist(
        &self,
        id: &str,
        template: &Template,
        create: bool,
        path: PathBuf,
    ) -> Result<TemplateEditor, String> {
        fs::create_dir_all(&self.custom)
            .map_err(|e| format!("Cannot create template directory: {e}"))?;
        self.check_custom_dir()?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.custom).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(&mut temp, template).map_err(|e| e.to_string())?;
        temp.write_all(b"\n").map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        Self::regular_file(&path)?;
        if create {
            temp.persist_noclobber(&path).map_err(|e| e.to_string())?;
        } else {
            temp.persist(&path).map_err(|e| e.to_string())?;
        }
        Ok(TemplateEditor {
            id: id.into(),
            source: TemplateSource::Custom,
            template: template.clone(),
            is_override: self.has_base(id),
        })
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.writable_path(id)?;
        if !Self::regular_file(&path)? { return Err("Custom template no longer exists".into()); }
        fs::remove_file(path).map_err(|e| format!("Cannot delete template: {e}"))
    }

    pub fn resolve_default(&self, saved: Option<&str>) -> String {
        saved.filter(|id| self.get(id).is_ok()).unwrap_or(FALLBACK_TEMPLATE).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn template() -> Template {
        serde_json::from_str(r#"{"name":"Test","description":"A legacy template","sections":[{"title":"Summary","instruction":"Summarize","format":"list"}]}"#).unwrap()
    }

    #[test]
    fn template_crud_preserves_hints_and_refuses_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let repo = TemplateRepository::new(dir.path().join("custom"), None);
        let mut value = template();
        if let Some(section) = value.sections.get_mut(0) {
            section.item_format = Some("- {owner}: {task}".into());
        }
        value.sections[0].example_item_format = Some("legacy example".into());
        repo.save("my_template", &value, true).unwrap();
        assert!(repo.save("my_template", &template(), true).is_err());
        let read = repo.get("my_template").unwrap();
        assert_eq!(read.source, TemplateSource::Custom);
        assert_eq!(read.template.sections[0].example_item_format, value.sections[0].example_item_format);
        value.name = "Updated".into();
        repo.save("my_template", &value, false).unwrap();
        assert_eq!(repo.get("my_template").unwrap().template.name, "Updated");
        assert_eq!(repo.resolve_default(Some("my_template")), "my_template");
        repo.delete("my_template").unwrap();
        assert_eq!(repo.resolve_default(Some("my_template")), FALLBACK_TEMPLATE);
        assert!(repo.save("my_template", &value, false).is_err());
    }

    #[test]
    fn template_paths_and_readonly_sources_are_protected() {
        let dir = tempfile::tempdir().unwrap();
        let bundled = dir.path().join("bundled");
        fs::create_dir(&bundled).unwrap();
        fs::write(
            bundled.join("project.json"),
            serde_json::to_vec(&template()).unwrap(),
        )
        .unwrap();
        let repo = TemplateRepository::new(dir.path().join("custom"), Some(bundled));
        for id in [
            "../escape",
            "a/b",
            "a\\b",
            ".",
            "",
            "CON",
            "CON.txt",
            "LPT1",
            "trailing.",
            "trailing ",
            "C:drive",
            "nul\0byte",
        ] {
            assert!(repo.get(id).is_err());
            assert!(repo.save(id, &template(), true).is_err());
            assert!(repo.delete(id).is_err());
        }
        for id in ["standard_meeting", "STANDARD_MEETING", "project", "PROJECT"] {
            assert!(repo.save(id, &template(), true).is_err()); assert!(repo.delete(id).is_err());
        }
        assert_eq!(repo.get("project").unwrap().source, TemplateSource::Bundled);
        let copy = repo.get("standard_meeting").unwrap().template;
        repo.save("standard_copy", &copy, true).unwrap();
        assert_eq!(repo.get("standard_copy").unwrap().source, TemplateSource::Custom);
    }

    #[test]
    fn invalid_template_does_not_replace_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let repo = TemplateRepository::new(dir.path().into(), None);
        repo.save("mine", &template(), true).unwrap();
        let before = fs::read(dir.path().join("mine.json")).unwrap();
        let mut invalid = template(); invalid.sections[0].instruction = " \n\t".into();
        assert!(repo.save("mine", &invalid, false).is_err());
        assert_eq!(fs::read(dir.path().join("mine.json")).unwrap(), before);
        assert_eq!(repo.resolve_default(Some("../bad")), FALLBACK_TEMPLATE);
    }

    #[test]
    fn template_legacy_names_and_custom_overrides_survive() {
        let dir = tempfile::tempdir().unwrap();
        let repo = TemplateRepository::new(dir.path().into(), None);
        for id in ["team.notes", "Team Meeting", "周会模板", "standard_meeting"] {
            fs::write(
                dir.path().join(format!("{id}.json")),
                serde_json::to_vec(&template()).unwrap(),
            )
            .unwrap();
            assert!(repo.ids().contains(&id.to_string()));
            assert_eq!(repo.get(id).unwrap().source, TemplateSource::Custom);
            let mut value = template(); value.name = "Edited legacy template".into();
            repo.save(id, &value, false).unwrap();
            assert_eq!(repo.get(id).unwrap().template.name, "Edited legacy template");
            repo.delete(id).unwrap();
        }
        assert_ne!(repo.get("standard_meeting").unwrap().source, TemplateSource::Custom);
        assert!(repo.save("standard_meeting", &template(), true).is_err());
    }

    #[test]
    fn built_in_personal_override_can_be_restored_without_changing_embedded_template() {
        let dir = tempfile::tempdir().unwrap();
        let repo = TemplateRepository::new(dir.path().into(), None);
        let original = repo.get("standard_meeting").unwrap();
        let mut personal = original.template.clone();
        personal.markdown = Some("# <Title>\n\n## 私人规则\n只写重点。".into());
        let saved = repo.save_override("standard_meeting", &personal).unwrap();
        assert_eq!(saved.source, TemplateSource::Custom);
        assert!(saved.is_override);
        assert!(repo.save_override("standard_meeting", &personal).is_err());
        repo.delete("standard_meeting").unwrap();
        let restored = repo.get("standard_meeting").unwrap();
        assert_eq!(restored.template.markdown, original.template.markdown);
        assert!(!restored.is_override);
    }

    #[test]
    fn template_write_failure_preserves_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not_a_directory");
        fs::write(&file, b"keep me").unwrap();
        let repo = TemplateRepository::new(file.clone(), None);
        assert!(repo.save("mine", &template(), true).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"keep me");
    }

    #[cfg(unix)]
    #[test]
    fn template_symlinks_are_never_read_written_or_deleted() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(
            outside.path().join("victim.json"),
            serde_json::to_vec(&template()).unwrap(),
        )
        .unwrap();
        symlink(
            outside.path().join("victim.json"),
            dir.path().join("link.json"),
        )
        .unwrap();
        let repo = TemplateRepository::new(dir.path().into(), None);
        assert!(repo.get("link").is_err()); assert!(repo.save("link", &template(), false).is_err()); assert!(repo.delete("link").is_err());
        assert!(outside.path().join("victim.json").exists());
        symlink(outside.path(), dir.path().join("linked_dir")).unwrap();
        let repo = TemplateRepository::new(dir.path().join("linked_dir"), None);
        assert!(repo.save("other", &template(), true).is_err());
    }
}
