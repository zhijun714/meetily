import { afterAll, afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { TemplateEditor, TemplateLeaveGuard } from '../../src/components/TemplateSettings';

const originalCore = { ...await import('@tauri-apps/api/core') };
const originalWindow = globalThis.window;
let commands: (command: string, args?: any) => Promise<any>;
const invoke = mock((command: string, args?: any) => commands(command, args));
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke, isTauri: () => false }));
const { TemplateSettings } = await import('../../src/components/TemplateSettings');
const builtin: TemplateEditor = { id: 'standard_meeting', source: 'builtin', template: { name: 'Standard', description: 'Meeting notes', sections: [], markdown: '# <Title>\n\n## 重点事项\n简短记录。' } };
let stored: TemplateEditor;
let renderer: ReactTestRenderer | undefined;
let guard: TemplateLeaveGuard | null = null;
const guardChanged = (value: TemplateLeaveGuard | null) => { guard = value; };
const copy = <T,>(value: T): T => JSON.parse(JSON.stringify(value));
const button = (label: string) => renderer!.root.findAllByType('button').find(node => node.children.join('') === label)!;
const input = (suffix: string) => renderer!.root.findAllByType('input').find(node => node.props.id.endsWith(suffix))!;
const markdown = () => renderer!.root.findByType('textarea');
const text = () => JSON.stringify(renderer!.toJSON());
async function show() { await act(async () => { renderer = create(<TemplateSettings onGuardChange={guardChanged} />); }); }
async function click(label: string) { await act(async () => { button(label).props.onClick(); }); }
async function change(suffix: string, value: string) { await act(async () => { input(suffix).props.onChange({ target: { value } }); }); }
async function edit(value: string) { await act(async () => { markdown().props.onChange({ target: { value } }); }); }

beforeEach(() => {
  globalThis.window = { addEventListener() {}, removeEventListener() {} } as any;
  stored = copy(builtin); guard = null; invoke.mockClear();
  commands = async (command, args) => {
    if (command === 'api_list_templates') return [stored, { ...stored, id: 'other' }].map(value => ({ id: value.id, name: value.template.name, description: value.template.description }));
    if (command === 'api_get_default_template') return 'standard_meeting';
    if (command === 'api_get_template_editor') return { ...copy(stored), id: args.templateId };
    if (command === 'api_save_template') { stored = { id: args.templateId, source: 'custom', is_override: args.overrideBuiltin, template: copy(args.template) }; return copy(stored); }
    if (command === 'api_delete_template') { stored = copy(builtin); return null; }
    if (command === 'api_set_default_template') return null;
    throw new Error(command);
  };
});
afterEach(async () => { if (renderer) await act(async () => renderer!.unmount()); renderer = undefined; });
afterAll(() => { mock.module('@tauri-apps/api/core', () => originalCore); globalThis.window = originalWindow; });

test('builtin Markdown can be edited directly and saved as a personal override, then restored', async () => {
  await show();
  expect(renderer!.root.findAllByType('textarea')).toHaveLength(1);
  expect(renderer!.root.findAllByType('input')).toHaveLength(1);
  await edit('# <Title>\n\n## 结论\n保留事实。');
  await click('Save template');
  const save = invoke.mock.calls.find(([command]) => command === 'api_save_template')!;
  expect(save[1].templateId).toBe('standard_meeting');
  expect(save[1].overrideBuiltin).toBe(true);
  expect(save[1].template.markdown).toContain('## 结论');
  expect(save[1].template.sections).toEqual([]);
  await click('Restore built-in'); await click('Confirm restore');
  expect(markdown().props.value).toBe(builtin.template.markdown);
});

test('new template generates its own ID and retains draft on failed save', async () => {
  await show(); await click('New template');
  expect(renderer!.root.findAllByType('input')).toHaveLength(1);
  await change('-name', 'New'); await edit('## 结论\n只写事实');
  const original = commands; commands = async (command, args) => { if (command === 'api_save_template') throw 'disk full'; return original(command, args); };
  await click('Save template');
  const save = invoke.mock.calls.find(([c]) => c === 'api_save_template')![1];
  expect(save.templateId).toMatch(/^template_[a-f0-9]{32}$/);
  expect(text()).toContain('Could not save template');
  expect(markdown().props.value).toBe('## 结论\n只写事实');
  expect(text()).toContain('Unsaved changes');
});

test('legacy instructions survive duplication in one Markdown field', async () => {
  stored = { ...copy(builtin), template: { name: 'Legacy', description: 'Notes', sections: [
    { title: 'Decisions', instruction: 'List decisions', format: 'list', item_format: 'preferred', example_item_format: 'legacy' },
  ] } };
  await show(); await click('Duplicate template');
  expect(markdown().props.value).toContain('preferred'); expect(markdown().props.value).toContain('legacy');
  await click('Save template');
  expect(invoke.mock.calls.find(([c]) => c === 'api_save_template')![1].template.markdown).toContain('List decisions');
});

test('dirty guard requires explicit discard or successful save and supports cancellation', async () => {
  await show(); await edit('## 结论\n决定。');
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
  await show(); await edit('## 结论\n事实');
  const original = commands; commands = async (command, args) => { if (command === 'api_save_template') throw 'disk full'; return original(command, args); };
  let pending!: Promise<boolean>; let resolved = false;
  await act(async () => { pending = guard!(); void pending.then(() => { resolved = true; }); });
  await click('Save and continue');
  expect(resolved).toBe(false); expect(text()).toContain('Could not save template');
  expect(markdown().props.value).toContain('事实');
  await click('Keep editing'); expect(await pending).toBe(false);
});

test('switching with a draft waits for discard; custom deletion waits for confirmation', async () => {
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
