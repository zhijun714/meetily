import { useEffect, useId, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { transcriptionConnectionError } from '@/lib/transcription-connection-error';

interface CustomTranscriptionConfig {
    endpoint: string;
    model: string;
    apiKey: string | null;
}

interface CustomTranscriptionSettingsProps {
    onSaved: (model: string) => void;
    onPendingChange: (pending: boolean) => void;
}

export function CustomTranscriptionSettings({ onSaved, onPendingChange }: CustomTranscriptionSettingsProps) {
    const id = useId();
    const [config, setConfig] = useState<CustomTranscriptionConfig>({ endpoint: '', model: '', apiKey: '' });
    const [isLoading, setIsLoading] = useState(true);
    const [loadFailed, setLoadFailed] = useState(false);
    const [loadAttempt, setLoadAttempt] = useState(0);
    const [operation, setOperation] = useState<'save' | 'test' | null>(null);
    const [error, setError] = useState('');
    const [status, setStatus] = useState('');
    const mounted = useRef(false);
    const pending = useRef(false);

    useEffect(() => {
        mounted.current = true;
        return () => {
            mounted.current = false;
            if (pending.current) onPendingChange(false);
        };
    }, [onPendingChange]);

    useEffect(() => {
        let disposed = false;
        setIsLoading(true);
        setLoadFailed(false);
        setError('');
        invoke<CustomTranscriptionConfig | null>('api_get_custom_transcription_config')
            .then(saved => {
                if (!disposed) {
                    setConfig({ endpoint: saved?.endpoint || '', model: saved?.model || '', apiKey: saved?.apiKey || '' });
                }
            })
            .catch(() => {
                if (!disposed) {
                    setLoadFailed(true);
                    setError('Could not load custom transcription settings. Retry before editing.');
                }
            })
            .finally(() => { if (!disposed) setIsLoading(false); });
        return () => { disposed = true; };
    }, [loadAttempt]);

    const update = (field: keyof CustomTranscriptionConfig, value: string) => {
        setConfig(previous => ({ ...previous, [field]: value }));
        setError('');
        setStatus('');
    };

    const run = async (action: 'save' | 'test') => {
        if (pending.current || isLoading || loadFailed) return;
        setError('');
        setStatus('');
        const endpoint = config.endpoint.trim();
        const model = config.model.trim();
        try {
            const url = new URL(endpoint);
            if (!['http:', 'https:'].includes(url.protocol) || !url.hostname || url.username || url.password || url.search || url.hash) {
                throw new Error('Invalid endpoint');
            }
        } catch {
            setError('Enter an HTTP or HTTPS base URL without credentials, query parameters, or a fragment.');
            return;
        }
        if (!model) {
            setError('Enter the transcription model served by your endpoint.');
            return;
        }

        pending.current = true;
        setOperation(action);
        onPendingChange(true);
        // Always send the key, including an empty string to explicitly clear it.
        const payload = { endpoint, model, apiKey: config.apiKey || '' };
        try {
            await invoke(action === 'save'
                ? 'api_save_custom_transcription_config'
                : 'api_test_custom_transcription_connection', payload);
        } catch (error) {
            if (mounted.current) {
                setError(action === 'save'
                    ? 'Could not save custom transcription settings. Check the configuration and try again.'
                    : transcriptionConnectionError(error));
            }
            pending.current = false;
            if (mounted.current) {
                setOperation(null);
                onPendingChange(false);
            }
            return;
        }

        if (!mounted.current) return;
        if (action === 'save') {
            // The save command already persists AND activates the provider atomically.
            // This event carries no credentials and lets recording readiness reload.
            try {
                await emit('transcript-config-updated', { provider: 'custom-openai', model });
            } catch {
                // Persistence succeeded; shared state must still reflect the saved provider.
            }
            if (!mounted.current) return;
            setStatus('Custom transcription saved and activated.');
        } else {
            setStatus('Connection test succeeded. Save to activate this configuration.');
        }
        pending.current = false;
        setOperation(null);
        onPendingChange(false);
        if (action === 'save') onSaved(model);
    };

    const disabled = isLoading || loadFailed || operation !== null;

    return (
        <div className="space-y-4 mt-6" aria-busy={isLoading || operation !== null}>
            <p className="text-sm text-gray-600">
                Meeting audio is sent to the server you configure for transcription. Choose a server you trust.
            </p>
            {isLoading && <p role="status" className="text-sm text-gray-600">Loading custom transcription settings...</p>}
            <div className="space-y-1">
                <Label htmlFor={`${id}-endpoint`}>Endpoint base URL</Label>
                <Input
                    id={`${id}-endpoint`}
                    type="url"
                    value={config.endpoint}
                    onChange={event => update('endpoint', event.target.value)}
                    disabled={disabled}
                    placeholder="http://localhost:8000/v1"
                    aria-describedby={`${id}-endpoint-hint`}
                    autoComplete="off"
                    spellCheck={false}
                />
                <p id={`${id}-endpoint-hint`} className="text-xs text-gray-500">
                    Enter the base URL, for example http://localhost:8000/v1. The server must support
                    {' '}audio/transcriptions; chat completions alone are not sufficient.
                </p>
            </div>
            <div className="space-y-1">
                <Label htmlFor={`${id}-model`}>Transcription model</Label>
                <Input
                    id={`${id}-model`}
                    value={config.model}
                    onChange={event => update('model', event.target.value)}
                    disabled={disabled}
                    placeholder="whisper-1"
                    autoComplete="off"
                    spellCheck={false}
                />
            </div>
            <div className="space-y-1">
                <Label htmlFor={`${id}-key`}>API key (optional)</Label>
                <Input
                    id={`${id}-key`}
                    type="password"
                    value={config.apiKey || ''}
                    onChange={event => update('apiKey', event.target.value)}
                    disabled={disabled}
                    placeholder="Leave blank if your server does not require a key"
                    aria-describedby={`${id}-key-hint`}
                    autoComplete="off"
                    spellCheck={false}
                />
                <p id={`${id}-key-hint`} className="text-xs text-gray-500">Saving a blank key clears the stored key.</p>
            </div>
            <p className="text-xs text-gray-500">
                Test Connection sends one second of generated silence. Testing does not save or activate settings.
            </p>
            {error && <p role="alert" className="text-sm text-red-600">{error}</p>}
            {status && <p role="status" className="text-sm text-green-700">{status}</p>}
            {loadFailed && (
                <Button type="button" variant="outline" onClick={() => setLoadAttempt(attempt => attempt + 1)}>
                    Retry loading settings
                </Button>
            )}
            <div className="flex flex-wrap gap-2">
                <Button type="button" variant="outline" disabled={disabled} onClick={() => void run('test')}>
                    {operation === 'test' ? 'Testing connection...' : 'Test Connection'}
                </Button>
                <Button type="button" disabled={disabled} onClick={() => void run('save')}>
                    {operation === 'save' ? 'Saving...' : 'Save and activate'}
                </Button>
            </div>
        </div>
    );
}
