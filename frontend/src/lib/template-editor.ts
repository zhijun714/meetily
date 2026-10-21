export interface TemplateSection {
  title: string;
  instruction: string;
  format: 'paragraph' | 'list' | 'string';
  item_format?: string | null;
  example_item_format?: string | null;
}
export interface SummaryTemplate { name: string; description: string; sections: TemplateSection[] }
export interface TemplateEditor { id: string; source: 'builtin' | 'bundled' | 'custom'; template: SummaryTemplate }

export function validateTemplateDraft(draft: TemplateEditor, creating: boolean): string | null {
  if (creating && !/^[A-Za-z0-9_-]{1,100}$/.test(draft.id)) {
    return 'Enter an ID using letters, numbers, underscores or hyphens (1–100 characters).';
  }
  const value = draft.template;
  if (!value.name.trim() || !value.description.trim() || !value.sections.length || value.sections.some(s => !s.title.trim() || !s.instruction.trim())) {
    return 'Enter a name, description, and a title and instruction for every section.';
  }
  if (value.sections.some(s => !['paragraph', 'list', 'string'].includes(s.format))) return 'Choose a supported section output format.';
  return null;
}

export function duplicateTemplate(draft: TemplateEditor): TemplateEditor {
  const copy: TemplateEditor = JSON.parse(JSON.stringify(draft));
  copy.id = ''; copy.source = 'custom'; copy.template.name += ' (copy)';
  return copy;
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
