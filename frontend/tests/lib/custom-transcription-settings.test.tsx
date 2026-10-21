import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const originalCore = { ...await import('@tauri-apps/api/core') };
const originalEvent = { ...await import('@tauri-apps/api/event') };
let load: () => Promise<unknown>;
let save: () => Promise<unknown>;
let connectionTest: () => Promise<unknown>;
const invoke = mock(async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
    if (command === 'api_get_custom_transcription_config') return load();
    if (command === 'api_save_custom_transcription_config') return save();
    if (command === 'api_test_custom_transcription_connection') return connectionTest();
    throw new Error(`Unexpected command: ${command}`);
});
const emit = mock(async (_event: string, _payload?: unknown) => {});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
mock.module('@tauri-apps/api/event', () => ({ ...originalEvent, emit }));
const { CustomTranscriptionSettings } = await import('../../src/components/CustomTranscriptionSettings');

afterAll(() => {
    mock.module('@tauri-apps/api/core', () => originalCore);
    mock.module('@tauri-apps/api/event', () => originalEvent);
});

const savedConfig = { endpoint: 'http://localhost:8000/v1', model: 'whisper-1', apiKey: 'stored-test-key' };
const onSaved = mock((_model: string) => {});
const onPendingChange = mock((_pending: boolean) => {});
let renderer: ReactTestRenderer | undefined;
beforeEach(() => {
    load = async () => savedConfig;
    save = async () => null;
    connectionTest = async () => null;
    invoke.mockClear(); emit.mockClear(); onSaved.mockClear(); onPendingChange.mockClear();
});
afterEach(async () => {
    if (renderer) await act(async () => { renderer!.unmount(); });
    renderer = undefined;
});
async function show() {
    await act(async () => {
        renderer = create(<CustomTranscriptionSettings onSaved={onSaved} onPendingChange={onPendingChange} />);
    });
}
const inputs = () => renderer!.root.findAllByType('input');
const button = (label: string) => renderer!.root.findAllByType('button').find(node => node.children.join('') === label)!;
const text = () => JSON.stringify(renderer!.toJSON());
async function change(index: number, value: string) {
    await act(async () => { inputs()[index].props.onChange({ target: { value } }); });
}
async function click(label: string) {
    await act(async () => { button(label).props.onClick(); });
}
function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>(done => { resolve = done; });
    return { promise, resolve };
}

describe('custom transcription settings', () => {
    test('first-run null config opens an editable form and saves once before activating', async () => {
        load = async () => null;
        const request = deferred<unknown>();
        save = () => request.promise;
        await show();
        expect(inputs().map(node => node.props.value)).toEqual(['', '', '']);
        expect(inputs().every(node => !node.props.disabled)).toBe(true);
        await change(0, savedConfig.endpoint);
        await change(1, savedConfig.model);
        await click('Save and activate');
        expect(onSaved).not.toHaveBeenCalled();
        expect(inputs().every(node => node.props.disabled)).toBe(true);
        expect(onPendingChange).toHaveBeenLastCalledWith(true);
        await act(async () => { request.resolve(null); });
        expect(invoke.mock.calls.filter(([command]) => command === 'api_save_custom_transcription_config')).toEqual([
            ['api_save_custom_transcription_config', { endpoint: savedConfig.endpoint, model: savedConfig.model, apiKey: '' }],
        ]);
        expect(emit).toHaveBeenCalledWith('transcript-config-updated', { provider: 'custom-openai', model: savedConfig.model });
        expect(onSaved).toHaveBeenCalledTimes(1);
        expect(onSaved).toHaveBeenCalledWith(savedConfig.model);
        expect(onPendingChange).toHaveBeenLastCalledWith(false);
    });

    test('saving an empty API key explicitly clears the stored key', async () => {
        await show();
        expect(inputs()[2].props.type).toBe('password');
        await change(2, '');
        await click('Save and activate');
        expect(invoke).toHaveBeenCalledWith('api_save_custom_transcription_config', { ...savedConfig, apiKey: '' });
    });

    test('test sends the current draft without saving or activating', async () => {
        await show();
        await change(1, 'draft-model');
        await click('Test Connection');
        expect(invoke).toHaveBeenCalledWith('api_test_custom_transcription_connection', { ...savedConfig, model: 'draft-model' });
        expect(invoke.mock.calls.filter(([command]) => command === 'api_save_custom_transcription_config')).toHaveLength(0);
        expect(onSaved).not.toHaveBeenCalled();
        expect(emit).not.toHaveBeenCalled();
        expect(text()).toContain('Connection test succeeded');
        await change(1, 'changed-after-test');
        expect(text()).not.toContain('Connection test succeeded');
    });

    test('failed saves keep the form open and do not expose server error credentials', async () => {
        save = async () => { throw new Error('secret-server-response'); };
        await show();
        await click('Save and activate');
        expect(onSaved).not.toHaveBeenCalled();
        expect(emit).not.toHaveBeenCalled();
        expect(renderer!.root.findAllByProps({ role: 'alert' })).toHaveLength(1);
        expect(text()).not.toContain('secret-server-response');
        expect(button('Save and activate').props.disabled).toBe(false);
        save = async () => null;
        await click('Save and activate');
        expect(onSaved).toHaveBeenCalledTimes(1);
    });

    test('failed tests never save or activate', async () => {
        connectionTest = async () => { throw new Error('secret-server-response'); };
        await show();
        await click('Test Connection');
        expect(text()).toContain('Connection test failed');
        expect(text()).not.toContain('secret-server-response');
        expect(onSaved).not.toHaveBeenCalled();
        expect(emit).not.toHaveBeenCalled();
        expect(button('Test Connection').props.disabled).toBe(false);
    });

    test('load failure blocks saving until retry successfully hydrates', async () => {
        load = async () => { throw new Error('load failure'); };
        await show();
        expect(button('Save and activate').props.disabled).toBe(true);
        expect(inputs().every(node => node.props.disabled)).toBe(true);
        load = async () => savedConfig;
        await click('Retry loading settings');
        expect(inputs()[0].props.value).toBe(savedConfig.endpoint);
        expect(button('Save and activate').props.disabled).toBe(false);
    });

    test('late hydration from a previous provider visit cannot overwrite a new draft', async () => {
        const oldLoad = deferred<unknown>();
        load = () => oldLoad.promise;
        await show();
        expect(inputs().every(node => node.props.disabled)).toBe(true);
        await act(async () => { renderer!.unmount(); });
        load = async () => null;
        await show();
        await change(1, 'new-visit-draft');
        await act(async () => { oldLoad.resolve(savedConfig); });
        expect(inputs()[1].props.value).toBe('new-visit-draft');
        expect(inputs()[0].props.value).toBe('');
    });

    test('leaving the form during a request releases the provider lock and ignores completion', async () => {
        const request = deferred<unknown>();
        connectionTest = () => request.promise;
        await show();
        await click('Test Connection');
        expect(onPendingChange).toHaveBeenLastCalledWith(true);
        await act(async () => { renderer!.unmount(); });
        renderer = undefined;
        expect(onPendingChange).toHaveBeenLastCalledWith(false);
        await act(async () => { request.resolve(null); });
        expect(onSaved).not.toHaveBeenCalled();
        expect(emit).not.toHaveBeenCalled();
    });
});
