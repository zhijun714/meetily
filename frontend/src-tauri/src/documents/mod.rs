//! Derived meeting documents. All callers invoke this only after their DB save succeeds.
mod format;
pub mod preferences;
mod service;

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind { Transcript, Summary }
impl DocumentKind {
    pub fn as_str(self) -> &'static str { match self { Self::Transcript => "transcript", Self::Summary => "summary" } }
    fn extension(self) -> &'static str { match self { Self::Transcript => "txt", Self::Summary => "md" } }
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentPreferences {
    pub transcript_enabled: bool,
    pub transcript_folder: Option<String>,
    pub summary_enabled: bool,
    pub summary_folder: Option<String>,
}
impl DocumentPreferences {
    fn for_kind(&self, kind: DocumentKind) -> (bool, &Option<String>) {
        match kind { DocumentKind::Transcript => (self.transcript_enabled, &self.transcript_folder), DocumentKind::Summary => (self.summary_enabled, &self.summary_folder) }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub meeting_id: String,
    pub kind: DocumentKind,
    pub success: bool,
    pub skipped: bool,
    pub error: Option<String>,
    pub path: Option<String>,
}

async fn run<R: Runtime>(app: &AppHandle<R>, pool: &SqlitePool, meeting_id: &str, kind: DocumentKind, automatic: bool) -> ExportResult {
    let outcome = match preferences::load(app) {
        Ok(prefs) => service::export(pool, &prefs, meeting_id, kind, automatic).await,
        Err(error) => Err(format!("Cannot load document settings: {}", error)),
    };
    let mut result = ExportResult { meeting_id: meeting_id.into(), kind, success: false, skipped: false, error: None, path: None };
    match outcome {
        Ok(Some(path)) => { result.success = true; result.path = Some(path); }
        Ok(None) => { result.success = true; result.skipped = true; }
        Err(error) => {
            log::warn!("Document export failed for {} ({}): {}", meeting_id, kind.as_str(), error);
            result.error = Some(error);
        }
    }
    if !result.skipped { let _ = app.emit("document-export-result", &result); }
    result
}

/// Errors deliberately do not propagate into a successful meeting/summary save.
pub async fn after_commit<R: Runtime>(app: &AppHandle<R>, pool: &SqlitePool, meeting_id: &str, kind: DocumentKind) -> ExportResult {
    run(app, pool, meeting_id, kind, true).await
}

#[tauri::command]
pub async fn api_export_meeting_document<R: Runtime>(app: AppHandle<R>, state: tauri::State<'_, AppState>, meeting_id: String, kind: DocumentKind) -> Result<ExportResult, String> {
    // Explicit user exports work even with automatic updates switched off.
    Ok(run(&app, state.db_manager.pool(), &meeting_id, kind, false).await)
}

#[tauri::command]
pub async fn api_get_document_export_status(state: tauri::State<'_, AppState>, meeting_id: String) -> Result<Vec<ExportResult>, String> {
    let rows = sqlx::query("SELECT kind,path,last_error FROM document_exports WHERE meeting_id=?")
        .bind(&meeting_id).fetch_all(state.db_manager.pool()).await.map_err(|e| e.to_string())?;
    rows.into_iter().map(|row| {
        let kind: String = row.get("kind");
        let error: Option<String> = row.get("last_error");
        Ok(ExportResult { meeting_id: meeting_id.clone(), kind: match kind.as_str() { "transcript" => DocumentKind::Transcript, "summary" => DocumentKind::Summary, _ => return Err("Unknown document kind".into()) }, success: error.is_none(), skipped: false, path: row.get("path"), error })
    }).collect()
}
