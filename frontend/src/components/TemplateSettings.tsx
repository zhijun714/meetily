'use client';

import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { registerNavigationGuard } from '@/lib/navigation-guard';
import { Button } from './ui/button';
import { Input } from './ui/input';
import { Label } from './ui/label';
import type { TemplateInfo } from '@/hooks/meeting-details/useTemplates';
import { duplicateTemplate, templateMarkdown, validateTemplateDraft, type TemplateEditor, type SummaryTemplate } from '@/lib/template-editor';
export type { TemplateEditor, SummaryTemplate, TemplateSection } from '@/lib/template-editor';
export type TemplateLeaveGuard = () => Promise<boolean>;
interface Props { onGuardChange?: (guard: TemplateLeaveGuard | null) => void }
const empty = (): TemplateEditor => ({ id: '', source: 'custom', template: { name: '', description: '', sections: [], markdown: '# <Meeting title>\n\n## 重点事项\n\n记录重要信息。' } });
const newId = () => `template_${crypto.randomUUID().replaceAll('-', '')}`;

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

  const adopt = useCallback((value: TemplateEditor, isNew = false) => {
    const converted = { ...value, template: { ...value.template, markdown: templateMarkdown(value.template) } };
    setDraft(converted); setCreating(isNew); setBaseline(isNew ? '' : JSON.stringify(converted));
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
    return () => { window.removeEventListener('beforeunload', beforeUnload); };
  }, []);

  const finishLeave = (allow: boolean) => {
    const resolve = leave.current; leave.current = null; setLeaveOpen(false); resolve?.(allow);
  };
  const save = async (): Promise<boolean> => {
    if (!draft || operation.current) return false;
    const validation = validateTemplateDraft(draft, creating);
    if (validation) { setError(validation); return false; }
    const template: SummaryTemplate = { name: draft.template.name, description: draft.template.description || draft.template.name, sections: [], markdown: draft.template.markdown };
    operation.current = true; setBusy(true); setError(''); setStatus('');
    try {
      const overrideBuiltin = !creating && draft.source !== 'custom';
      const saved = await invoke<TemplateEditor>('api_save_template', { templateId: draft.id, template, create: creating, overrideBuiltin });
      if (!mounted.current) return false;
      adopt(saved); setStatus('Template saved.');
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
    const value = duplicate && draft ? duplicateTemplate(draft) : empty();
    adopt({ ...value, id: newId() }, true);
  };
  const setTemplate = (patch: Partial<SummaryTemplate>) => {
    if (!draft) return;
    setDraft({ ...draft, template: { ...draft.template, ...patch } }); setError(''); setStatus('');
  };
  const makeDefault = async () => {
    if (!draft || creating || operation.current) return;
    operation.current = true; setBusy(true); setError('');
    try { await invoke('api_set_default_template', { templateId: draft.id }); if (mounted.current) { setDefaultId(draft.id); setStatus('Default template saved.'); } }
    catch { if (mounted.current) setError('Could not save the default template.'); }
    finally { operation.current = false; if (mounted.current) setBusy(false); }
  };
  const remove = async () => {
    if (!draft || draft.source !== 'custom' || creating || operation.current) return;
    operation.current = true; setBusy(true); setError('');
    try {
      await invoke('api_delete_template', { templateId: draft.id });
      if (!mounted.current) return;
      setDeleteOpen(false);
      await reloadList();
      const restored = draft.is_override ? await invoke<TemplateEditor>('api_get_template_editor', { templateId: draft.id }) : null;
      if (restored) adopt(restored); else { setDraft(null); setBaseline(''); }
      setStatus(restored ? 'Built-in template restored.' : 'Template deleted. Select a template or create a new one.');
    } catch { if (mounted.current) setError('Could not delete or refresh the template. Reload to check the current list.'); }
    finally { operation.current = false; if (mounted.current) setBusy(false); }
  };

  return <div className="space-y-5 py-6" aria-busy={busy}>
    <h2 className="text-xl font-semibold">Summary templates</h2>
    <p className="text-sm text-gray-600">Edit the Markdown instructions directly. Saving a built-in template creates a personal version; restoring it returns to the built-in version.</p>
    <div className="flex flex-wrap gap-2 items-end">
      <div className="space-y-1 flex-1 min-w-48"><Label htmlFor={`${uid}-select`}>Template</Label>
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
      <p className="text-sm text-gray-600">{draft.is_override ? 'Personal version' : draft.source === 'custom' ? 'Custom template' : 'Built-in template'}{dirty ? ' · Unsaved changes' : ''}</p>
      <fieldset disabled={busy || leaveOpen || deleteOpen} className="space-y-4">
        <div className="space-y-1"><Label htmlFor={`${uid}-name`}>Name</Label><Input id={`${uid}-name`} value={draft.template.name} onChange={e => setTemplate({ name: e.target.value })} /></div>
        <div className="space-y-1"><Label htmlFor={`${uid}-markdown`}>Markdown template</Label>
          <textarea id={`${uid}-markdown`} aria-label="Markdown template" spellCheck={false} className="border rounded p-3 w-full min-h-80 font-mono text-sm" value={draft.template.markdown ?? ''} onChange={e => setTemplate({ markdown: e.target.value })} />
          <p className="text-xs text-gray-500">Use ## headings for sections. Write what each section should contain; the instructions will be replaced by meeting content.</p>
        </div>
      </fieldset>
      <div className="flex flex-wrap gap-2">
        <Button disabled={busy || !dirty || leaveOpen || deleteOpen} onClick={() => void save()}>Save template</Button>
        <Button variant="outline" disabled={busy || creating || defaultId === draft.id || leaveOpen || deleteOpen} onClick={() => void makeDefault()}>Use as default</Button>
        {draft.source === 'custom' && !creating && <Button variant={draft.is_override ? 'outline' : 'destructive'} disabled={busy || leaveOpen || deleteOpen} onClick={() => setDeleteOpen(true)}>{draft.is_override ? 'Restore built-in' : 'Delete template'}</Button>}
      </div>
    </>}
    {leaveOpen && <div role="dialog" aria-modal="true" aria-label="Unsaved template changes" className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4">
      <div className="bg-white rounded p-6 space-y-4 max-w-lg"><h3 className="font-semibold">Unsaved template changes</h3><p>Save your changes or discard them before leaving this template.</p>
        <div className="flex flex-wrap gap-2"><Button disabled={busy} onClick={() => { void save().then(saved => { if (saved) finishLeave(true); }); }}>Save and continue</Button>
          <Button variant="outline" disabled={busy} onClick={() => finishLeave(true)}>Discard changes</Button><Button variant="outline" disabled={busy} onClick={() => finishLeave(false)}>Keep editing</Button></div>
        {error && <p role="alert">{error}</p>}
      </div>
    </div>}
    {deleteOpen && <div role="dialog" aria-modal="true" aria-label="Confirm template deletion" className="fixed inset-0 z-50 bg-black/40 flex items-center justify-center p-4">
      <div className="bg-white rounded p-6 space-y-4 max-w-lg"><h3 className="font-semibold">{draft?.is_override ? `Restore built-in ${draft.template.name}?` : `Delete ${draft?.template.name}?`}</h3><p>{draft?.is_override ? 'This removes your personal version and restores the built-in template.' : 'This deletes the custom template and any unsaved changes. Existing summaries are preserved.'}</p>
        <div className="flex gap-2"><Button variant="destructive" disabled={busy} onClick={() => void remove()}>{draft?.is_override ? 'Confirm restore' : 'Confirm delete'}</Button><Button variant="outline" disabled={busy} onClick={() => setDeleteOpen(false)}>Cancel</Button></div>
        {error && <p role="alert">{error}</p>}
      </div>
    </div>}
  </div>;
}
