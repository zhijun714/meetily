// audio/transcription/provider.rs
//
// Defines the unified TranscriptionProvider trait and common types for all
// transcription engines (Whisper, Parakeet, future providers).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct CustomTranscriptionConfig {
    pub endpoint: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    pub model: String,
}

impl CustomTranscriptionConfig {
    pub fn validated(mut self) -> Result<Self, String> {
        let endpoint = self.endpoint.trim();
        let url = reqwest::Url::parse(endpoint)
            .map_err(|_| "Enter a valid HTTP or HTTPS endpoint URL".to_string())?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err("Endpoint must be an HTTP or HTTPS URL with a host".to_string());
        }
        let authority = endpoint.split_once("://").map(|(_, rest)| {
            rest.split(['/', '?', '#']).next().unwrap_or_default()
        });
        if !url.username().is_empty()
            || url.password().is_some()
            || authority.is_some_and(|value| value.contains('@'))
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Endpoint must not contain credentials, a query, or a fragment".to_string());
        }
        self.endpoint = url.as_str().trim_end_matches('/').to_string();
        self.model = self.model.trim().to_string();
        if self.model.is_empty() {
            return Err("Transcription model is required".to_string());
        }
        self.api_key = self.api_key
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty());
        if let Some(key) = &self.api_key {
            reqwest::header::HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| "Invalid transcription API key format".to_string())?;
        }
        Ok(self)
    }
}

// ============================================================================
// TRANSCRIPTION PROVIDER TRAIT & ERROR TYPES
// ============================================================================

/// Granular error types for transcription operations
#[derive(Debug, Clone)]
pub enum TranscriptionError {
    ModelNotLoaded,
    AudioTooShort { samples: usize, minimum: usize },
    EngineFailed(String),
    UnsupportedLanguage(String),
}

impl std::fmt::Display for TranscriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelNotLoaded => write!(f, "No transcription model is loaded"),
            Self::AudioTooShort { samples, minimum } => write!(
                f,
                "Audio too short: {} samples (minimum {})",
                samples, minimum
            ),
            Self::EngineFailed(msg) => write!(f, "Transcription engine failed: {}", msg),
            Self::UnsupportedLanguage(lang) => {
                write!(f, "Language '{}' is not supported by this provider", lang)
            }
        }
    }
}

impl std::error::Error for TranscriptionError {}

/// Unified transcription result across all providers
#[derive(Debug, Clone)]
pub struct TranscriptResult {
    pub text: String,
    pub confidence: Option<f32>, // None if provider doesn't support confidence scores
    pub is_partial: bool,
}

/// Trait for transcription providers (Whisper, Parakeet, future providers)
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    /// Transcribe audio samples to text
    ///
    /// # Arguments
    /// * `audio` - Audio samples (16kHz mono, f32 format)
    /// * `language` - Optional language hint (e.g., "en", "es", "fr")
    ///
    /// # Returns
    /// * `TranscriptResult` with text, optional confidence, and partial flag
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError>;

    /// Check if a model is currently loaded
    async fn is_model_loaded(&self) -> bool;

    /// Get the name of the currently loaded model
    async fn get_current_model(&self) -> Option<String>;

    /// Get the provider name (for logging/debugging)
    fn provider_name(&self) -> &'static str;
}
