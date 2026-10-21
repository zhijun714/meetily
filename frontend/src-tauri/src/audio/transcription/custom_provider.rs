use super::provider::{CustomTranscriptionConfig, TranscriptResult, TranscriptionError, TranscriptionProvider};
use async_trait::async_trait;
use reqwest::{multipart, Client};
use serde::Deserialize;
use std::time::Duration;

pub struct CustomTranscriptionProvider {
    client: Client,
    config: CustomTranscriptionConfig,
}

#[derive(Deserialize)]
struct Response {
    text: String,
}

impl CustomTranscriptionProvider {
    pub fn new(config: CustomTranscriptionConfig) -> Result<Self, String> {
        Self::with_timeout(config, Duration::from_secs(300))
    }

    fn with_timeout(config: CustomTranscriptionConfig, timeout: Duration) -> Result<Self, String> {
        let config = config.validated()?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Could not initialize transcription HTTP client".to_string())?;
        Ok(Self { client, config })
    }
}

fn encode_wav(audio: &[f32]) -> Result<Vec<u8>, TranscriptionError> {
    let data_size = audio.len().checked_mul(2)
        .and_then(|size| u32::try_from(size).ok())
        .filter(|size| *size <= u32::MAX - 36)
        .ok_or_else(|| TranscriptionError::EngineFailed("Audio exceeds WAV size limit".into()))?;
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&16000u32.to_le_bytes());
    wav.extend_from_slice(&32000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for sample in audio {
        let sample = if sample.is_finite() { sample.clamp(-1.0, 1.0) } else { 0.0 };
        wav.extend_from_slice(&((sample * i16::MAX as f32) as i16).to_le_bytes());
    }
    Ok(wav)
}

fn request_error(error: reqwest::Error) -> TranscriptionError {
    let message = if error.is_timeout() {
        "Custom transcription request timed out"
    } else if error.is_connect() {
        "Could not connect to custom transcription endpoint"
    } else {
        "Custom transcription request failed"
    };
    TranscriptionError::EngineFailed(message.into())
}

#[async_trait]
impl TranscriptionProvider for CustomTranscriptionProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> Result<TranscriptResult, TranscriptionError> {
        let file = multipart::Part::bytes(encode_wav(&audio)?)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|_| TranscriptionError::EngineFailed("Could not prepare audio upload".into()))?;
        let mut form = multipart::Form::new()
            .part("file", file)
            .text("model", self.config.model.clone());
        if let Some(language) = language.as_deref().map(str::trim)
            .filter(|language| !language.is_empty()
                && !language.eq_ignore_ascii_case("auto")
                && !language.eq_ignore_ascii_case("auto-translate")) {
            form = form.text("language", language.to_string());
        }
        let mut request = self.client
            .post(format!("{}/audio/transcriptions", self.config.endpoint))
            .multipart(form);
        if let Some(key) = &self.config.api_key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.map_err(request_error)?;
        if !response.status().is_success() {
            return Err(TranscriptionError::EngineFailed(format!(
                "Custom transcription endpoint returned HTTP {}", response.status().as_u16()
            )));
        }
        let bytes = response.bytes().await.map_err(request_error)?;
        let response: Response = serde_json::from_slice(&bytes).map_err(|_| {
            TranscriptionError::EngineFailed(
                "Custom transcription endpoint must return JSON with a string 'text' field".into(),
            )
        })?;
        Ok(TranscriptResult { text: response.text, confidence: None, is_partial: false })
    }

    async fn is_model_loaded(&self) -> bool { true }

    async fn get_current_model(&self) -> Option<String> { Some(self.config.model.clone()) }

    fn provider_name(&self) -> &'static str { "Custom (OpenAI-compatible)" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};

    fn config(endpoint: &str, key: Option<&str>) -> CustomTranscriptionConfig {
        CustomTranscriptionConfig {
            endpoint: endpoint.into(), api_key: key.map(str::to_string), model: " whisper-1 ".into(),
        }
    }

    async fn server(status: &str, body: &str, extra_headers: &str) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}", body.len());
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut buf = [0; 4096];
                let read = stream.read(&mut buf).await.unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buf[..read]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length: usize = headers.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().unwrap())
                    }).expect("Multipart upload needs Content-Length");
                    if request.len() >= end + 4 + length { break; }
                }
            }
            stream.write_all(response.as_bytes()).await.unwrap();
            request
        });
        (endpoint, handle)
    }

    #[test]
    fn validates_config_without_echoing_secrets() {
        for endpoint in ["ftp://localhost", "not a URL", "http://user:secret@localhost/v1", "http://@localhost", "http://localhost?key=secret", "http://localhost/#secret"] {
            let error = config(endpoint, None).validated().err().unwrap();
            assert!(!error.contains("secret"));
        }
        let mut invalid = config("http://localhost", None);
        invalid.model = " ".into();
        assert!(invalid.validated().is_err());
        assert!(config("http://localhost", Some("secret\r\nx: y")).validated().is_err());
        let valid = config(" http://localhost:8000/v1/ ", Some("  key  ")).validated().unwrap();
        assert_eq!(valid.endpoint, "http://localhost:8000/v1");
        assert_eq!(valid.model, "whisper-1");
        assert_eq!(valid.api_key.as_deref(), Some("key"));
    }

    #[tokio::test]
    async fn posts_authenticated_multipart_pcm_wav_and_language() {
        let (endpoint, request) = server("200 OK", r#"{"text":"hello"}"#, "").await;
        let provider = CustomTranscriptionProvider::new(config(&endpoint, Some(" key "))).unwrap();
        let result = provider.transcribe(vec![-2.0, 0.0, 2.0, f32::NAN], Some(" en ".into())).await.unwrap();
        assert_eq!(result.text, "hello");
        assert_eq!(result.confidence, None);
        assert!(!result.is_partial);
        assert!(provider.is_model_loaded().await);
        assert_eq!(provider.get_current_model().await.as_deref(), Some("whisper-1"));
        let request = request.await.unwrap();
        let text = String::from_utf8_lossy(&request);
        assert!(text.starts_with("POST /v1/audio/transcriptions HTTP/1.1\r\n"));
        assert!(text.contains("authorization: Bearer key\r\n"));
        assert!(text.contains("multipart/form-data; boundary="));
        assert!(text.contains("name=\"model\"\r\n\r\nwhisper-1\r\n"));
        assert!(text.contains("name=\"language\"\r\n\r\nen\r\n"));
        assert!(text.contains("name=\"file\"; filename=\"audio.wav\""));
        assert!(text.contains("Content-Type: audio/wav"));
        let start = request.windows(4).position(|w| w == b"RIFF").unwrap();
        let wav = &request[start..start + 52];
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 44);
        assert_eq!(u16::from_le_bytes(wav[20..22].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(wav[22..24].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16000);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 32000);
        assert_eq!(u16::from_le_bytes(wav[32..34].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 8);
        assert_eq!(&wav[44..], &[1, 128, 0, 0, 255, 127, 0, 0]);
    }

    #[tokio::test]
    async fn omits_optional_auth_and_automatic_language() {
        for language in [None, Some(""), Some(" "), Some(" auto "), Some("auto-translate")] {
            let (endpoint, request) = server("200 OK", r#"{"text":""}"#, "").await;
            let provider = CustomTranscriptionProvider::new(config(&endpoint, Some(" "))).unwrap();
            assert!(provider.transcribe(vec![0.0; 1600], language.map(str::to_string)).await.unwrap().text.is_empty());
            let request = request.await.unwrap();
            let text = String::from_utf8_lossy(&request);
            assert!(!text.contains("authorization:"));
            assert!(!text.contains("name=\"language\""));
        }
    }

    #[tokio::test]
    async fn rejects_http_and_invalid_json_responses_without_echoing_body() {
        for (status, body) in [("401 Unauthorized", "secret"), ("500 Internal Server Error", "secret"), ("200 OK", "secret"), ("200 OK", r#"{"text":null}"#), ("200 OK", r#"{"text":123}"#), ("200 OK", r#"{"error":"secret"}"#)] {
            let (endpoint, request) = server(status, body, "").await;
            let provider = CustomTranscriptionProvider::new(config(&endpoint, Some("secret"))).unwrap();
            let error = provider.transcribe(vec![0.0], None).await.unwrap_err().to_string();
            assert!(!error.contains("secret"));
            if !status.starts_with("200") { assert!(error.contains(&status[..3])); }
            request.await.unwrap();
        }
    }

    #[tokio::test]
    async fn does_not_follow_redirects() {
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let location = format!("Location: http://{}/stolen\r\n", target.local_addr().unwrap());
        let (endpoint, request) = server("307 Temporary Redirect", "", &location).await;
        let provider = CustomTranscriptionProvider::new(config(&endpoint, Some("secret"))).unwrap();
        assert!(provider.transcribe(vec![0.0], None).await.unwrap_err().to_string().contains("307"));
        request.await.unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(50), target.accept()).await.is_err());
    }

    #[tokio::test]
    async fn times_out_slow_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let provider = CustomTranscriptionProvider::with_timeout(config(&endpoint, Some("secret")), Duration::from_millis(50)).unwrap();
        let error = provider.transcribe(vec![0.0], None).await.unwrap_err().to_string();
        assert!(error.contains("timed out"));
        assert!(!error.contains("secret"));
    }
}
