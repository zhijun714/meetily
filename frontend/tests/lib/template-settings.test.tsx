import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { TemplateEditor, TemplateLeaveGuard } from '../../src/components/TemplateSettings';

const originalCore = { ...await import('@tauri-apps/api/core') };
const originalWindow = globalThis.window;
let commands: (command: string, args?: any) => Promise<any>;
const invoke = mock((command: string, args?: any) => commands(command, args));
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke, isTauri: () => false }));
const { TemplateSettings } = await import('../../src/components/TemplateSettings');
const builtin: TemplateEditor = { id: 'standard_meeting', source: 'builtin', template: { name: 'Standard', description: 'Meeting notes', sections: [
  { title: 'Summary', instruction: 'Summarize', format: 'paragraph', item_format: 'preferred', example_item_format: 'legacy' },
  { title: 'Actions', instruction: 'Extract tasks', format: 'list' },
] } };
let stored: TemplateEditor;
let renderer: ReactTestRenderer | undefined;
let guard: TemplateLeaveGuard | null = null;
const guardChanged = (value: TemplateLeaveGuard | null) => { guard = value; };
const copy = <T,>(value: T): T => JSON.parse(JSON.stringify(value));
const button = (label: string) => renderer!.root.findAllByType('button').find(node => node.children.join('') === label)!;
const input = (suffix: string) => renderer!.root.findAllByType('input').find(node => node.props.id.endsWith(suffix))!;
const text = () => JSON.stringify(renderer!.toJSON());
async function show() { await act(async () => { renderer = create(<TemplateSettings onGuardChange={guardChanged} />); }); }
async function click(label: string) { await act(async () => { button(label).props.onClick(); }); }
async function change(suffix: string, value: string) { await act(async () => { input(suffix).props.onChange({ target: { value } }); }); }
beforeEach(() => {
  globalThis.window = { addEventListener() {}, removeEventListener() {} } as any;
  stored = copy(builtin); guard = null; invoke.mockClear();
  commands = async (command, args) => {
    if (command === 'api_list_templates') return [builtin, { ...builtin, id: 'other' }].map(value => ({ id: value.id, name: value.template.name, description: value.template.description }));
    if (command === 'api_get_default_template') return 'standard_meeting';
    if (command === 'api_get_template_editor') return { ...copy(stored), id: args.templateId };
    if (command === 'api_save_template') return { id: args.templateId, source: 'custom', template: copy(args.template) };
    if (command === 'api_set_default_template' || command === 'api_delete_template') return null;
    throw new Error(command);
  };
});
afterEach(async () => { if (renderer) await act(async () => renderer!.unmount()); renderer = undefined; });
afterAll(() => { mock.module('@tauri-apps/api/core', () => originalCore); globalThis.window = originalWindow; });

test('builtin is read-only and duplication saves both optional hints to a new custom ID', async () => {
  await show();
  expect(renderer!.root.findByType('fieldset').props.disabled).toBe(true);
  expect(button('Delete template')).toBeUndefined();
  expect(button('Save template')).toBeUndefined();
  await click('Duplicate template');
  await change('-id', 'my_copy');
  expect(input('-name').props.value).toBe('Standard (copy)');
  await click('Save template');
  const save = invoke.mock.calls.find(([command]) => command === 'api_save_template')!;
  expect(save[1].create).toBe(true);
  expect(save[1].templateId).toBe('my_copy');
  expect(save[1].template.sections[0].item_format).toBe('preferred');
  expect(save[1].template.sections[0].example_item_format).toBe('legacy');
  expect(text()).toContain('Template saved.');
});

test('new template validates fields and retains draft on save failure', async () => {
  await show(); await click('New template'); await click('Save template');
  expect(invoke.mock.calls.filter(([c]) => c === 'api_save_template')).toHaveLength(0);
  await change('-id', 'new_template'); await change('-name', 'New'); await change('-description', 'Description'); await change('-title-0', 'Notes');
  await act(async () => { renderer!.root.findByType('textarea').props.onChange({ target: { value: 'Extract notes' } }); });
  const original = commands; commands = async (command, args) => { if (command === 'api_save_template') throw 'disk full'; return original(command, args); };
  await click('Save template');
  expect(text()).toContain('Could not save template'); expect(input('-name').props.value).toBe('New');
  expect(button('Save template').props.disabled).toBe(false);
  expect(text()).toContain('Unsaved changes');
});

test('section ordering and formats are saved and previewed', async () => {
  stored = { ...copy(builtin), id: 'mine', source: 'custom' };
  await show(); await click('Move section 2 up');
  expect(input('-title-0').props.value).toBe('Actions');
  await act(async () => { renderer!.root.findAllByType('select')[1].props.onChange({ target: { value: 'string' } }); });
  expect(text()).toContain('A single concise line.');
  await click('Save template');
  const saved = invoke.mock.calls.find(([c]) => c === 'api_save_template')![1].template;
  expect(saved.sections.map((s: any) => s.title)).toEqual(['Actions', 'Summary']);
  expect(saved.sections[0].format).toBe('string');
});

test('dirty guard requires explicit discard or successful save and supports cancellation', async () => {
  await show(); await click('Duplicate template'); await change('-id', 'copy');
  let first!: Promise<boolean>; await act(async () => { first = guard!(); });
  expect(text()).toContain('Unsaved template changes'); await click('Keep editing'); expect(await first).toBe(false);
  let second!: Promise<boolean>; await act(async () => { second = guard!(); });
  await click('Save and continue'); expect(await second).toBe(true);
  expect(text()).not.toContain('Unsaved changes');
  await change('-name', 'Another edit');
  let third!: Promise<boolean>; await act(async () => { third = guard!(); });
  await click('Discard changes'); expect(await third).toBe(true);
});

test('failed save in the leave dialog keeps navigation blocked and draft intact', async () => {
  await show(); await click('Duplicate template'); await change('-id', 'copy');
  const original = commands; commands = async (command, args) => { if (command === 'api_save_template') throw 'disk full'; return original(command, args); };
  let pending!: Promise<boolean>; let resolved = false;
  await act(async () => { pending = guard!(); void pending.then(() => { resolved = true; }); });
  await click('Save and continue');
  expect(resolved).toBe(false); expect(text()).toContain('Could not save template');
  expect(input('-name').props.value).toBe('Standard (copy)');
  await click('Keep editing'); expect(await pending).toBe(false);
});

test('switching with a draft waits for discard and deletion waits for confirmation', async () => {
  stored = { ...copy(builtin), source: 'custom' }; await show(); await change('-name', 'Draft');
  await act(async () => { renderer!.root.findAllByType('select')[0].props.onChange({ target: { value: 'other' } }); });
  expect(invoke.mock.calls.filter(([c]) => c === 'api_get_template_editor')).toHaveLength(1);
  await click('Discard changes');
  expect(invoke.mock.calls.filter(([c]) => c === 'api_get_template_editor')).toHaveLength(2);
  await click('Delete template'); expect(invoke.mock.calls.some(([c]) => c === 'api_delete_template')).toBe(false);
  await click('Confirm delete'); expect(invoke).toHaveBeenCalledWith('api_delete_template', { templateId: 'other' });
});

test('legacy custom IDs remain editable and default changes do not save drafts', async () => {
  stored = { ...copy(builtin), id: '周会.notes', source: 'custom' };
  const original = commands; commands = async (command, args) => command === 'api_get_template_editor' ? copy(stored) : original(command, args);
  await show(); await change('-name', 'Updated legacy'); await click('Save template');
  expect(invoke.mock.calls.find(([c]) => c === 'api_save_template')![1].templateId).toBe('周会.notes');
  await click('Use as default'); expect(invoke).toHaveBeenCalledWith('api_set_default_template', { templateId: '周会.notes' });
});
