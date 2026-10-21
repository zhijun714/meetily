'use client';

import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { registerNavigationGuard } from '@/lib/navigation-guard';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import type { TemplateInfo } from '@/hooks/meeting-details/useTemplates';

import { duplicateTemplate, moveTemplateSection, validateTemplateDraft, type TemplateSection, type SummaryTemplate, type TemplateEditor } from '@/lib/template-editor';
export type { TemplateSection, SummaryTemplate, TemplateEditor } from '@/lib/template-editor';
export type TemplateLeaveGuard = () => Promise<boolean>;
interface Props { onGuardChange?: (guard: TemplateLeaveGuard | null) => void }
const section = (): TemplateSection => ({ title: '', instruction: '', format: 'paragraph' });
const empty = (): TemplateEditor => ({ id: '', source: 'custom', template: { name: '', description: '', sections: [section()] } });

export function TemplateSettings({ onGuardChange }: Props) {
  const uid = useId();
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [defaultId, setDefaultId] = useState('standard_meeting');
  const [draft, setDraft] = useState<TemplateEditor | null>(null);
  const [baseline, setBaseline] = useState('');
  const [creating, setCreating] = useState(false);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState('');
  const [status, setStatus] = useState('');
  const [leaveOpen, setLeaveOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const mounted = useRef(false);
  const operation = useRef(false);
  const leave = useRef<((allow: boolean) => void) | null>(null);
  const state = useRef({ dirty: false, busy: true });
  const dirty = draft !== null && JSON.stringify(draft) !== baseline;
  state.current = { dirty, busy };
  const readonly = draft?.source !== 'custom';

  const adopt = useCallback((value: TemplateEditor, isNew = false) => {
    setDraft(value); setCreating(isNew); setBaseline(isNew ? '' : JSON.stringify(value));
    setError(''); setStatus('');
  }, []);

  const reloadList = async () => {
    const [items, savedDefault] = await Promise.all([
      invoke<TemplateInfo[]>('api_list_templates'), invoke<string>('api_get_default_template'),
    ]);
    if (mounted.current) { setTemplates(items); setDefaultId(savedDefault); }
    return items;
  };

  const load = async () => {
    if (operation.current) return;
    operation.current = true; setBusy(true); setError('');
    try {
      const items = await reloadList();
      const first = items.find(item => item.id === 'standard_meeting') || items[0];
      if (first) {
        const value = await invoke<TemplateEditor>('api_get_template_editor', { templateId: first.id });
        if (mounted.current) adopt(value);
      }
    } catch { if (mounted.current) setError('Could not load templates. Retry loading before editing.'); }
    finally { operation.current = false; if (mounted.current) setBusy(false); }
  };

  const guard = useCallback<TemplateLeaveGuard>(() => {
    if (state.current.busy) return Promise.resolve(false);
    if (!state.current.dirty) return Promise.resolve(true);
    if (leave.current) return Promise.resolve(false);
    return new Promise(resolve => { leave.current = resolve; setLeaveOpen(true); });
  }, []);

  useEffect(() => {
    mounted.current = true;
    void load();
    return () => { mounted.current = false; leave.current?.(false); leave.current = null; };
    // Initial hydration belongs to this editor instance only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => registerNavigationGuard(guard), [guard]);
  useEffect(() => { onGuardChange?.(guard); return () => onGuardChange?.(null); }, [onGuardChange, guard]);

  useEffect(() => {
    const beforeUnload = (event: BeforeUnloadEvent) => {
      if (state.current.dirty || state.current.busy) { event.preventDefault(); event.returnValue = ''; }
    };
    window.addEventListener('beforeunload', beforeUnload);
    // Native window close hides the app in the tray and retains this mounted draft.
    return () => { window.removeEventListener('beforeunload', beforeUnload); };
  }, [guard]);

  const finishLeave = (allow: boolean) => {
    const resolve = leave.current; leave.current = null; setLeaveOpen(false); resolve?.(allow);
  };

  const save = async (): Promise<boolean> => {
    if (!draft || readonly || operation.current) return false;
    const validation = validateTemplateDraft(draft, creating);
    if (validation) { setError(validation); return false; }
    const value = draft.template;
    operation.current = true; setBusy(true); setError(''); setStatus('');
    try {
      const saved = await invoke<TemplateEditor>('api_save_template', { templateId: draft.id, template: value, create: creating });
      if (!mounted.current) return false;
      adopt(saved); setStatus('Template saved.');
      // Saving succeeded even if the subsequent list refresh fails.
      try { await reloadList(); } catch { if (mounted.current) setError('Template saved, but the list could not refresh. Retry loading.'); }
      return true;
    } catch (failure) { if (mounted.current) setError(`Could not save template: ${String(failure)}`); return false; }
    finally { operation.current = false; if (mounted.current) setBusy(false); }
  };

  const switchTo = async (id: string) => {
    if (!(await guard()) || operation.current) return;
    operation.current = true; setBusy(true); setError('');
    try {
      const value = await invoke<TemplateEditor>('api_get_template_editor', { templateId: id });
      if (mounted.current) adopt(value);
    } catch { if (mounted.current) setError('Could not open this template. Your draft has been preserved.'); }
    finally { operation.current = false; if (mounted.current) setBusy(false); }
  };

  const newDraft = async (duplicate: boolean) => {
    if (!(await guard())) return;
    if (duplicate && draft) {
      // Copy all fields, including both legacy optional formatting hints.
      adopt(duplicateTemplate(draft), true);
    } else adopt(empty(), true);
  };

  const setTemplate = (patch: Partial<SummaryTemplate>) => {
    if (!draft) return;
    setDraft({ ...draft, template: { ...draft.template, ...patch } }); setError(''); setStatus('');
  };
  const setSection = (index: number, patch: Partial<TemplateSection>) => {
    if (draft) setTemplate({ sections: draft.template.sections.map((s, i) => i === index ? { ...s, ...patch } : s) });
  };
  const move = (index: number, direction: number) => {
    if (!draft) return;
    setTemplate({ sections: moveTemplateSection(draft.template.sections, index, direction) });
  };
  const makeDefault = async () => {
    if (!draft || creating || operation.current) return;
    operation.current = true; setBusy(true); setError('');
    try { await invoke('api_set_default_template', { templateId: draft.id }); if (mounted.current) { setDefaultId(draft.id); setStatus('Default template saved.'); } }
    catch { if (mounted.current) setError('Could not save the default template.'); }
    finally { operation.current = false; if (mounted.current) setBusy(false); }
  };
  const remove = async () => {
    if (!draft || readonly || creating || operation.current) return;
    operation.current = true; setBusy(true); setError('');
    try {
      await invoke('api_delete_template', { templateId: draft.id });
      if (!mounted.current) return;
      setDeleteOpen(false); setDraft(null); setBaseline('');
      await reloadList(); setStatus('Template deleted. Select a template or create a new one.');
    } catch { if (mounted.current) setError('Could not delete or refresh the template. Reload to check the current list.'); }
    finally { operation.current = false; if (mounted.current) setBusy(false); }
  };

  return <div className="space-y-5 py-6" aria-busy={busy}>
    <h2 className="text-xl font-semibold">Summary templates</h2>
    <p className="text-sm text-gray-600">Choose a default for new meeting pages. Duplicate a built-in template to customize it.</p>
    <div className="flex flex-wrap gap-2 items-end">
      <div className="space-y-1 flex-1 min-w-48">
        <Label htmlFor={`${uid}-select`}>Template</Label>
        <select id={`${uid}-select`} className="border rounded p-2 w-full" disabled={busy || leaveOpen || deleteOpen}
          value={creating ? '' : draft?.id || ''} onChange={e => void switchTo(e.target.value)}>
          <option value="" disabled>{creating ? 'Unsaved custom template' : 'Select a template'}</option>
          {templates.map(item => <option key={item.id} value={item.id}>{item.name}{item.id === defaultId ? ' (default)' : ''}</option>)}
        </select>
      </div>
      <Button variant="outline" disabled={busy || leaveOpen || deleteOpen} onClick={() => void newDraft(false)}>New template</Button>
      <Button variant="outline" disabled={busy || !draft || leaveOpen || deleteOpen} onClick={() => void newDraft(true)}>Duplicate template</Button>
    </div>
    {error && <p role="alert" className="text-sm text-red-600">{error}</p>}
    {status && <p role="status" className="text-sm text-green-700">{status}</p>}
    <Button variant="outline" disabled={busy || dirty} onClick={() => void load()}>Reload templates</Button>
    {draft && <>
      <p className="text-sm text-gray-600">Source: {draft.source}{readonly ? ' · Read-only' : ''}{dirty ? ' · Unsaved changes' : ''}</p>
      <fieldset disabled={busy || readonly || leaveOpen || deleteOpen} className="space-y-4">
        <div className="space-y-1"><Label htmlFor={`${uid}-id`}>Template ID</Label><Input id={`${uid}-id`} value={draft.id} disabled={!creating}
          onChange={e => setDraft({ ...draft, id: e.target.value })} placeholder="my_meeting_template" /></div>
        <div className="space-y-1"><Label htmlFor={`${uid}-name`}>Name</Label><Input id={`${uid}-name`} value={draft.template.name} onChange={e => setTemplate({ name: e.target.value })} /></div>
        <div className="space-y-1"><Label htmlFor={`${uid}-description`}>Description</Label><Input id={`${uid}-description`} value={draft.template.description} onChange={e => setTemplate({ description: e.target.value })} /></div>
        {draft.template.sections.map((item, index) => <div key={index} className="space-y-2 border rounded p-3">
          <h3 className="font-medium">Section {index + 1}</h3>
          <Label htmlFor={`${uid}-title-${index}`}>Section title</Label><Input id={`${uid}-title-${index}`} value={item.title} onChange={e => setSection(index, { title: e.target.value })} />
          <Label htmlFor={`${uid}-instruction-${index}`}>Generation instruction</Label><textarea id={`${uid}-instruction-${index}`} className="border rounded p-2 w-full" value={item.instruction} onChange={e => setSection(index, { instruction: e.target.value })} />
          <Label htmlFor={`${uid}-format-${index}`}>Output format</Label><select id={`${uid}-format-${index}`} className="border rounded p-2 w-full" value={item.format} onChange={e => setSection(index, { format: e.target.value as TemplateSection['format'] })}>
            <option value="paragraph">Paragraph</option><option value="list">List</option><option value="string">Single line</option>
          </select>
          <Label htmlFor={`${uid}-item-${index}`}>Item format (optional)</Label><Input id={`${uid}-item-${index}`} value={item.item_format ?? ''} onChange={e => setSection(index, { item_format: e.target.value || null })} />
          <Label htmlFor={`${uid}-example-${index}`}>Example item format (optional)</Label><Input id={`${uid}-example-${index}`} value={item.example_item_format ?? ''} onChange={e => setSection(index, { example_item_format: e.target.value || null })} />
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" disabled={index === 0} onClick={() => move(index, -1)}>Move section {index + 1} up</Button>
            <Button variant="outline" disabled={index === draft.template.sections.length - 1} onClick={() => move(index, 1)}>Move section {index + 1} down</Button>
            <Button variant="outline" disabled={draft.template.sections.length === 1} onClick={() => setTemplate({ sections: draft.template.sections.filter((_, i) => i !== index) })}>Remove section {index + 1}</Button>
          </div>
        </div>)}
        <Button variant="outline" onClick={() => setTemplate({ sections: [...draft.template.sections, section()] })}>Add section</Button>
      </fieldset>
      <div className="flex flex-wrap gap-2">
        {!readonly && <Button disabled={busy || !dirty || leaveOpen || deleteOpen} onClick={() => void save()}>Save template</Button>}
        <Button variant="outline" disabled={busy || creating || defaultId === draft.id || leaveOpen || deleteOpen} onClick={() => void makeDefault()}>Use as default</Button>
        {!readonly && !creating && <Button variant="destructive" disabled={busy || leaveOpen || deleteOpen} onClick={() => setDeleteOpen(true)}>Delete template</Button>}
      </div>
      <section aria-label="Structure preview" className="border rounded p-4 space-y-3">
        <h3 className="font-semibold">Structure preview</h3><p className="text-xs text-gray-500">Illustrative structure. No summary is generated.</p>
        <h4 className="text-lg font-bold">Meeting title</h4>
        {draft.template.sections.map((item, index) => <div key={index}><h5 className="font-semibold">{item.title || 'Untitled section'}</h5>
          {item.format === 'list' ? <ul className="list-disc pl-5"><li>{item.item_format || item.example_item_format || 'List item'}</li></ul>
            : <p className="text-gray-500">{item.format === 'string' ? 'A single concise line.' : 'A paragraph describing this part of the meeting.'}</p>}
        </div>)}
      </section>
    </>}
    {leaveOpen && <div role="dialog" aria-modal="true" aria-label="Unsaved template changes" className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4">
      <div className="bg-white rounded p-6 space-y-4 max-w-lg"><h3 className="font-semibold">Unsaved template changes</h3><p>Save your changes or discard them before leaving this template.</p>
        <div className="flex flex-wrap gap-2"><Button disabled={busy} onClick={() => { void save().then(saved => { if (saved) finishLeave(true); }); }}>Save and continue</Button>
          <Button variant="outline" disabled={busy} onClick={() => finishLeave(true)}>Discard changes</Button><Button variant="outline" disabled={busy} onClick={() => finishLeave(false)}>Keep editing</Button></div>
        {error && <p role="alert">{error}</p>}
      </div>
    </div>}
    {deleteOpen && <div role="dialog" aria-modal="true" aria-label="Confirm template deletion" className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4">
      <div className="bg-white rounded p-6 space-y-4 max-w-lg"><h3 className="font-semibold">Delete {draft?.template.name}?</h3><p>This deletes the custom template and any unsaved changes. Existing summaries are preserved. A deleted default falls back to Standard Meeting.</p>
        <div className="flex gap-2"><Button variant="destructive" disabled={busy} onClick={() => void remove()}>Confirm delete</Button><Button variant="outline" disabled={busy} onClick={() => setDeleteOpen(false)}>Cancel deletion</Button></div>
        {error && <p role="alert">{error}</p>}
      </div>
    </div>}
  </div>;
}
