use super::*;
use serde_json::json;

async fn fixture() -> (SqlitePool, tempfile::TempDir, DocumentPreferences) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::raw_sql("CREATE TABLE meetings(id TEXT PRIMARY KEY,title TEXT,created_at TEXT); CREATE TABLE transcripts(id TEXT PRIMARY KEY,meeting_id TEXT,transcript TEXT,timestamp TEXT,audio_start_time REAL); CREATE TABLE summary_processes(meeting_id TEXT PRIMARY KEY,status TEXT,result TEXT);")
        .execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../../../migrations/20261021000000_add_document_exports.sql")).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO meetings VALUES ('m','Meeting','2026-10-21T00:00:00Z')").execute(&pool).await.unwrap();
    let folder = tempfile::tempdir().unwrap();
    let prefs = DocumentPreferences { transcript_enabled: true, transcript_folder: Some(folder.path().to_str().unwrap().into()), summary_enabled: true, summary_folder: Some(folder.path().to_str().unwrap().into()) };
    (pool, folder, prefs)
}
async fn text(pool: &SqlitePool, id: &str, body: &str, time: f64) {
    sqlx::query("INSERT INTO transcripts VALUES (?,'m',?,'2026-10-21T00:00:00Z',?)").bind(id).bind(body).bind(time).execute(pool).await.unwrap();
}

#[tokio::test]
async fn complete_transcript_sorted_and_path_stable_after_rename_and_directory_change() {
    let (pool, _folder, mut prefs) = fixture().await;
    for i in (0..350).rev() { text(&pool, &i.to_string(), &format!("segment {}", i), i as f64).await; }
    let path = export(&pool, &prefs, "m", DocumentKind::Transcript, true).await.unwrap().unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert_eq!(body.lines().filter(|line| line.starts_with('[')).count(), 350);
    assert!(body.find("[00:00:01] segment 1\n").unwrap() < body.find("[00:05:49] segment 349\n").unwrap());
    sqlx::query("UPDATE meetings SET title='Renamed' WHERE id='m'").execute(&pool).await.unwrap();
    let other = tempfile::tempdir().unwrap();
    prefs.transcript_folder = Some(other.path().to_str().unwrap().into());
    assert_eq!(export(&pool, &prefs, "m", DocumentKind::Transcript, false).await.unwrap(), Some(path.clone()));
    assert!(std::fs::read_to_string(path).unwrap().starts_with("Renamed\n"));
    assert_eq!(std::fs::read_dir(other.path()).unwrap().count(), 0);
}
#[tokio::test]
async fn disabled_is_noop_but_manual_export_works_and_collisions_are_preserved() {
    let (pool, folder, mut prefs) = fixture().await;
    prefs.transcript_enabled = false;
    assert!(export(&pool, &prefs, "m", DocumentKind::Transcript, true).await.unwrap().is_none());
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_exports").fetch_one(&pool).await.unwrap(), 0);
    let existing = folder.path().join(format::filename("Meeting", "2026-10-21", "m", "txt"));
    std::fs::write(&existing, "external").unwrap();
    let path = export(&pool, &prefs, "m", DocumentKind::Transcript, false).await.unwrap().unwrap();
    assert_ne!(Path::new(&path), existing);
    assert_eq!(std::fs::read_to_string(existing).unwrap(), "external");
}
#[tokio::test]
async fn external_edits_and_failed_writes_preserve_database_and_previous_file() {
    let (pool, folder, prefs) = fixture().await;
    text(&pool, "1", "saved", 0.0).await;
    let path = export(&pool, &prefs, "m", DocumentKind::Transcript, false).await.unwrap().unwrap();
    std::fs::write(&path, "external edit").unwrap();
    text(&pool, "2", "newly committed", 2.0).await;
    assert!(export(&pool, &prefs, "m", DocumentKind::Transcript, true).await.unwrap_err().contains("outside Meetily"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "external edit");
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transcripts").fetch_one(&pool).await.unwrap(), 2);
    std::fs::remove_dir_all(folder.path()).unwrap();
    assert!(export(&pool, &prefs, "m", DocumentKind::Transcript, true).await.is_err());
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transcripts").fetch_one(&pool).await.unwrap(), 2);
}
#[tokio::test]
async fn latest_summary_only_empty_and_pending_do_not_overwrite() {
    let (pool, _folder, prefs) = fixture().await;
    sqlx::query("INSERT INTO summary_processes VALUES ('m','completed',?)").bind(json!({"markdown":"first"}).to_string()).execute(&pool).await.unwrap();
    let path = export(&pool, &prefs, "m", DocumentKind::Summary, true).await.unwrap().unwrap();
    sqlx::query("UPDATE summary_processes SET result=?").bind(json!({"summary_json":[{"content":"new edit"}]}).to_string()).execute(&pool).await.unwrap();
    assert!(export(&pool, &prefs, "m", DocumentKind::Summary, false).await.unwrap_err().contains("Invalid summary block"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\n");
    sqlx::query("UPDATE summary_processes SET result=?, status='pending'").bind(json!({"markdown":"pending"}).to_string()).execute(&pool).await.unwrap();
    assert!(export(&pool, &prefs, "m", DocumentKind::Summary, false).await.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\n");
    sqlx::query("UPDATE summary_processes SET result=?, status='completed'").bind(json!({"markdown":"latest"}).to_string()).execute(&pool).await.unwrap();
    let (a, b) = tokio::join!(export(&pool, &prefs, "m", DocumentKind::Summary, true), export(&pool, &prefs, "m", DocumentKind::Summary, false));
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "latest\n");
}
#[tokio::test]
async fn interrupted_write_can_be_retried_from_pending_hash_and_preferences_validate() {
    let (pool, folder, prefs) = fixture().await;
    let path = export(&pool, &prefs, "m", DocumentKind::Transcript, false).await.unwrap().unwrap();
    let current = std::fs::read(&path).unwrap();
    let backup = folder.path().join(".meetily-document-backup-crash");
    std::fs::create_dir(&backup).unwrap();
    std::fs::write(backup.join("original-path"), &path).unwrap();
    std::fs::rename(&path, backup.join("document")).unwrap();
    sqlx::query("UPDATE document_exports SET pending_hash=last_hash,last_hash=NULL").execute(&pool).await.unwrap();
    assert!(export(&pool, &prefs, "m", DocumentKind::Transcript, false).await.is_ok());
    assert_eq!(std::fs::read(path).unwrap(), current);
    assert!(super::super::preferences::validate_directory("relative").is_err());
    assert!(super::super::preferences::validate_directory(folder.path().to_str().unwrap()).is_ok());
    assert!(!DocumentPreferences::default().transcript_enabled);
    assert!(!DocumentPreferences::default().summary_enabled);
}
