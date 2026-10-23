import { expect, test } from 'bun:test';
import { duplicateTemplate, resolveTemplateSelection, templateMarkdown, validateTemplateDraft, type TemplateEditor } from '../../src/lib/template-editor';

const legacy = (): TemplateEditor => ({ id: 'standard_meeting', source: 'builtin', template: { name: 'Meeting', description: 'Notes', sections: [
  { title: 'Summary', instruction: 'Summarize', format: 'paragraph', item_format: 'preferred', example_item_format: 'legacy' },
  { title: 'Actions', instruction: 'Tasks', format: 'list' },
] } });

test('legacy templates preserve every generation hint when edited as Markdown', () => {
  const original = legacy();
  const markdown = templateMarkdown(original.template);
  for (const text of ['Notes', '## Summary', 'Summarize', 'prose paragraphs', 'preferred', 'legacy', '## Actions', 'Markdown bullet list']) {
    expect(markdown).toContain(text);
  }
  const copy = duplicateTemplate(original);
  expect(copy.source).toBe('custom');
  expect(copy.template.markdown).toBe(markdown);
  expect(copy.template.sections).toEqual([]);
  expect(original.template.markdown).toBeUndefined();
});

test('name and Markdown source are the only required user content', () => {
  const draft = legacy();
  draft.id = 'template_123'; draft.template.description = ''; draft.template.sections = [];
  draft.template.markdown = '# Short title\n\n## 结论\n保留事实';
  expect(validateTemplateDraft(draft, true)).toBeNull();
  draft.template.markdown = 'no section';
  expect(validateTemplateDraft(draft, true)).not.toBeNull();
  draft.template.markdown = '## 结论\n内容'; draft.template.name = '  ';
  expect(validateTemplateDraft(draft, true)).not.toBeNull();
  draft.template.name = 'Mine'; draft.id = '../escape';
  expect(validateTemplateDraft(draft, true)).not.toBeNull();
  expect(validateTemplateDraft({ ...draft, id: '周会.notes' }, false)).toBeNull();
});

test('persisted default initializes new pages while explicit valid selections survive refresh', () => {
  const ids = ['standard_meeting', 'my_template', 'other'];
  expect(resolveTemplateSelection(ids, 'my_template', 'standard_meeting', false)).toBe('my_template');
  expect(resolveTemplateSelection(ids, 'my_template', 'other', true)).toBe('other');
  expect(resolveTemplateSelection(ids, 'my_template', 'deleted', true)).toBe('my_template');
  expect(resolveTemplateSelection(ids, 'deleted', 'deleted', true)).toBe('standard_meeting');
});
