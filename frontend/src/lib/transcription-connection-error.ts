const PREFIX = 'Transcription engine failed: ';

const SAFE_ERRORS: Record<string, string> = {
  'Custom transcription request timed out': 'Connection test timed out. Check network access and whether the model service is available.',
  'Could not connect to custom transcription endpoint': 'Could not connect to the transcription server. Check the address, network, proxy, and HTTPS certificate.',
  "Custom transcription endpoint must return JSON with a string 'text' field": "The server responded, but did not return the expected JSON text field. Check the transcription API and response format.",
  'Enter a valid HTTP or HTTPS endpoint URL': 'Enter a valid HTTP or HTTPS base URL.',
  'Endpoint must be an HTTP or HTTPS URL with a host': 'Enter a valid HTTP or HTTPS base URL.',
  'Endpoint must not contain credentials, a query, or a fragment': 'Use a base URL without credentials, query parameters, or a fragment.',
  'Invalid transcription API key format': 'The API key contains invalid header characters. Paste only the key, without a Bearer prefix.',
};

export function transcriptionConnectionError(error: unknown): string {
  const raw = typeof error === 'string' ? error : error instanceof Error ? error.message : '';
  const message = raw.startsWith(PREFIX) ? raw.slice(PREFIX.length) : raw;
  if (SAFE_ERRORS[message]) return SAFE_ERRORS[message];

  // Only display recognized backend diagnostics, never arbitrary server text or credentials.
  const match = /^Custom transcription endpoint returned HTTP (\d{3})$/.exec(message);
  if (match) {
    const status = Number(match[1]);
    const hints: Record<number, string> = {
      400: 'The server rejected the audio or parameters. The test uses one second of generated silence; check the model name and audio requirements.',
      401: 'Authentication failed. Check the API key; paste the key without a Bearer prefix.',
      403: 'Access denied. Check the API key permissions and model access.',
      404: 'Endpoint or model not found. Enter the base URL (for example https://ai.gitee.com/v1); the app appends /audio/transcriptions.',
      405: 'This URL does not accept transcription uploads. Check the API base URL.',
      413: 'The server rejected the audio upload size.',
      415: 'The server does not accept the WAV audio format used by the app.',
      422: 'The server rejected the request fields or audio. Check the model identifier and required parameters.',
      429: 'The server rate limit or quota was exceeded. Check your balance and retry later.',
    };
    const hint = hints[status] || (status >= 300 && status < 400
      ? 'The server redirected the request. Use its final API base URL directly.'
      : status >= 500 ? 'The model service returned an error. Retry later or check the service status.'
        : 'Check the API endpoint and server configuration.');
    return `Connection test failed (HTTP ${status}). ${hint}`;
  }
  return 'Connection test failed. Check the endpoint, model, API key, and server support for audio/transcriptions.';
}
