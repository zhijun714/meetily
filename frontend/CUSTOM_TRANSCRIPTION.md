# Custom transcription servers

In **Settings → Transcription**, select **Custom (OpenAI-compatible transcription)**. Enter the API base URL, transcription model identifier, and an optional API key, then save. Summary model settings are separate.

For example, a base URL of `http://localhost:8000/v1` sends audio to `http://localhost:8000/v1/audio/transcriptions`. Use the base URL, not the full transcription URL. The server must implement the OpenAI audio transcription API: a multipart request with a WAV `file`, a `model`, and an optional `language`, returning JSON with a string `text` field. A server that only implements `/chat/completions` is insufficient.

The test button sends generated silence to the configured endpoint; it does not record microphone audio. A successful test verifies the upload and response format, not speech recognition quality. The server may bill this request.

During recording, speech segments are converted to 16 kHz mono PCM WAV and uploaded to the saved server. Audio leaves the computer when the endpoint is remote. Recognition appears as segments complete, with latency depending on the service and network. API keys are stored in the application's local settings database, like existing provider credentials. Leaving the key empty and saving removes the custom transcription key.

Select and activate a local Whisper or Parakeet model to return to local transcription. Import and retranscription dialogs continue to offer local models.

## CI verification

The macOS build workflow performs TypeScript checks, transcription readiness tests, custom provider tests, and the desktop build on GitHub's runner. For personal builds choose `sign-build: false`; updater signature generation is disabled for that path. Download the resulting build from the workflow's artifacts. This does not update an already installed application automatically.
