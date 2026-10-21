import { describe, expect, test } from 'bun:test';

import {
  getProviderCommands,
  checkTranscriptionReady,
  hasDownloadingModel,
} from '../../src/lib/transcription-model-readiness';

describe('transcription model readiness', () => {
  test('uses Whisper commands when Whisper is the configured provider', () => {
    expect(getProviderCommands('localWhisper')).toEqual({
      initialize: 'whisper_init',
      hasAvailableModels: 'whisper_has_available_models',
      getAvailableModels: 'whisper_get_available_models',
    });
  });

  test('uses Parakeet commands when Parakeet is the configured provider', () => {
    expect(getProviderCommands('parakeet')).toEqual({
      initialize: 'parakeet_init',
      hasAvailableModels: 'parakeet_has_available_models',
      getAvailableModels: 'parakeet_get_available_models',
    });
  });

  test('does not silently treat an unsupported provider as Parakeet', () => {
    expect(getProviderCommands('deepgram')).toBeNull();
  });

  test('allows a configured remote provider without initializing local engines', async () => {
    const commands: string[] = [];
    const invoke = async <T>(command: string): Promise<T> => {
      commands.push(command);
      return { endpoint: 'http://localhost:8000/v1', model: 'whisper-1' } as T;
    };
    expect(await checkTranscriptionReady('custom-openai', invoke)).toBeTrue();
    expect(commands).toEqual(['api_get_custom_transcription_config']);
  });

  test('blocks remote recording with missing or invalid saved configuration', async () => {
    for (const config of [null, { endpoint: '', model: 'whisper' },
      { endpoint: 'ftp://example.com', model: 'whisper' },
      { endpoint: 'https://user:pass@example.com/v1', model: 'whisper' },
      { endpoint: 'https://example.com/v1', model: ' ' }]) {
      const invoke = async <T>(): Promise<T> => config as T;
      expect(await checkTranscriptionReady('custom-openai', invoke)).toBeFalse();
    }
  });

  test('still initializes the selected local engine before checking availability', async () => {
    const commands: string[] = [];
    const invoke = async <T>(command: string): Promise<T> => {
      commands.push(command);
      return false as T;
    };
    expect(await checkTranscriptionReady('localWhisper', invoke)).toBeFalse();
    expect(commands).toEqual(['whisper_init', 'whisper_has_available_models']);
  });

  test('recognizes only active downloads', () => {
    expect(hasDownloadingModel([{ status: 'Available' }])).toBeFalse();
    expect(hasDownloadingModel([{ status: 'Downloading' }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 0 } } }])).toBeTrue();
    expect(hasDownloadingModel([{ status: { Downloading: { progress: 42 } } }])).toBeTrue();
  });
});
