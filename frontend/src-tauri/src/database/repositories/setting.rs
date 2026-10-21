use crate::audio::transcription::provider::CustomTranscriptionConfig;
use crate::database::models::{Setting, TranscriptSetting};
use crate::summary::CustomOpenAIConfig;
use sqlx::SqlitePool;

#[derive(serde::Deserialize, Debug)]
pub struct SaveModelConfigRequest {
    pub provider: String,
    pub model: String,
    #[serde(rename = "whisperModel")]
    pub whisper_model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(rename = "ollamaEndpoint")]
    pub ollama_endpoint: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
pub struct SaveTranscriptConfigRequest {
    pub provider: String,
    pub model: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
}

pub struct SettingsRepository;

// Transcript providers: localWhisper, deepgram, elevenLabs, groq, openai
// Summary providers: openai, claude, ollama, groq, added openrouter
// NOTE: Handle data exclusion in the higher layer as this is database abstraction layer(using SELECT *)

impl SettingsRepository {
    pub async fn get_model_config(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<Setting>, sqlx::Error> {
        let setting = sqlx::query_as::<_, Setting>("SELECT * FROM settings LIMIT 1")
            .fetch_optional(pool)
            .await?;
        Ok(setting)
    }

    pub async fn save_model_config(
        pool: &SqlitePool,
        provider: &str,
        model: &str,
        whisper_model: &str,
        ollama_endpoint: Option<&str>,
    ) -> std::result::Result<(), sqlx::Error> {
        // Using id '1' for backward compatibility
        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, whisperModel, ollamaEndpoint)
            VALUES ('1', $1, $2, $3, $4)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                whisperModel = excluded.whisperModel,
                ollamaEndpoint = excluded.ollamaEndpoint
            "#,
        )
        .bind(provider)
        .bind(model)
        .bind(whisper_model)
        .bind(ollama_endpoint)
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn save_api_key(
        pool: &SqlitePool,
        provider: &str,
        api_key: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        // Custom OpenAI uses JSON config (customOpenAIConfig) instead of a separate API key column
        if provider == "custom-openai" {
            return Err(sqlx::Error::Protocol(
                "custom-openai provider should use save_custom_openai_config() instead of save_api_key()".into(),
            ));
        }

        let api_key_column = match provider {
            "openai" => "openaiApiKey",
            "claude" => "anthropicApiKey",
            "ollama" => "ollamaApiKey",
            "groq" => "groqApiKey",
            "openrouter" => "openRouterApiKey",
            "builtin-ai" => return Ok(()), // No API key needed
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            r#"
            INSERT INTO settings (id, provider, model, whisperModel, "{}")
            VALUES ('1', 'openai', 'gpt-4o-2024-11-20', 'large-v3', $1)
            ON CONFLICT(id) DO UPDATE SET
                "{}" = $1
            "#,
            api_key_column, api_key_column
        );
        sqlx::query(&query).bind(api_key).execute(pool).await?;

        Ok(())
    }

    pub async fn get_api_key(
        pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        // Custom OpenAI uses JSON config - extract API key from there
        if provider == "custom-openai" {
            let config = Self::get_custom_openai_config(pool).await?;
            return Ok(config.and_then(|c| c.api_key));
        }

        let api_key_column = match provider {
            "openai" => "openaiApiKey",
            "ollama" => "ollamaApiKey",
            "groq" => "groqApiKey",
            "claude" => "anthropicApiKey",
            "openrouter" => "openRouterApiKey",
            "builtin-ai" => return Ok(None), // No API key needed
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            "SELECT {} FROM settings WHERE id = '1' LIMIT 1",
            api_key_column
        );
        let api_key = sqlx::query_scalar(&query).fetch_optional(pool).await?;
        Ok(api_key)
    }

    pub async fn get_transcript_config(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<TranscriptSetting>, sqlx::Error> {
        let setting =
            sqlx::query_as::<_, TranscriptSetting>("SELECT * FROM transcript_settings LIMIT 1")
                .fetch_optional(pool)
                .await?;
        Ok(setting)

    }

    pub async fn save_transcript_config(
        pool: &SqlitePool,
        provider: &str,
        model: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO transcript_settings (id, provider, model)
            VALUES ('1', $1, $2)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model
            "#,
        )
        .bind(provider)
        .bind(model)
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn save_transcript_api_key(
        pool: &SqlitePool,
        provider: &str,
        api_key: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        let api_key_column = match provider {
            "localWhisper" => "whisperApiKey",
            "parakeet" | "custom-openai" => return Ok(()),
            "deepgram" => "deepgramApiKey",
            "elevenLabs" => "elevenLabsApiKey",
            "groq" => "groqApiKey",
            "openai" => "openaiApiKey",
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            r#"
            INSERT INTO transcript_settings (id, provider, model, "{}")
            VALUES ('1', 'parakeet', '{}', $1)
            ON CONFLICT(id) DO UPDATE SET
                "{}" = $1
            "#,
            api_key_column, crate::config::DEFAULT_PARAKEET_MODEL, api_key_column
        );
        sqlx::query(&query).bind(api_key).execute(pool).await?;

        Ok(())
    }

    pub async fn get_transcript_api_key(
        pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<Option<String>, sqlx::Error> {
        if provider == "custom-openai" {
            return Ok(Self::get_custom_transcription_config(pool).await?.and_then(|c| c.api_key));
        }
        let api_key_column = match provider {
            "localWhisper" => "whisperApiKey",
            "parakeet" => return Ok(None), // Parakeet doesn't need an API key
            "deepgram" => "deepgramApiKey",
            "elevenLabs" => "elevenLabsApiKey",
            "groq" => "groqApiKey",
            "openai" => "openaiApiKey",
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            "SELECT {} FROM transcript_settings WHERE id = '1' LIMIT 1",
            api_key_column
        );
        let api_key = sqlx::query_scalar(&query).fetch_optional(pool).await?;
        Ok(api_key)
    }

    pub async fn delete_api_key(
        pool: &SqlitePool,
        provider: &str,
    ) -> std::result::Result<(), sqlx::Error> {
        // Custom OpenAI uses JSON config - clear the entire config
        if provider == "custom-openai" {
            sqlx::query("UPDATE settings SET customOpenAIConfig = NULL WHERE id = '1'")
                .execute(pool)
                .await?;
            return Ok(());
        }

        let api_key_column = match provider {
            "openai" => "openaiApiKey",
            "ollama" => "ollamaApiKey",
            "groq" => "groqApiKey",
            "claude" => "anthropicApiKey",
            "openrouter" => "openRouterApiKey",
            "builtin-ai" => return Ok(()), // No API key needed
            _ => {
                return Err(sqlx::Error::Protocol(
                    format!("Invalid provider: {}", provider).into(),
                ))
            }
        };

        let query = format!(
            "UPDATE settings SET {} = NULL WHERE id = '1'",
            api_key_column
        );
        sqlx::query(&query).execute(pool).await?;

        Ok(())
    }

    pub async fn get_custom_transcription_config(
        pool: &SqlitePool,
    ) -> Result<Option<CustomTranscriptionConfig>, sqlx::Error> {
        let json: Option<String> = sqlx::query_scalar::<_, Option<String>>(
            "SELECT customTranscriptionConfig FROM transcript_settings WHERE id = '1'",
        )
        .fetch_optional(pool)
        .await?
        .flatten();
        json.map(|json| {
            serde_json::from_str::<CustomTranscriptionConfig>(&json)
                .map_err(|_| sqlx::Error::Protocol("Invalid custom transcription configuration".into()))?
                .validated()
                .map_err(sqlx::Error::Protocol)
        })
        .transpose()
    }

    pub async fn save_custom_transcription_config(
        pool: &SqlitePool,
        config: &CustomTranscriptionConfig,
    ) -> Result<(), sqlx::Error> {
        let config = config.clone().validated().map_err(sqlx::Error::Protocol)?;
        let json = serde_json::to_string(&config)
            .map_err(|_| sqlx::Error::Protocol("Could not serialize transcription configuration".into()))?;
        sqlx::query(
            r#"INSERT INTO transcript_settings (id, provider, model, customTranscriptionConfig)
               VALUES ('1', 'custom-openai', $1, $2)
               ON CONFLICT(id) DO UPDATE SET
                   provider = excluded.provider,
                   model = excluded.model,
                   customTranscriptionConfig = excluded.customTranscriptionConfig"#,
        )
        .bind(&config.model)
        .bind(json)
        .execute(pool)
        .await?;
        Ok(())
    }

    // ===== CUSTOM OPENAI CONFIG METHODS =====

    /// Gets the custom OpenAI configuration from JSON
    ///
    /// # Returns
    /// * `Ok(Some(CustomOpenAIConfig))` - Config exists and is valid JSON
    /// * `Ok(None)` - No config stored
    /// * `Err(sqlx::Error)` - Database error
    pub async fn get_custom_openai_config(
        pool: &SqlitePool,
    ) -> std::result::Result<Option<CustomOpenAIConfig>, sqlx::Error> {
        use sqlx::Row;

        let row = sqlx::query(
            r#"
            SELECT customOpenAIConfig
            FROM settings
            WHERE id = '1'
            LIMIT 1
            "#
        )
        .fetch_optional(pool)
        .await?;

        match row {
            Some(record) => {
                let config_json: Option<String> = record.get("customOpenAIConfig");

                if let Some(json) = config_json {
                    // Parse JSON into CustomOpenAIConfig
                    let config: CustomOpenAIConfig = serde_json::from_str(&json)
                        .map_err(|e| sqlx::Error::Protocol(
                            format!("Invalid JSON in customOpenAIConfig: {}", e).into()
                        ))?;

                    Ok(Some(config))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Saves the custom OpenAI configuration as JSON
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `config` - CustomOpenAIConfig to save (includes endpoint, apiKey, model, maxTokens, temperature, topP)
    ///
    /// # Returns
    /// * `Ok(())` - Config saved successfully
    /// * `Err(sqlx::Error)` - Database or JSON serialization error
    pub async fn save_custom_openai_config(
        pool: &SqlitePool,
        config: &CustomOpenAIConfig,
    ) -> std::result::Result<(), sqlx::Error> {
        // Serialize config to JSON
        let config_json = serde_json::to_string(config)
            .map_err(|e| sqlx::Error::Protocol(
                format!("Failed to serialize config to JSON: {}", e).into()
            ))?;

        // Upsert into settings table
        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, whisperModel, customOpenAIConfig)
            VALUES ('1', 'custom-openai', $1, 'large-v3', $2)
            ON CONFLICT(id) DO UPDATE SET
                customOpenAIConfig = excluded.customOpenAIConfig
            "#,
        )
        .bind(&config.model)
        .bind(config_json)
        .execute(pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod custom_provider_settings_tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await.unwrap();
        sqlx::raw_sql(include_str!("../../../migrations/20250916100000_initial_schema.sql"))
            .execute(&pool).await.unwrap();
        sqlx::raw_sql(include_str!("../../../migrations/20260101000000_add_custom_transcription_config.sql"))
            .execute(&pool).await.unwrap();
        pool
    }

    fn config(model: &str) -> CustomTranscriptionConfig {
        CustomTranscriptionConfig {
            endpoint: " http://localhost:8000/v1/ ".into(),
            api_key: Some(" key ".into()),
            model: model.into(),
        }
    }

    #[tokio::test]
    async fn save_inserts_and_reactivates_custom_provider_atomically() {
        let pool = pool().await;
        assert!(SettingsRepository::get_custom_transcription_config(&pool).await.unwrap().is_none());
        SettingsRepository::save_custom_transcription_config(&pool, &config(" first ")).await.unwrap();
        let row = SettingsRepository::get_transcript_config(&pool).await.unwrap().unwrap();
        assert_eq!(row.provider, "custom-openai");
        assert_eq!(row.model, "first");
        assert_eq!(row.get_custom_transcription_config().unwrap().model, "first");
        assert_eq!(SettingsRepository::get_transcript_api_key(&pool, "custom-openai").await.unwrap().as_deref(), Some("key"));
        SettingsRepository::save_transcript_config(&pool, "localWhisper", "small").await.unwrap();
        SettingsRepository::save_transcript_api_key(&pool, "localWhisper", "legacy").await.unwrap();
        SettingsRepository::save_custom_transcription_config(&pool, &config("second")).await.unwrap();
        SettingsRepository::save_transcript_api_key(&pool, "custom-openai", "ignored").await.unwrap();
        let row = SettingsRepository::get_transcript_config(&pool).await.unwrap().unwrap();
        assert_eq!(row.provider, "custom-openai");
        assert_eq!(row.model, "second");
        assert_eq!(row.whisper_api_key.as_deref(), Some("legacy"));
        let saved = SettingsRepository::get_custom_transcription_config(&pool).await.unwrap().unwrap();
        assert_eq!(saved.model, row.model);
        assert_eq!(saved.endpoint, "http://localhost:8000/v1");
        assert_eq!(saved.api_key.as_deref(), Some("key"));
    }

    #[tokio::test]
    async fn invalid_save_preserves_active_provider_and_config() {
        let pool = pool().await;
        SettingsRepository::save_custom_transcription_config(&pool, &config("first")).await.unwrap();
        assert!(SettingsRepository::save_custom_transcription_config(&pool, &config(" ")).await.is_err());
        let row = SettingsRepository::get_transcript_config(&pool).await.unwrap().unwrap();
        assert_eq!(row.provider, "custom-openai");
        assert_eq!(row.model, "first");
        assert_eq!(row.get_custom_transcription_config().unwrap().model, "first");
    }

    #[tokio::test]
    async fn malformed_stored_config_does_not_expose_secrets() {
        let pool = pool().await;
        SettingsRepository::save_custom_transcription_config(&pool, &config("first")).await.unwrap();
        sqlx::query("UPDATE transcript_settings SET customTranscriptionConfig = ?")
            .bind(r#"{"apiKey":"secret","endpoint":secret}"#).execute(&pool).await.unwrap();
        let error = SettingsRepository::get_custom_transcription_config(&pool).await.err().unwrap().to_string();
        assert!(!error.contains("secret"));
    }
}
