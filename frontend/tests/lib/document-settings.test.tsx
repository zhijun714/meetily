import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalEvent = { ...await import('@tauri-apps/api/event') };
let handler: (command: string, args?: any) => Promise<any>;
let listener: (event: { payload: any }) => void;
const invoke = mock((command: string, args?: any) => handler(command, args));
const cleanup = mock(() => {});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
mock.module('@tauri-apps/api/event', () => ({ ...originalEvent, listen: async (_name: string, callback: typeof listener) => { listener = callback; return cleanup; } }));
const { DocumentSettings } = await import('../../src/components/DocumentSettings');
const { DocumentExports } = await import('../../src/components/MeetingDetails/DocumentExports');
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('@tauri-apps/api/event', () => originalEvent);
});
let renderer: ReactTestRenderer | undefined;
const prefs = { transcript_enabled: true, transcript_folder: '/txt', summary_enabled: false, summary_folder: '/md' };
beforeEach(() => { invoke.mockClear(); cleanup.mockClear(); handler = async command => command === 'api_get_document_preferences' ? prefs : null; });
afterEach(async () => { if (renderer) await act(async () => renderer!.unmount()); renderer = undefined; });
const buttons = () => renderer!.root.findAllByType('button');
const button = (label: string) => buttons().find(node => node.children.join('') === label)!;
const text = () => JSON.stringify(renderer!.toJSON());
const click = async (label: string) => { await act(async () => { await button(label).props.onClick(); }); };
const show = async () => { await act(async () => { renderer = create(<DocumentSettings />); }); };
test('hydrates independent toggles and cancellation preserves selected folder', async () => {
  await show();
  expect(renderer!.root.findAllByType('input').map(node => node.props.checked)).toEqual([true, false]);
  await click('Choose Transcript TXT directory');
  expect(text()).toContain('/txt');
  expect(invoke.mock.calls.some(([command]) => command === 'api_save_document_preferences')).toBe(false);
});
test('validates enabled folder and preserves draft on backend save failure', async () => {
  handler = async command => command === 'api_get_document_preferences' ? { ...prefs, transcript_folder: null } : null;
  await show(); await click('Save document settings');
  expect(text()).toContain('Choose a directory');
  expect(invoke.mock.calls.some(([command]) => command === 'api_save_document_preferences')).toBe(false);
  handler = async command => {
    if (command === 'api_select_document_directory') return '/chosen';
    if (command === 'api_save_document_preferences') throw 'disk unavailable';
  };
  await click('Choose Transcript TXT directory'); await click('Save document settings');
  expect(text()).toContain('were not saved'); expect(text()).toContain('/chosen');
  expect(button('Save document settings').props.disabled).toBe(false);
});
test('load failure blocks saving and retry hydrates before enabling actions', async () => {
  handler = async () => { throw 'load failed'; };
  await show(); expect(button('Save document settings').props.disabled).toBe(true);
  handler = async () => prefs;
  await click('Retry loading document settings');
  expect(button('Save document settings').props.disabled).toBe(false);
});
test('same tick directory clicks open one native dialog', async () => {
  await show();
  let resolve!: (folder: string | null) => void;
  handler = async () => new Promise(done => { resolve = done; });
  await act(async () => { button('Choose Transcript TXT directory').props.onClick(); button('Choose Transcript TXT directory').props.onClick(); });
  expect(invoke.mock.calls.filter(([command]) => command === 'api_select_document_directory')).toHaveLength(1);
  await act(async () => { resolve(null); });
});
test('meeting export failure exposes retry and success clears the failure', async () => {
  const result = { meeting_id: 'm', kind: 'transcript', success: false, skipped: false, error: 'directory offline', path: null };
  handler = async command => command === 'api_get_document_export_status' ? [result] : { ...result, success: true, error: null, path: '/txt/meeting.txt' };
  await act(async () => { renderer = create(<DocumentExports meetingId="m" />); });
  expect(text()).toContain('directory offline');
  await click('Retry TXT');
  expect(invoke).toHaveBeenCalledWith('api_export_meeting_document', { meetingId: 'm', kind: 'transcript' });
  expect(text()).not.toContain('directory offline'); expect(text()).toContain('/txt/meeting.txt');
});
test('unrelated meeting events do not discard persisted status hydration', async () => {
  let resolve!: (value: any) => void;
  handler = async () => new Promise(done => { resolve = done; });
  await act(async () => { renderer = create(<DocumentExports meetingId="m" />); });
  await act(async () => {
    listener({ payload: { meeting_id: 'other', kind: 'transcript', success: true } });
    resolve([{ meeting_id: 'm', kind: 'summary', error: 'stored error', path: null }]);
  });
  expect(text()).toContain('stored error');
});
