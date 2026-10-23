export interface TemplateSection {
  title: string;
  instruction: string;
  format: 'paragraph' | 'list' | 'string';
  item_format?: string | null;
  example_item_format?: string | null;
}
export interface SummaryTemplate { name: string; description: string; sections: TemplateSection[]; markdown?: string | null }
export interface TemplateEditor { id: string; source: 'builtin' | 'bundled' | 'custom'; template: SummaryTemplate; is_override?: boolean }

const outputFormat: Record<TemplateSection['format'], string> = {
  paragraph: 'Write prose paragraphs. Do not use a bullet list.',
  list: 'Write a Markdown bullet list, with one item per point.',
  string: 'Write a single concise line of plain text, without bullets or line breaks.',
};

export function templateMarkdown(template: SummaryTemplate): string {
  if (template.markdown != null) return template.markdown;
  const sections = template.sections.map(section => [
    `## ${section.title}`, section.instruction, outputFormat[section.format] ?? '',
    section.item_format ? `Item format:\n\n${section.item_format}` : '',
    section.example_item_format ? `Example item format:\n\n${section.example_item_format}` : '',
  ].filter(Boolean).join('\n\n')).join('\n\n');
  return `# <Add Title here>\n\n${template.description}\n\nAnalyze the entire transcript and create a concise, descriptive title for the meeting.\n\n${sections}`;
}

export function validateTemplateDraft(draft: TemplateEditor, creating: boolean): string | null {
  if (creating && (!draft.id || !/^[A-Za-z0-9_-]{1,100}$/.test(draft.id))) {
    return 'Template identifier is invalid. Recreate the draft.';
  }
  if (!draft.template.name.trim()) return 'Enter a template name.';
  const markdown = templateMarkdown(draft.template);
  if (!markdown.trim() || !/^##\s+\S/m.test(markdown)) return 'Enter Markdown with at least one ## section heading.';
  return null;
}

export function duplicateTemplate(draft: TemplateEditor): TemplateEditor {
  const template: SummaryTemplate = { name: `${draft.template.name} (copy)`, description: draft.template.description,
    sections: [], markdown: templateMarkdown(draft.template) };
  return { id: '', source: 'custom', template };
}

export function moveTemplateSection(sections: TemplateSection[], index: number, direction: number): TemplateSection[] {
  if (index < 0 || index >= sections.length || index + direction < 0 || index + direction >= sections.length) return sections;
  const result = [...sections];
  [result[index], result[index + direction]] = [result[index + direction], result[index]];
  return result;
}

export function resolveTemplateSelection(ids: string[], defaultId: string, current: string, explicit: boolean): string {
  if (explicit && ids.includes(current)) return current;
  return ids.includes(defaultId) ? defaultId : 'standard_meeting';
}
