use super::{management::TemplateRepository, types::Template};
use once_cell::sync::Lazy;
use std::{path::PathBuf, sync::RwLock};
use tracing::{info, warn};

static BUNDLED_TEMPLATES_DIR: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

pub fn set_bundled_templates_dir(path: PathBuf) {
    info!("Bundled templates directory set to: {:?}", path);
    if let Ok(mut dir) = BUNDLED_TEMPLATES_DIR.write() { *dir = Some(path); }
}

/// Keep the existing cross-platform custom directory so previously installed templates survive.
pub fn repository() -> Result<TemplateRepository, String> {
    let custom = dirs::data_dir().ok_or("Cannot find application data directory")?.join("Meetily").join("templates");
    let bundled = BUNDLED_TEMPLATES_DIR.read().map_err(|_| "Template directory lock unavailable")?.clone();
    Ok(TemplateRepository::new(custom, bundled))
}

pub fn get_template(template_id: &str) -> Result<Template, String> {
    repository()?.get(template_id).map(|editor| editor.template)
}

pub fn validate_and_parse_template(json_content: &str) -> Result<Template, String> {
    let template: Template = serde_json::from_str(json_content)
        .map_err(|e| format!("Failed to parse template JSON: {}", e))?;
    template.validate()?;
    Ok(template)
}

pub fn list_template_ids() -> Vec<String> {
    repository().map(|repo| repo.ids()).unwrap_or_else(|e| {
        warn!("Cannot list templates: {}", e);
        super::defaults::list_builtin_template_ids().into_iter().map(str::to_owned).collect()
    })
}

pub fn list_templates() -> Vec<(String, String, String)> {
    list_template_ids().into_iter().filter_map(|id| match get_template(&id) {
        Ok(template) => Some((id, template.name, template.description)),
        Err(e) => { warn!("Failed to load template '{}': {}", id, e); None }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_builtin_template_loading_and_listing() {
        let template = get_template("daily_standup").unwrap();
        assert_eq!(template.name, "Daily Standup");
        assert!(!template.sections.is_empty());
        assert!(list_template_ids().contains(&"standard_meeting".to_string()));
        assert!(get_template("../standard_meeting").is_err());
        assert!(get_template("nonexistent_template").is_err());
        assert!(validate_and_parse_template("invalid json").is_err());
    }
}
