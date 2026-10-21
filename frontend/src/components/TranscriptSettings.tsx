import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select';
import { Input } from './ui/input';
import { Button } from './ui/button';
import { Label } from './ui/label';
import { Eye, EyeOff, Lock, Unlock } from 'lucide-react';
import { ModelManager } from './WhisperModelManager';
import { ParakeetModelManager } from './ParakeetModelManager';
import { CustomTranscriptionSettings } from './CustomTranscriptionSettings';


export interface TranscriptModelProps {
    provider: 'localWhisper' | 'parakeet' | 'deepgram' | 'elevenLabs' | 'groq' | 'openai' | 'custom-openai';
    model: string;
    apiKey?: string | null;
}

export interface TranscriptSettingsProps {
    transcriptModelConfig: TranscriptModelProps;
    setTranscriptModelConfig: (config: TranscriptModelProps) => void;
    onModelSelect?: () => void;
}

export function TranscriptSettings({ transcriptModelConfig, setTranscriptModelConfig, onModelSelect }: TranscriptSettingsProps) {
    const [apiKey, setApiKey] = useState<string | null>(transcriptModelConfig.apiKey || null);
    const [showApiKey, setShowApiKey] = useState<boolean>(false);
    const [isApiKeyLocked, setIsApiKeyLocked] = useState<boolean>(true);
    const [isLockButtonVibrating, setIsLockButtonVibrating] = useState<boolean>(false);
    const [uiProvider, setUiProvider] = useState<TranscriptModelProps['provider']>(transcriptModelConfig.provider);
    const [isCustomRequestPending, setIsCustomRequestPending] = useState(false);
    const [isLocalSavePending, setIsLocalSavePending] = useState(false);
    const [localSaveError, setLocalSaveError] = useState('');
    const localSavePending = useRef(false);

    // Sync uiProvider when backend config changes (e.g., after model selection or initial load)
    useEffect(() => {
        setUiProvider(transcriptModelConfig.provider);
    }, [transcriptModelConfig.provider]);

    useEffect(() => {
        if (transcriptModelConfig.provider === 'localWhisper' || transcriptModelConfig.provider === 'parakeet') {
            setApiKey(null);
        }
    }, [transcriptModelConfig.provider]);

    const fetchApiKey = async (provider: string) => {
        try {

            const data = await invoke('api_get_transcript_api_key', { provider }) as string;

            setApiKey(data || '');
        } catch {
            setApiKey(null);
        }
    };
    const modelOptions = {
        localWhisper: [], // Model selection handled by ModelManager component
        parakeet: [], // Model selection handled by ParakeetModelManager component
        deepgram: ['nova-2-phonecall'],
        elevenLabs: ['eleven_multilingual_v2'],
        groq: ['llama-3.3-70b-versatile'],
        openai: ['gpt-4o'],
    };
    const requiresApiKey = uiProvider === 'deepgram' || uiProvider === 'elevenLabs' || uiProvider === 'openai' || uiProvider === 'groq';

    const handleInputClick = () => {
        if (isApiKeyLocked) {
            setIsLockButtonVibrating(true);
            setTimeout(() => setIsLockButtonVibrating(false), 500);
        }
    };

    const handleLocalModelSelect = async (provider: 'localWhisper' | 'parakeet', model: string) => {
        if (localSavePending.current) return;
        localSavePending.current = true;
        setIsLocalSavePending(true);
        setLocalSaveError('');
        try {
            // Managers call onModelSelect before their own auto-save. Own persistence here
            // so shared state and recording readiness only change after a successful save.
            await invoke('api_save_transcript_config', { provider, model, apiKey: null });
        } catch {
            setLocalSaveError('Could not save the local transcription model. Please try again.');
            localSavePending.current = false;
            setIsLocalSavePending(false);
            return;
        }
        try {
            await emit('transcript-config-updated', { provider, model });
        } catch {
            // Saving succeeded; notification failure must not prevent the state update.
        }
        localSavePending.current = false;
        setIsLocalSavePending(false);
        setTranscriptModelConfig({ provider, model, apiKey: null });
        onModelSelect?.();
    };

    return (
        <div>
            <div>
                {/* <div className="flex justify-between items-center mb-4">
                    <h3 className="text-lg font-semibold text-gray-900">Transcript Settings</h3>
                </div> */}
                <div className="space-y-4 pb-6">
                    <div>
                        <Label className="block text-sm font-medium text-gray-700 mb-1">
                            Transcript Model
                        </Label>
                        <div className="flex space-x-2 mx-1">
                            <Select
                                value={uiProvider}
                                disabled={isCustomRequestPending || isLocalSavePending}
                                onValueChange={(value) => {
                                    const provider = value as TranscriptModelProps['provider'];
                                    setUiProvider(provider);
                                    setLocalSaveError('');
                                    if (provider !== 'localWhisper' && provider !== 'parakeet' && provider !== 'custom-openai') {
                                        fetchApiKey(provider);
                                    }
                                }}
                            >
                                <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                    <SelectValue placeholder="Select provider" />
                                </SelectTrigger>
                                <SelectContent>
                                    <SelectItem value="parakeet">⚡ Parakeet (Recommended - Real-time / Accurate)</SelectItem>
                                    <SelectItem value="localWhisper">🏠 Local Whisper (High Accuracy)</SelectItem>
                                    <SelectItem value="custom-openai">Custom (OpenAI-compatible transcription)</SelectItem>
                                    {/* <SelectItem value="deepgram">☁️ Deepgram (Backup)</SelectItem>
                                    <SelectItem value="elevenLabs">☁️ ElevenLabs</SelectItem>
                                    <SelectItem value="groq">☁️ Groq</SelectItem>
                                    <SelectItem value="openai">☁️ OpenAI</SelectItem> */}
                                </SelectContent>
                            </Select>

                            {uiProvider !== 'localWhisper' && uiProvider !== 'parakeet' && uiProvider !== 'custom-openai' && (
                                <Select
                                    value={transcriptModelConfig.model}
                                    onValueChange={(value) => {
                                        const model = value as TranscriptModelProps['model'];
                                        setTranscriptModelConfig({ ...transcriptModelConfig, provider: uiProvider, model });
                                    }}
                                >
                                    <SelectTrigger className='focus:ring-1 focus:ring-blue-500 focus:border-blue-500'>
                                        <SelectValue placeholder="Select model" />
                                    </SelectTrigger>
                                    <SelectContent>
                                        {modelOptions[uiProvider].map((model) => (
                                            <SelectItem key={model} value={model}>{model}</SelectItem>
                                        ))}
                                    </SelectContent>
                                </Select>
                            )}

                        </div>
                    </div>

                    {uiProvider === 'custom-openai' && (
                        <CustomTranscriptionSettings
                            onPendingChange={setIsCustomRequestPending}
                            onSaved={(model) => {
                                setTranscriptModelConfig({ provider: 'custom-openai', model, apiKey: null });
                                onModelSelect?.();
                            }}
                        />
                    )}

                    {uiProvider === 'localWhisper' && (
                        <fieldset className="mt-6" disabled={isLocalSavePending}>
                            <ModelManager
                                selectedModel={transcriptModelConfig.provider === 'localWhisper' ? transcriptModelConfig.model : undefined}
                                onModelSelect={(model) => void handleLocalModelSelect('localWhisper', model)}
                                autoSave={false}
                            />
                        </fieldset>
                    )}

                    {uiProvider === 'parakeet' && (
                        <fieldset className="mt-6" disabled={isLocalSavePending}>
                            <ParakeetModelManager
                                selectedModel={transcriptModelConfig.provider === 'parakeet' ? transcriptModelConfig.model : undefined}
                                onModelSelect={(model) => void handleLocalModelSelect('parakeet', model)}
                                autoSave={false}
                            />
                        </fieldset>
                    )}


                    {isLocalSavePending && <p role="status" className="text-sm text-gray-600">Saving transcription model...</p>}
                    {localSaveError && <p role="alert" className="text-sm text-red-600">{localSaveError}</p>}

                    {requiresApiKey && (
                        <div>
                            <Label className="block text-sm font-medium text-gray-700 mb-1">
                                API Key
                            </Label>
                            <div className="relative mx-1">
                                <Input
                                    type={showApiKey ? "text" : "password"}
                                    className={`pr-24 focus:ring-1 focus:ring-blue-500 focus:border-blue-500 ${isApiKeyLocked ? 'bg-gray-100 cursor-not-allowed' : ''
                                        }`}
                                    value={apiKey || ''}
                                    onChange={(e) => setApiKey(e.target.value)}
                                    disabled={isApiKeyLocked}
                                    onClick={handleInputClick}
                                    placeholder="Enter your API key"
                                />
                                {isApiKeyLocked && (
                                    <div
                                        onClick={handleInputClick}
                                        className="absolute inset-0 flex items-center justify-center bg-gray-100 bg-opacity-50 rounded-md cursor-not-allowed"
                                    />
                                )}
                                <div className="absolute inset-y-0 right-0 pr-1 flex items-center">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setIsApiKeyLocked(!isApiKeyLocked)}
                                        className={`transition-colors duration-200 ${isLockButtonVibrating ? 'animate-vibrate text-red-500' : ''
                                            }`}
                                        title={isApiKeyLocked ? "Unlock to edit" : "Lock to prevent editing"}
                                    >
                                        {isApiKeyLocked ? <Lock className="h-4 w-4" /> : <Unlock className="h-4 w-4" />}
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon"
                                        onClick={() => setShowApiKey(!showApiKey)}
                                    >
                                        {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                                    </Button>
                                </div>
                            </div>
                        </div>
                    )}
                </div>
            </div>
        </div >
    )
}








