'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface DocumentPreferences {
  transcript_enabled: boolean;
  transcript_folder: string | null;
  summary_enabled: boolean;
  summary_folder: string | null;
}
const defaults: DocumentPreferences = {
  transcript_enabled: false, transcript_folder: null,
  summary_enabled: false, summary_folder: null,
};

export function DocumentSettings() {
  const [draft, setDraft] = useState(defaults);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState(false);
  const generation = useRef(0);
  const pending = useRef(false);
  const load = useCallback(async () => {
    const visit = ++generation.current;
    setLoaded(false); setError('');
    try {
      const preferences = await invoke<DocumentPreferences>('api_get_document_preferences');
      if (visit === generation.current) { setDraft(preferences); setLoaded(true); }
    } catch (error) { if (visit === generation.current) setError(`Cannot load document settings: ${String(error)}`); }
  }, []);
  useEffect(() => { void load(); return () => { generation.current++; }; }, [load]);
  const update = (changes: Partial<DocumentPreferences>) => {
    setDraft(previous => ({ ...previous, ...changes })); setSaved(false); setError('');
  };
  const browse = async (kind: 'transcript' | 'summary') => {
    if (!loaded || pending.current) return;
    pending.current = true;
    const visit = generation.current;
    setBusy(true); setError('');
    try {
      const folder = await invoke<string | null>('api_select_document_directory');
      if (visit === generation.current && folder !== null) update({ [`${kind}_folder`]: folder });
    } catch (error) { if (visit === generation.current) setError(`Cannot select directory: ${String(error)}`); }
    finally { pending.current = false; if (visit === generation.current) setBusy(false); }
  };
  const save = async () => {
    if (!loaded || pending.current) return;
    if ((draft.transcript_enabled && !draft.transcript_folder) || (draft.summary_enabled && !draft.summary_folder)) {
      setError('Choose a directory for each enabled document type.'); return;
    }
    pending.current = true;
    const visit = generation.current;
    setBusy(true); setError(''); setSaved(false);
    try {
      const preferences = await invoke<DocumentPreferences>('api_save_document_preferences', { preferences: draft });
      if (visit === generation.current) { setDraft(preferences); setSaved(true); }
    } catch (error) { if (visit === generation.current) setError(`Document settings were not saved: ${String(error)}`); }
    finally { pending.current = false; if (visit === generation.current) setBusy(false); }
  };
  const open = async (folder: string) => {
    const visit = generation.current;
    setError('');
    try { await invoke('api_open_document_directory', { folder }); }
    catch (error) { if (visit === generation.current) setError(`Cannot open directory: ${String(error)}`); }
  };
  return (
    <section className="space-y-6 max-w-3xl">
      <div>
        <h2 className="text-xl font-semibold text-gray-900">Documents</h2>
        <p className="mt-2 text-sm text-gray-600">Save complete transcripts as TXT and saved summaries as Markdown. Meeting data remains available in Meetily if a document cannot be written.</p>
      </div>
      {(['transcript', 'summary'] as const).map(kind => {
        const label = kind === 'transcript' ? 'Transcript TXT' : 'Summary Markdown';
        const enabled = draft[`${kind}_enabled`];
        const folder = draft[`${kind}_folder`];
        return <fieldset key={kind} disabled={!loaded || busy} className="rounded-lg border border-gray-200 p-4 space-y-3">
          <legend className="px-1 font-medium">{label}</legend>
          <label className="flex gap-3 items-center text-sm">
            <input type="checkbox" checked={enabled} onChange={event => update({ [`${kind}_enabled`]: event.target.checked })} />
            Automatically save {label}
          </label>
          <p className="text-sm text-gray-600 break-all" aria-label={`${label} directory`}>{folder || 'No directory selected'}</p>
          <div className="flex gap-3">
            <button type="button" className="rounded border px-3 py-1.5 text-sm" onClick={() => void browse(kind)}>Choose {label} directory</button>
            <button type="button" disabled={!folder || !loaded || busy} className="rounded border px-3 py-1.5 text-sm disabled:opacity-50" onClick={() => folder && void open(folder)}>Open {label} directory</button>
          </div>
        </fieldset>;
      })}
      <p className="text-sm text-gray-600">Directory changes apply to first exports. Existing documents keep their original paths. Files edited outside Meetily are preserved; move a conflicting file aside before retrying.</p>
      {error && <p role="alert" className="text-sm text-red-700">{error}</p>}
      {!loaded && error && <button type="button" onClick={() => void load()} className="rounded border px-3 py-2 text-sm">Retry loading document settings</button>}
      {saved && <p role="status" className="text-sm text-green-700">Document settings saved.</p>}
      <button type="button" disabled={!loaded || busy} onClick={() => void save()} className="rounded bg-blue-600 px-4 py-2 text-sm text-white disabled:opacity-50">{busy ? 'Working…' : 'Save document settings'}</button>
    </section>
  );
}
export default DocumentSettings;
