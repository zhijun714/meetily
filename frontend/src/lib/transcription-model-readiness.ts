export interface ModelWithStatus {
  status?: unknown;
}

interface ProviderCommands {
  initialize: string;
  hasAvailableModels: string;
  getAvailableModels: string;
}

const PROVIDER_COMMANDS: Record<string, ProviderCommands> = {
  localWhisper: {
    initialize: 'whisper_init',
    hasAvailableModels: 'whisper_has_available_models',
    getAvailableModels: 'whisper_get_available_models',
  },
  parakeet: {
    initialize: 'parakeet_init',
    hasAvailableModels: 'parakeet_has_available_models',
    getAvailableModels: 'parakeet_get_available_models',
  },
};

export function getProviderCommands(provider: string): ProviderCommands | null {
  return PROVIDER_COMMANDS[provider] ?? null;
}

export async function checkTranscriptionReady(
  provider: string,
  invoke: <T>(command: string) => Promise<T>,
): Promise<boolean> {
  if (provider === 'custom-openai') {
    const config = await invoke<{ endpoint: string; model: string } | null>(
      'api_get_custom_transcription_config',
    );
    if (!config?.model.trim()) return false;
    try {
      const url = new URL(config.endpoint.trim());
      return ['http:', 'https:'].includes(url.protocol)
        && !!url.hostname && !url.username && !url.password && !url.search && !url.hash;
    } catch {
      return false;
    }
  }

  const commands = getProviderCommands(provider);
  if (!commands) return false;
  await invoke(commands.initialize);
  return invoke<boolean>(commands.hasAvailableModels);
}

export function hasDownloadingModel(models: ModelWithStatus[]): boolean {
  return models.some(({ status }) => (
    status === 'Downloading'
    || (status !== null && typeof status === 'object' && 'Downloading' in status)
  ));
}
