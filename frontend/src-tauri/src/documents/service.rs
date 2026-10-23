use super::{format, DocumentKind, DocumentPreferences};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::LazyLock,
};

pub(super) static EXPORT_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

#[derive(sqlx::FromRow)]
struct Ownership {
    path: Option<String>,
    last_hash: Option<String>,
    pending_hash: Option<String>,
    rename_from: Option<String>,
    rename_hash: Option<String>,
}

/// A write reservation protects the snapshot from summary/edit/retranscription commits
/// until the atomic replacement and ownership update are complete.
async fn reserve<'a>(
    pool: &'a SqlitePool,
    id: &str,
    kind: DocumentKind,
) -> Result<Transaction<'a, Sqlite>, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO document_exports(meeting_id,kind) VALUES (?,?) ON CONFLICT(meeting_id,kind) DO UPDATE SET kind=excluded.kind")
        .bind(id).bind(kind.as_str()).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    Ok(tx)
}

async fn content(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
    kind: DocumentKind,
) -> Result<(String, String, String), String> {
    let row = sqlx::query("SELECT title,created_at,folder_path FROM meetings WHERE id=?")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    let title: String = row.get("title");
    let created: String = row.get("created_at");
    let folder: Option<String> = row.get("folder_path");
    let date = crate::meeting_title::recording_date(folder.as_deref(), &created)
        .map(|date| date.format("%Y%m%d").to_string())
        .unwrap_or_else(|_| created.clone());
    let body = match kind {
        DocumentKind::Transcript => {
            let rows = sqlx::query("SELECT transcript,timestamp,audio_start_time FROM transcripts WHERE meeting_id=? ORDER BY COALESCE(audio_start_time, 1e100), timestamp, rowid")
                .bind(id).fetch_all(&mut **tx).await.map_err(|e| e.to_string())?;
            // Legacy rows use their earliest wall-clock time as the relative origin.
            let origin = rows.iter().filter_map(|r| {
                let s: String = r.get("timestamp"); chrono::DateTime::parse_from_rfc3339(&s).ok()
            }).min();
            let mut body = format!("{}\nDate: {}\n\n", title, created);
            for row in rows {
                let text: String = row.get("transcript");
                let raw: String = row.get("timestamp");
                let seconds = row
                    .get::<Option<f64>, _>("audio_start_time")
                    .unwrap_or_else(|| {
                        chrono::DateTime::parse_from_rfc3339(&raw)
                            .ok()
                            .zip(origin)
                            .map(|(time, start)| (time - start).num_milliseconds() as f64 / 1000.0)
                            .unwrap_or_else(|| {
                                raw.split(':')
                                    .try_fold(0.0, |acc, part| {
                                        part.parse::<f64>().ok().map(|v| acc * 60.0 + v)
                                    })
                                    .unwrap_or(0.0)
                            })
                    });
                body.push_str(&format!("[{}] {}\n", format::timestamp(seconds), text));
            }
            body
        }
        DocumentKind::Summary => {
            let row = sqlx::query("SELECT status,result FROM summary_processes WHERE meeting_id=?")
                .bind(id).fetch_optional(&mut **tx).await.map_err(|e| e.to_string())?
                .ok_or("This meeting has no saved summary")?;
            let status: String = row.get("status");
            if matches!(status.to_lowercase().as_str(), "pending" | "processing") {
                return Err(
                    "Summary generation is in progress. Retry after the current summary is saved."
                        .into(),
                );
            }
            let raw: Option<String> = row.get("result");
            let value =
                serde_json::from_str(raw.as_deref().ok_or("This meeting has no saved summary")?)
                    .map_err(|e| e.to_string())?;
            crate::meeting_title::with_title(&format::summary_markdown(&value)?, &title)
        }
    };
    Ok((
        title,
        if kind == DocumentKind::Transcript {
            created
        } else {
            date
        },
        body,
    ))
}

fn same_title_variant(current: &Path, desired: &Path) -> bool {
    if current.parent() != desired.parent() || current.extension() != desired.extension() {
        return false;
    }
    let (Some(stem), Some(base)) = (
        current.file_stem().and_then(|name| name.to_str()),
        desired.file_stem().and_then(|name| name.to_str()),
    ) else {
        return false;
    };
    stem.strip_prefix(base)
        .and_then(|suffix| suffix.strip_prefix('-'))
        .and_then(|suffix| suffix.parse::<u32>().ok())
        .is_some_and(|suffix| suffix >= 2)
}

fn current_hash(path: &Path) -> Result<Option<String>, String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_file() => {
            Err("Export destination is not a regular file; it was preserved.".into())
        }
        Ok(_) => std::fs::read(path)
            .map(|v| Some(hash(&v)))
            .map_err(|e| format!("Cannot read document: {}", e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Cannot inspect document: {}", e)),
    }
}
fn remove_if_unchanged(path: &Path, expected: &str) {
    if current_hash(path).ok().flatten().as_deref() == Some(expected) {
        let _ = std::fs::remove_file(path);
    }
}
fn check_ownership(path: &Path, owner: &Ownership) -> Result<Option<String>, String> {
    let current = current_hash(path)?;
    if let Some(actual) = &current {
        if owner.last_hash.as_ref() != Some(actual) && owner.pending_hash.as_ref() != Some(actual) {
            return Err("The document was changed outside Meetily or belongs to another file. It was preserved. Move it aside before retrying.".into());
        }
    }
    Ok(current)
}
fn recover_missing(path: &Path, owner: &Ownership) -> Result<(), String> {
    if current_hash(path)?.is_some() {
        return Ok(());
    }
    if let (Some(previous), Some(expected)) = (&owner.rename_from, &owner.rename_hash) {
        let previous = Path::new(previous);
        if current_hash(previous)?.as_ref() == Some(expected) {
            match std::fs::hard_link(previous, path) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
                Err(error) => return Err(format!("Cannot restore interrupted rename: {error}")),
            }
        }
    }
    let parent = path.parent().ok_or("Document directory missing")?;
    // A journal is durable before displacement. After a crash, retry can restore
    // the previous inode without overwriting any newly-created external file.
    let entries = std::fs::read_dir(parent).map_err(|e| format!("Cannot inspect document recovery journal: {}", e))?;
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(".meetily-document-backup-") { continue; }
        let directory = entry.path();
        if std::fs::symlink_metadata(&directory).map(|m| !m.file_type().is_dir()).unwrap_or(true) { continue; }
        let original = std::fs::read_to_string(directory.join("original-path")).unwrap_or_default();
        if original != path.to_string_lossy() { continue; }
        let backup = directory.join("document");
        if check_ownership(&backup, owner).ok().flatten().is_some() {
            match std::fs::hard_link(&backup, path) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
                Err(error) => {
                    return Err(format!(
                        "Cannot restore document from {}: {}",
                        backup.display(),
                        error
                    ))
                }
            }
        }
    }
    Ok(())
}
async fn available_path(
    tx: &mut Transaction<'_, Sqlite>,
    desired: PathBuf,
    kind: DocumentKind,
) -> Result<PathBuf, String> {
    let mut candidate = desired.clone();
    let base = desired
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("Invalid document title")?;
    let mut suffix = 2;
    while std::fs::symlink_metadata(&candidate).is_ok()
        || sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_exports WHERE path=?")
            .bind(candidate.to_string_lossy().as_ref())
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| e.to_string())?
            > 0
    {
        candidate = desired.with_file_name(format!("{}-{}.{}", base, suffix, kind.extension()));
        suffix += 1;
    }
    Ok(candidate)
}

fn atomic_write(path: &Path, body: &str, owner: &Ownership) -> Result<(), String> {
    let parent = path.parent().ok_or("Document directory missing")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| format!("Cannot create document: {}", e))?;
    temp.write_all(body.as_bytes()).and_then(|_| temp.as_file().sync_all()).map_err(|e| format!("Cannot write document: {}", e))?;
    // Displace before checking: an external edit racing the earlier hash check is
    // preserved in the backup. Installing the new file never replaces a name.
    if current_hash(path)?.is_some() {
        let backup_dir = tempfile::Builder::new().prefix(".meetily-document-backup-").tempdir_in(parent)
            .map_err(|e| format!("Cannot protect existing document: {}", e))?.into_path();
        let backup = backup_dir.join("document");
        let mut journal = std::fs::File::create(backup_dir.join("original-path")).map_err(|e| e.to_string())?;
        journal.write_all(path.to_string_lossy().as_bytes()).and_then(|_| journal.sync_all()).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            std::fs::File::open(&backup_dir).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
            std::fs::File::open(parent).and_then(|f| f.sync_all()).map_err(|e| e.to_string())?;
        }
        if let Err(error) = std::fs::rename(path, &backup) {
            let _ = std::fs::remove_dir(&backup_dir);
            return Err(format!("Cannot protect existing document: {}", error));
        }
        let displaced = check_ownership(&backup, owner);
        if let Err(error) = displaced {
            // A hard link restores only into a vacant destination, never overwriting
            // a replacement created by an editor. Keep the backup on any conflict.
            let _ = std::fs::hard_link(&backup, path);
            return Err(format!("{} Preserved original at {}", error, backup.display()));
        }
        if let Err(error) = temp.persist_noclobber(path) {
            let _ = std::fs::hard_link(&backup, path);
            return Err(format!("Cannot install document: {}. Preserved original at {}", error, backup.display()));
        }
        // If an editor still holds the displaced inode it can continue writing it.
        // Keep that inode rather than unlinking it and silently losing those edits.
        // The backup path is stable and recoverable; no existing content is deleted.
    } else {
        temp.persist_noclobber(path).map_err(|e| {
            format!(
                "Document destination already exists or is unwritable: {}",
                e
            )
        })?;
    }
    #[cfg(unix)]
    std::fs::File::open(parent).and_then(|f| f.sync_all()).map_err(|e| format!("Cannot sync document directory: {}", e))?;
    Ok(())
}

pub(super) async fn export(
    pool: &SqlitePool,
    prefs: &DocumentPreferences,
    id: &str,
    kind: DocumentKind,
    automatic: bool,
) -> Result<Option<String>, String> {
    let _guard = EXPORT_LOCK.lock().await;
    let result = export_inner(pool, prefs, id, kind, automatic).await;
    if let Err(error) = &result {
        persist_error(pool, id, kind, error).await;
    }
    result
}
async fn export_inner(
    pool: &SqlitePool,
    prefs: &DocumentPreferences,
    id: &str,
    kind: DocumentKind,
    automatic: bool,
) -> Result<Option<String>, String> {
    let (enabled, folder) = prefs.for_kind(kind);
    if automatic && !enabled { return Ok(None); }
    // A directory is required only for first export; tracked paths survive preference changes.
    for _ in 0..8 {
        let mut tx = reserve(pool, id, kind).await?;
        let (title, created, body) = content(&mut tx, id, kind).await?;
        let owner: Ownership = sqlx::query_as("SELECT path,last_hash,pending_hash,rename_from,rename_hash FROM document_exports WHERE meeting_id=? AND kind=?")
            .bind(id).bind(kind.as_str()).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
        let base = format::filename(&title, &created, id, kind.extension());
        let mut moved_from = owner.rename_from.as_ref().zip(owner.rename_hash.as_ref())
            .map(|(path, digest)| (PathBuf::from(path), digest.clone()));
        let mut created_link: Option<(PathBuf, String)> = None;
        let path = match &owner.path {
            Some(path) if kind == DocumentKind::Transcript => PathBuf::from(path),
            Some(path) => {
                let current = PathBuf::from(path);
                let desired = current.with_file_name(&base);
                if current != desired && !same_title_variant(&current, &desired) {
                    recover_missing(&current, &owner)?;
                    let actual = check_ownership(&current, &owner)?;
                    if actual.is_none() && (owner.last_hash.is_some() || owner.pending_hash.is_some()) {
                        return Err("The previous document is missing; it was not renamed. Restore it before retrying.".into());
                    }
                    let candidate = available_path(&mut tx, desired, kind).await?;
                    if actual.is_some() {
                        // The final component is reserved above; a second file may still
                        // appear before rename. Install via a hard link that never replaces it.
                        std::fs::hard_link(&current, &candidate)
                            .map_err(|e| format!("Cannot reserve renamed document: {e}"))?;
                        if let Err(error) = check_ownership(&candidate, &owner) {
                            // The linked inode may have changed while checking ownership.
                            // Preserve both names so the user's edits remain reachable.
                            return Err(error);
                        }
                        // Keep the old name until the replacement and database ownership
                        // are committed. A crash or failed write must not lose the only
                        // known-good copy of the exported document.
                        let old_hash = actual.unwrap();
                        moved_from = Some((current.clone(), old_hash.clone()));
                        created_link = Some((candidate.clone(), old_hash));
                    }
                    let path_string = candidate
                        .to_str()
                        .ok_or("Document path is not valid UTF-8")?;
                    if let Err(error) = sqlx::query(
                        "UPDATE document_exports SET path=?,rename_from=?,rename_hash=? WHERE meeting_id=? AND kind=?",
                    )
                    .bind(path_string)
                    .bind(moved_from.as_ref().map(|(path, _)| path.to_string_lossy().into_owned()))
                    .bind(moved_from.as_ref().map(|(_, digest)| digest.as_str()))
                    .bind(id)
                    .bind(kind.as_str())
                    .execute(&mut *tx)
                    .await
                    {
                        if let Some((link, expected)) = &created_link {
                            remove_if_unchanged(link, expected);
                        }
                        return Err(error.to_string());
                    }
                    candidate
                } else {
                    current
                }
            }
            None => {
                let folder = super::preferences::validate_directory(folder.as_deref().ok_or(
                    "Choose a document directory in Settings → Documents before exporting.",
                )?)?;
                available_path(&mut tx, folder.join(&base), kind).await?
            }
        };
        if let Err(error) = recover_missing(&path, &owner)
            .and_then(|_| check_ownership(&path, &owner).map(|_| ()))
        {
            if let Some((link, expected)) = &created_link {
                remove_if_unchanged(link, expected);
            }
            return Err(error);
        }
        let actual = current_hash(&path)?;
        let digest = hash(body.as_bytes());
        let path_string = path.to_str().ok_or("Document path is not valid UTF-8")?.to_owned();
        if actual.as_ref() == Some(&digest) {
            sqlx::query("UPDATE document_exports SET last_hash=?,pending_hash=NULL,last_error=NULL WHERE meeting_id=? AND kind=?")
                .bind(&digest).bind(id).bind(kind.as_str()).execute(&mut *tx).await.map_err(|e| e.to_string())?;
            if let Err(error) = tx.commit().await {
                if let Some((link, expected)) = &created_link {
                    remove_if_unchanged(link, expected);
                }
                return Err(error.to_string());
            }
            if let Some((old, old_hash)) = moved_from {
                remove_if_unchanged(&old, &old_hash);
            }
            sqlx::query("UPDATE document_exports SET rename_from=NULL,rename_hash=NULL WHERE meeting_id=? AND kind=?")
                .bind(id).bind(kind.as_str()).execute(pool).await.map_err(|e| e.to_string())?;
            return Ok(Some(path_string));
        }
        // Persist pending ownership before the filesystem operation. After a crash either
        // last_hash or pending_hash identifies our own file, so retry remains safe.
        let last = actual.as_ref().or(owner.last_hash.as_ref());
        if let Err(error) = sqlx::query("UPDATE document_exports SET path=?,last_hash=?,pending_hash=?,last_error=NULL WHERE meeting_id=? AND kind=?")
            .bind(&path_string).bind(last).bind(&digest).bind(id).bind(kind.as_str()).execute(&mut *tx).await
        {
            if let Some((link, expected)) = &created_link {
                remove_if_unchanged(link, expected);
            }
            return Err(error.to_string());
        }
        if let Err(error) = tx.commit().await {
            if let Some((link, expected)) = &created_link {
                remove_if_unchanged(link, expected);
            }
            return Err(error.to_string());
        }
        let mut tx = reserve(pool, id, kind).await?;
        let (latest_title, latest_date, latest) = content(&mut tx, id, kind).await?;
        if latest != body || latest_title != title || latest_date != created {
            tx.rollback().await.map_err(|e| e.to_string())?;
            continue;
        }
        let owner = Ownership {
            path: Some(path_string.clone()),
            last_hash: last.cloned(),
            pending_hash: Some(digest.clone()),
            rename_from: moved_from.as_ref().map(|(path, _)| path.to_string_lossy().into_owned()),
            rename_hash: moved_from.as_ref().map(|(_, digest)| digest.clone()),
        };
        atomic_write(&path, &body, &owner)?;
        sqlx::query("UPDATE document_exports SET last_hash=?,pending_hash=NULL,last_error=NULL WHERE meeting_id=? AND kind=?")
            .bind(&digest).bind(id).bind(kind.as_str()).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        if let Some((old, old_hash)) = moved_from {
            remove_if_unchanged(&old, &old_hash);
        }
        sqlx::query("UPDATE document_exports SET rename_from=NULL,rename_hash=NULL WHERE meeting_id=? AND kind=?")
            .bind(id).bind(kind.as_str()).execute(pool).await.map_err(|e| e.to_string())?;
        return Ok(Some(path_string));
    }
    Err("Meeting content changed repeatedly during export; retry when saving has finished.".into())
}

async fn persist_error(pool: &SqlitePool, id: &str, kind: DocumentKind, error: &str) {
    let _ = sqlx::query("INSERT INTO document_exports(meeting_id,kind,last_error) VALUES (?,?,?) ON CONFLICT(meeting_id,kind) DO UPDATE SET last_error=excluded.last_error")
        .bind(id).bind(kind.as_str()).bind(error).execute(pool).await;
}

#[cfg(test)]
mod tests;
