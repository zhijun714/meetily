-- Migration: Add Custom OpenAI-compatible Transcription Configuration

-- This column stores: {endpoint, apiKey, model}
ALTER TABLE transcript_settings ADD COLUMN customTranscriptionConfig TEXT;
