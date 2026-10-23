use crate::database::models::SummaryProcess;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Row, SqlitePool};
use tracing::{error, info as log_info};

pub struct SummaryProcessesRepository;

impl SummaryProcessesRepository {
    /// Retrieves the current summary process state for a given meeting ID.
    pub async fn get_summary_data(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<SummaryProcess>, sqlx::Error> {
        sqlx::query_as::<_, SummaryProcess>("SELECT * FROM summary_processes WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
    }

    pub async fn update_meeting_summary(
        pool: &SqlitePool,
        meeting_id: &str,
        summary: &Value,
    ) -> Result<bool, sqlx::Error> {
        let mut transaction = pool.begin().await?;

        let meeting_exists: bool = sqlx::query("SELECT 1 FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(&mut *transaction)
            .await?
            .is_some();

        if !meeting_exists {
            log_info!(
                "Attempted to save summary for a non-existent meeting_id: {}",
                meeting_id
            );
            transaction.rollback().await?;
            return Ok(false);
        }

        let result_json = serde_json::to_string(summary);
        if result_json.is_err() {
            error!("Can't convert the json to string for saving to Database");
            transaction.rollback().await?;
            return Ok(false);
        }
        let now = Utc::now();
        let update = sqlx::query(
            "UPDATE summary_processes SET result = ?, updated_at = ? WHERE meeting_id = ?",
        )
        .bind(&result_json.unwrap())
        .bind(now)
        .bind(meeting_id)
        .execute(&mut *transaction)
        .await?;
        if update.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }

        sqlx::query("UPDATE meetings SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(meeting_id)
            .execute(&mut *transaction)
            .await?;

        transaction.commit().await?;

        log_info!(
            "Successfully updated summary and timestamp for meeting_id: {}",
            meeting_id
        );
        Ok(true)
    }

    pub async fn get_summary_data_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<SummaryProcess>, sqlx::Error> {
        sqlx::query_as::<_, SummaryProcess>(
            "SELECT p.* FROM summary_processes p JOIN transcript_chunks t ON p.meeting_id = t.meeting_id WHERE p.meeting_id = ?",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn create_or_reset_process(
        pool: &SqlitePool,
        meeting_id: &str,
        started_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        log_info!(
            "Creating or resetting summary process for meeting_id: {}",
            meeting_id
        );
        sqlx::query(
            r#"
            INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, start_time, result, error)
            VALUES (?, 'PENDING', ?, ?, ?, NULL, NULL)
            ON CONFLICT(meeting_id) DO UPDATE SET
                status = 'PENDING',
                updated_at = excluded.updated_at,
                start_time = excluded.start_time,
                result_backup = result,
                result_backup_timestamp = excluded.updated_at,
                result = result,
                error = NULL
            "#
        )
        .bind(meeting_id)
        .bind(started_at)
        .bind(started_at)
        .bind(started_at)
        .execute(pool)
        .await?;
        log_info!(
            "Backed up existing summary before regeneration for meeting_id: {}",
            meeting_id
        );
        Ok(())
    }

    pub async fn update_process_completed(
        pool: &SqlitePool,
        meeting_id: &str,
        started_at: DateTime<Utc>,
        result: Value,
        chunk_count: i64,
        processing_time: f64,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now();
        let result_str = serde_json::to_string(&result)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize result: {}", e)))?;

        let update = sqlx::query(
            r#"
            UPDATE summary_processes
            SET status = 'completed', result = ?, updated_at = ?, end_time = ?, chunk_count = ?, processing_time = ?, error = NULL, result_backup = NULL, result_backup_timestamp = NULL
            WHERE meeting_id = ? AND start_time = ? AND LOWER(status) = 'pending'
            "#
        )
        .bind(result_str)
        .bind(now)
        .bind(now)
        .bind(chunk_count)
        .bind(processing_time)
        .bind(meeting_id)
        .bind(started_at)
        .execute(pool)
        .await?;
        Ok(update.rows_affected() == 1)
    }

    /// Finalize the active generation and its document title in a single transaction.
    /// A user title (including a concurrent rename) wins over the model suggestion.
    #[allow(clippy::too_many_arguments)]
    pub async fn complete_with_title(
        pool: &SqlitePool,
        meeting_id: &str,
        started_at: DateTime<Utc>,
        mut result: Value,
        short_title: Option<&str>,
        chunk_count: i64,
        processing_time: f64,
    ) -> Result<Option<String>, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let row = sqlx::query(
            "SELECT title, created_at, folder_path, title_manually_edited FROM meetings WHERE id=?",
        )
        .bind(meeting_id)
        .fetch_one(&mut *tx)
        .await?;
        let old_title: String = row.get("title");
        let manually_edited: i64 = row.get("title_manually_edited");
        let final_title = if manually_edited != 0 {
            old_title
        } else if let Some(short_title) = short_title.filter(|s| !s.trim().is_empty()) {
            let folder: Option<String> = row.get("folder_path");
            let created: String = row.get("created_at");
            match crate::meeting_title::recording_date(folder.as_deref(), &created) {
                Ok(date) => crate::meeting_title::canonical_title(date, short_title),
                Err(_) => old_title,
            }
        } else {
            old_title
        };
        result["MeetingName"] = Value::String(final_title.clone());
        if let Some(markdown) = result.get("markdown").and_then(Value::as_str) {
            result["markdown"] =
                Value::String(crate::meeting_title::with_title(markdown, &final_title));
        }
        let now = Utc::now();
        let result_str = serde_json::to_string(&result)
            .map_err(|e| sqlx::Error::Protocol(format!("Failed to serialize result: {e}")))?;
        let update = sqlx::query(
            "UPDATE summary_processes SET status='completed', result=?, updated_at=?, end_time=?, chunk_count=?, processing_time=?, error=NULL, result_backup=NULL, result_backup_timestamp=NULL WHERE meeting_id=? AND start_time=? AND LOWER(status)='pending'"
        ).bind(result_str).bind(now).bind(now).bind(chunk_count).bind(processing_time)
            .bind(meeting_id).bind(started_at).execute(&mut *tx).await?;
        if update.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(None);
        }
        sqlx::query("UPDATE meetings SET title=?, updated_at=? WHERE id=?")
            .bind(&final_title)
            .bind(now)
            .bind(meeting_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE transcript_chunks SET meeting_name=? WHERE meeting_id=?")
            .bind(&final_title)
            .bind(meeting_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(final_title))
    }

    pub async fn update_process_failed(
        pool: &SqlitePool,
        meeting_id: &str,
        started_at: DateTime<Utc>,
        error: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now();

        let update = sqlx::query(
            r#"
            UPDATE summary_processes
            SET
                status = 'failed',
                error = ?,
                updated_at = ?,
                end_time = ?,
                result = COALESCE(result_backup, result),
                result_backup = NULL,
                result_backup_timestamp = NULL
            WHERE meeting_id = ? AND start_time = ? AND LOWER(status) = 'pending'
            "#,
        )
        .bind(error)
        .bind(now)
        .bind(now)
        .bind(meeting_id)
        .bind(started_at)
        .execute(pool)
        .await?;
        Ok(update.rows_affected() == 1)
    }

    pub async fn update_process_cancelled(
        pool: &SqlitePool,
        meeting_id: &str,
        started_at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now();

        let update = sqlx::query(
            r#"
            UPDATE summary_processes
            SET
                status = 'cancelled',
                updated_at = ?,
                end_time = ?,
                error = 'Generation was cancelled by user',
                result = COALESCE(result_backup, result),
                result_backup = NULL,
                result_backup_timestamp = NULL
            WHERE meeting_id = ? AND start_time = ? AND LOWER(status) = 'pending'
            "#,
        )
        .bind(now)
        .bind(now)
        .bind(meeting_id)
        .bind(started_at)
        .execute(pool)
        .await?;
        Ok(update.rows_affected() == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE summary_processes (
                meeting_id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                created_at TEXT,
                updated_at TEXT,
                start_time TEXT,
                result TEXT,
                result_backup TEXT,
                result_backup_timestamp TEXT,
                end_time TEXT,
                chunk_count INTEGER,
                processing_time REAL,
                error TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn seed_pending(
        pool: &SqlitePool,
        meeting_id: &str,
        started_at: DateTime<Utc>,
        result: Option<&str>,
        result_backup: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO summary_processes (
                meeting_id, status, created_at, updated_at, start_time, result, result_backup
            ) VALUES (?, 'PENDING', ?, ?, ?, ?, ?)",
        )
        .bind(meeting_id)
        .bind(started_at)
        .bind(started_at)
        .bind(started_at)
        .bind(result)
        .bind(result_backup)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn completion_uses_generated_title_and_respects_user_renames() {
        let pool = test_pool().await;
        sqlx::query("CREATE TABLE meetings(id TEXT PRIMARY KEY,title TEXT,created_at TEXT,folder_path TEXT,updated_at TEXT,title_manually_edited INTEGER DEFAULT 0)")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "CREATE TABLE transcript_chunks(meeting_id TEXT PRIMARY KEY,meeting_name TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (id, manually_edited) in [("generated", 0), ("custom", 1), ("stale", 0)] {
            sqlx::query("INSERT INTO meetings(id,title,created_at,title_manually_edited) VALUES (?, ?, '2026-10-22T12:00:00Z', ?)")
                .bind(id).bind(if id == "custom" { "我起的标题" } else { "Original" }).bind(manually_edited)
                .execute(&pool).await.unwrap();
            seed_pending(&pool, id, Utc::now(), None, None).await;
        }
        let date = crate::meeting_title::recording_date(None, "2026-10-22T12:00:00Z").unwrap();
        let expected = crate::meeting_title::canonical_title(date, "转写优化");
        let started: DateTime<Utc> = sqlx::query_scalar(
            "SELECT start_time FROM summary_processes WHERE meeting_id='generated'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            SummaryProcessesRepository::complete_with_title(
                &pool,
                "generated",
                started,
                json!({"markdown":"## 结论\n完成"}),
                Some("转写优化"),
                1,
                1.0
            )
            .await
            .unwrap(),
            Some(expected.clone())
        );
        let saved: String =
            sqlx::query_scalar("SELECT result FROM summary_processes WHERE meeting_id='generated'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(saved.contains(&format!("# {expected}")));
        let custom_start: DateTime<Utc> = sqlx::query_scalar(
            "SELECT start_time FROM summary_processes WHERE meeting_id='custom'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            SummaryProcessesRepository::complete_with_title(
                &pool,
                "custom",
                custom_start,
                json!({"markdown":"## 结论\n完成"}),
                Some("转写优化"),
                1,
                1.0
            )
            .await
            .unwrap(),
            Some("我起的标题".into())
        );
        let stale_start: DateTime<Utc> =
            sqlx::query_scalar("SELECT start_time FROM summary_processes WHERE meeting_id='stale'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            SummaryProcessesRepository::complete_with_title(
                &pool,
                "stale",
                stale_start - chrono::Duration::seconds(1),
                json!({"markdown":"bad"}),
                Some("坏标题"),
                1,
                1.0
            )
            .await
            .unwrap(),
            None
        );
        let title: String = sqlx::query_scalar("SELECT title FROM meetings WHERE id='stale'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(title, "Original");
    }

    #[tokio::test]
    async fn terminal_compare_and_set_preserves_first_writer() {
        let pool = test_pool().await;
        let completed_start = Utc::now();
        seed_pending(&pool, "completed-first", completed_start, None, None).await;
        assert!(
            SummaryProcessesRepository::update_process_completed(
                &pool,
                "completed-first",
                completed_start,
                json!({"markdown": "completed"}),
                1,
                1.0,
            )
            .await
            .unwrap()
        );
        assert!(
            !SummaryProcessesRepository::update_process_cancelled(
                &pool,
                "completed-first",
                completed_start,
            )
            .await
            .unwrap()
        );
        let completed_status: String = sqlx::query_scalar(
            "SELECT status FROM summary_processes WHERE meeting_id = 'completed-first'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(completed_status, "completed");

        let cancelled_start = completed_start + chrono::Duration::nanoseconds(1);
        let previous = r#"{"markdown":"previous"}"#;
        seed_pending(
            &pool,
            "cancelled-first",
            cancelled_start,
            Some(previous),
            Some(previous),
        )
        .await;
        assert!(
            SummaryProcessesRepository::update_process_cancelled(
                &pool,
                "cancelled-first",
                cancelled_start,
            )
            .await
            .unwrap()
        );
        assert!(
            !SummaryProcessesRepository::update_process_completed(
                &pool,
                "cancelled-first",
                cancelled_start,
                json!({"markdown": "completed"}),
                1,
                1.0,
            )
            .await
            .unwrap()
        );
        let cancelled: (String, String) = sqlx::query_as(
            "SELECT status, result FROM summary_processes WHERE meeting_id = 'cancelled-first'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(cancelled, ("cancelled".to_string(), previous.to_string()));
    }

    #[tokio::test]
    async fn cancelled_compare_and_set_rejects_stale_start_time() {
        let pool = test_pool().await;
        let current_start = Utc::now();
        seed_pending(&pool, "stale-cancel", current_start, None, None).await;
        assert!(!SummaryProcessesRepository::update_process_cancelled(
            &pool,
            "stale-cancel",
            current_start - chrono::Duration::nanoseconds(1),
        )
        .await
        .unwrap());
        let status: String = sqlx::query_scalar(
            "SELECT status FROM summary_processes WHERE meeting_id = 'stale-cancel'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "PENDING");
    }
}
