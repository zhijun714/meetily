import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalEvents = { ...await import('@tauri-apps/api/event') };
const originalAnalytics = { ...await import('../../src/lib/analytics') };
const originalToast = { ...await import('sonner') };
let listing: () => Promise<any>;
let defaultValue: () => Promise<string>;
let listener: () => void;
let subscribe: () => Promise<() => void>;
const stop = mock(() => {});
const invoke = mock(async (command: string) => command === 'api_list_templates' ? listing() : defaultValue());
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
mock.module('@tauri-apps/api/event', () => ({ ...originalEvents, listen: async (_name: string, callback: () => void) => { listener = callback; return subscribe(); } }));
mock.module('../../src/lib/analytics', () => ({ default: { trackFeatureUsed() {} } }));
mock.module('sonner', () => ({ toast: { success() {} } }));
const { useTemplates } = await import('../../src/hooks/meeting-details/useTemplates');
let current!: ReturnType<typeof useTemplates>;
let renderer: ReactTestRenderer | undefined;
const items = ['standard_meeting', 'mine', 'other'].map(id => ({ id, name: id, description: id }));
function Harness() { current = useTemplates(); return null; }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
async function show() { await act(async () => { renderer = create(<Harness />); }); }
beforeEach(() => { listing = async () => items; defaultValue = async () => 'mine'; subscribe = async () => stop; stop.mockClear(); invoke.mockClear(); });
afterEach(async () => { if (renderer) await act(async () => renderer!.unmount()); renderer = undefined; });
afterAll(() => { mock.module('@tauri-apps/api/core', () => originalCore); mock.module('@tauri-apps/api/event', () => originalEvents); mock.module('../../src/lib/analytics', () => originalAnalytics); mock.module('sonner', () => originalToast); });

test('waits for subscription and saved default, retains explicit choice on refresh', async () => {
  const registration = deferred<() => void>(); subscribe = () => registration.promise;
  await show(); expect(current.isLoading).toBe(true); expect(invoke).not.toHaveBeenCalled();
  await act(async () => registration.resolve(stop)); expect(current.selectedTemplate).toBe('mine'); expect(current.isLoading).toBe(false);
  await act(async () => current.handleTemplateSelection('other', 'Other'));
  defaultValue = async () => 'standard_meeting'; await act(async () => listener()); expect(current.selectedTemplate).toBe('other');
  listing = async () => items.filter(item => item.id !== 'other'); await act(async () => listener()); expect(current.selectedTemplate).toBe('standard_meeting');
});

test('older refresh cannot overwrite a newer snapshot or an explicit choice', async () => {
  await show(); const old = deferred<string>(); defaultValue = () => old.promise;
  await act(async () => listener()); defaultValue = async () => 'other'; await act(async () => listener());
  expect(current.selectedTemplate).toBe('other'); await act(async () => old.resolve('mine')); expect(current.selectedTemplate).toBe('other');
  const pending = deferred<string>(); defaultValue = () => pending.promise; await act(async () => listener());
  await act(async () => current.handleTemplateSelection('standard_meeting', 'Standard'));
  await act(async () => pending.resolve('mine')); expect(current.selectedTemplate).toBe('standard_meeting');
});

test('late event registration is released on unmount without starting hydration', async () => {
  const registration = deferred<() => void>(); subscribe = () => registration.promise; await show();
  await act(async () => renderer!.unmount()); renderer = undefined;
  await act(async () => registration.resolve(stop)); expect(stop).toHaveBeenCalledTimes(1); expect(invoke).not.toHaveBeenCalled();
});

test('subscription failure still hydrates and missing default falls back', async () => {
  subscribe = async () => { throw 'subscription failed'; }; defaultValue = async () => 'deleted';
  await show(); expect(current.selectedTemplate).toBe('standard_meeting'); expect(current.isLoading).toBe(false);
});
