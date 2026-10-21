'use client';
import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type Kind = 'transcript' | 'summary';
export interface DocumentExportResult {
  meeting_id: string;
  kind: Kind;
  success: boolean;
  skipped: boolean;
  error: string | null;
  path: string | null;
}
export function DocumentExports({ meetingId }: { meetingId: string }) {
  const [results, setResults] = useState<Partial<Record<Kind, DocumentExportResult>>>({});
  const [busy, setBusy] = useState<Kind | null>(null);
  const [loadError, setLoadError] = useState('');
  const visit = useRef(0);
  useEffect(() => {
    const current = ++visit.current;
    let unlisten: (() => void) | undefined;
    const eventVersion: Record<Kind, number> = { transcript: 0, summary: 0 };
    setResults({}); setBusy(null); setLoadError('');
    const accept = (result: DocumentExportResult) => {
      if (visit.current === current && result.meeting_id === meetingId && !result.skipped) {
        setResults(previous => ({ ...previous, [result.kind]: result }));
      }
    };
    // Subscribe first so an automatic export cannot be missed while loading persisted failures.
    void listen<DocumentExportResult>('document-export-result', event => {
      if (event.payload.meeting_id !== meetingId || event.payload.skipped) return;
      eventVersion[event.payload.kind]++; accept(event.payload);
    })
      .then(async cleanup => {
        if (visit.current !== current) { cleanup(); return; }
        unlisten = cleanup;
        const version = { ...eventVersion };
        const stored = await invoke<DocumentExportResult[]>('api_get_document_export_status', { meetingId });
        stored.forEach(result => { if (eventVersion[result.kind] === version[result.kind]) accept(result); });
      })
      .catch(error => { if (visit.current === current) setLoadError(`Cannot load document export status: ${String(error)}`); });
    return () => { visit.current++; unlisten?.(); };
  }, [meetingId]);
  const exportDocument = async (kind: Kind) => {
    const current = visit.current;
    setBusy(kind);
    try {
      const result = await invoke<DocumentExportResult>('api_export_meeting_document', { meetingId, kind });
      if (visit.current === current) setResults(previous => ({ ...previous, [kind]: result }));
    } catch (error) {
      if (visit.current === current) setResults(previous => ({ ...previous, [kind]: {
        meeting_id: meetingId, kind, success: false, skipped: false, path: previous[kind]?.path ?? null, error: String(error),
      } }));
    } finally { if (visit.current === current) setBusy(null); }
  };
  return <div aria-label="Meeting document exports" className="border-b border-gray-200 bg-white px-4 py-2 text-sm">
    <div className="flex flex-wrap gap-2 items-center">
      <span className="text-gray-600">Documents</span>
      {(['transcript', 'summary'] as const).map(kind => <button key={kind} type="button" disabled={busy !== null} onClick={() => void exportDocument(kind)} className="rounded border px-3 py-1 text-gray-700 disabled:opacity-50">
        {busy === kind ? 'Exporting…' : `${results[kind]?.error ? 'Retry' : 'Export'} ${kind === 'transcript' ? 'TXT' : 'Markdown'}`}
      </button>)}
    </div>
    {loadError && <p role="alert" className="mt-1 text-red-700">{loadError}</p>}
    {(['transcript', 'summary'] as const).map(kind => {
      const result = results[kind];
      if (!result) return null;
      return result.error ? <p role="alert" key={kind} className="mt-1 text-red-700">{kind === 'transcript' ? 'Transcript' : 'Summary'} saved in Meetily; document export failed: {result.error}</p>
        : result.path ? <p role="status" key={kind} className="mt-1 break-all text-gray-600">{kind === 'transcript' ? 'TXT' : 'Markdown'}: {result.path}</p> : null;
    })}
  </div>;
}
