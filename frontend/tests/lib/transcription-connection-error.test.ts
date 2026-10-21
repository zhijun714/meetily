import { describe, expect, test } from 'bun:test';
import { transcriptionConnectionError } from '../../src/lib/transcription-connection-error';

describe('transcription connection diagnostics', () => {
  test('preserves HTTP status from Tauri string rejections and supplies a useful hint', () => {
    expect(transcriptionConnectionError('Transcription engine failed: Custom transcription endpoint returned HTTP 401')).toContain('Authentication failed');
    expect(transcriptionConnectionError(new Error('Transcription engine failed: Custom transcription endpoint returned HTTP 404'))).toContain('base URL');
    expect(transcriptionConnectionError('Transcription engine failed: Custom transcription endpoint returned HTTP 400')).toContain('silence');
    expect(transcriptionConnectionError('Transcription engine failed: Custom transcription endpoint returned HTTP 429')).toContain('quota');
  });
  test('distinguishes network timeout and response parsing', () => {
    expect(transcriptionConnectionError('Transcription engine failed: Custom transcription request timed out')).toContain('timed out');
    expect(transcriptionConnectionError('Transcription engine failed: Could not connect to custom transcription endpoint')).toContain('proxy');
    expect(transcriptionConnectionError("Transcription engine failed: Custom transcription endpoint must return JSON with a string 'text' field")).toContain('JSON text');
  });
  test('never reflects unknown error text, URLs or credentials', () => {
    for (const error of ['secret-key', 'Custom transcription endpoint returned HTTP 401 secret-key', {message: 'secret-key'}, new Error('https://user:secret-key@example.com')]) {
      expect(transcriptionConnectionError(error)).not.toContain('secret-key');
    }
  });
});
