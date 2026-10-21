use super::{DocumentKind, DocumentPreferences};
use std::{io::Write, path::{Path, PathBuf}};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_store::StoreExt;

pub fn validate_directory(folder: &str) -> Result<PathBuf, String> {
    let path = Path::new(folder);
    if !path.is_absolute() { return Err("Choose an absolute document directory.".into()); }
    if !path.is_dir() { return Err("The document directory does not exist.".into()); }
    let path = path.canonicalize().map_err(|e| format!("Cannot access directory: {}", e))?;
    let mut probe = tempfile::NamedTempFile::new_in(&path).map_err(|e| format!("Directory is not writable: {}", e))?;
    probe.write_all(b"meetily directory check").and_then(|_| probe.as_file().sync_all())
        .map_err(|e| format!("Directory is not writable: {}", e))?;
    Ok(path)
}

pub fn load<R: Runtime>(app: &AppHandle<R>) -> Result<DocumentPreferences, String> {
    let store = app.store("document_preferences.json").map_err(|e| e.to_string())?;
    store.get("preferences").map(serde_json::from_value).transpose().map_err(|e| e.to_string()).map(|v| v.unwrap_or_default())
}

#[tauri::command]
pub async fn api_get_document_preferences<R: Runtime>(app: AppHandle<R>) -> Result<DocumentPreferences, String> { load(&app) }

#[tauri::command]
pub async fn api_save_document_preferences<R: Runtime>(app: AppHandle<R>, mut preferences: DocumentPreferences) -> Result<DocumentPreferences, String> {
    for kind in [DocumentKind::Transcript, DocumentKind::Summary] {
        let (enabled, folder) = preferences.for_kind(kind);
        if enabled {
            let normalized = validate_directory(folder.as_deref().ok_or("Select a directory before enabling automatic export.")?)?
                .to_str().ok_or("Directory path is not valid UTF-8")?.to_owned();
            match kind { DocumentKind::Transcript => preferences.transcript_folder = Some(normalized), DocumentKind::Summary => preferences.summary_folder = Some(normalized) }
        }
    }
    let store = app.store("document_preferences.json").map_err(|e| e.to_string())?;
    let previous = store.get("preferences");
    store.set("preferences", serde_json::to_value(&preferences).map_err(|e| e.to_string())?);
    if let Err(error) = store.save() {
        if let Some(previous) = previous { store.set("preferences", previous); } else { store.delete("preferences"); }
        return Err(format!("Cannot save document settings: {}", error));
    }
    Ok(preferences)
}

#[tauri::command]
pub async fn api_select_document_directory<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
    // Callback dialog avoids blocking the UI thread on macOS.
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| { let _ = sender.send(folder); });
    let folder = receiver.await.map_err(|e| e.to_string())?;
    folder.map(|folder| {
        let path = folder.into_path().map_err(|e| e.to_string())?;
        path.into_os_string().into_string().map_err(|_| "Directory path is not valid UTF-8".into())
    }).transpose()
}

#[tauri::command]
pub async fn api_open_document_directory(folder: String) -> Result<(), String> {
    let path = Path::new(&folder);
    if !path.is_absolute() || !path.is_dir() { return Err("Document directory does not exist.".into()); }
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let program = "xdg-open";
    std::process::Command::new(program).arg(path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}
