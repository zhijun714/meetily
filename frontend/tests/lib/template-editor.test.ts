import { expect, test } from 'bun:test';
import { duplicateTemplate, moveTemplateSection, resolveTemplateSelection, validateTemplateDraft, type TemplateEditor } from '../../src/lib/template-editor';
const value = (): TemplateEditor => ({ id: 'standard_meeting', source: 'builtin', template: { name: 'Meeting', description: 'Notes', sections: [
  { title: 'Summary', instruction: 'Summarize', format: 'paragraph', item_format: 'hint', example_item_format: 'legacy' },
  { title: 'Actions', instruction: 'Tasks', format: 'list' },
] } });

test('duplicate creates an independent custom draft without losing either optional format', () => {
  const original = value(); const copy = duplicateTemplate(original);
  expect(copy.id).toBe(''); expect(copy.source).toBe('custom');
  expect(copy.template.sections[0]).toEqual(original.template.sections[0]);
  copy.template.sections[0].title = 'Changed'; expect(original.template.sections[0].title).toBe('Summary');
  expect(validateTemplateDraft(copy, true)).not.toBeNull(); copy.id = 'my_template';
  expect(validateTemplateDraft(copy, true)).toBeNull();
});

test('new IDs are strict and existing safe legacy IDs are kept for backend validation', () => {
  for (const id of ['../escape', 'space name', '周会.notes', 'a/b', 'a\\b', '']) {
    const draft = value(); draft.id = id; expect(validateTemplateDraft(draft, true)).not.toBeNull();
  }
  const draft = value(); draft.id = '周会.notes'; expect(validateTemplateDraft(draft, false)).toBeNull();
});

test('validation rejects whitespace and unsupported formats before invoking save', () => {
  for (const field of ['name', 'description'] as const) {
    const draft = value(); draft.template[field] = ' \n\t'; expect(validateTemplateDraft(draft, false)).not.toBeNull();
  }
  for (const field of ['title', 'instruction'] as const) {
    const draft = value(); draft.template.sections[0][field] = ' \n'; expect(validateTemplateDraft(draft, false)).not.toBeNull();
  }
  const draft = value(); draft.template.sections = []; expect(validateTemplateDraft(draft, false)).not.toBeNull();
  draft.template.sections = [{ title: 'Test', instruction: 'Test', format: 'html' as any }]; expect(validateTemplateDraft(draft, false)).not.toBeNull();
});

test('section reordering preserves whole sections and leaves original untouched', () => {
  const draft = value(); const moved = moveTemplateSection(draft.template.sections, 1, -1);
  expect(moved.map(s => s.title)).toEqual(['Actions', 'Summary']); expect(moved[1].example_item_format).toBe('legacy');
  expect(draft.template.sections[0].title).toBe('Summary');
  expect(moveTemplateSection(moved, 0, -1)).toBe(moved);
});

test('persisted default initializes new pages while explicit valid selections survive refresh', () => {
  const ids = ['standard_meeting', 'my_template', 'other'];
  expect(resolveTemplateSelection(ids, 'my_template', 'standard_meeting', false)).toBe('my_template');
  expect(resolveTemplateSelection(ids, 'my_template', 'other', true)).toBe('other');
  expect(resolveTemplateSelection(ids, 'my_template', 'deleted', true)).toBe('my_template');
  expect(resolveTemplateSelection(ids, 'deleted', 'deleted', true)).toBe('standard_meeting');
});
