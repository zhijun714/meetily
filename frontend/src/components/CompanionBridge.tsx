'use client';

import { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { RecordingStatus, useRecordingState } from '@/contexts/RecordingStateContext';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { checkTranscriptionReady } from '@/lib/transcription-model-readiness';

type CompanionCommand = { operation_id: string; action: 'start' | 'stop' };

// One global listener, independent of navigation. Deliberately does not mount
// useRecordingStart: that hook also registers sidebar and auto-start listeners.
export function CompanionBridge({ enabled }: { enabled: boolean }) {
  const recording = useRecordingState();
  const { clearTranscripts } = useTranscripts();
  const { setIsMeetingActive } = useSidebar();
  const latest = useRef({ enabled, recording, clearTranscripts, setIsMeetingActive });
  latest.current = { enabled, recording, clearTranscripts, setIsMeetingActive };
  const reportNow = useRef<(() => Promise<void>) | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenStarting: (() => void) | undefined;
    let timer: ReturnType<typeof setInterval> | undefined;
    let busy = false;
    const handled = new Set<string>();

    let reports = Promise.resolve();
    const report = () => {
      const { recording: current, enabled: ready } = latest.current;
      // Capture every transition and preserve IPC order: a delayed heartbeat
      // must not overwrite COMPLETED or make an old IDLE confirm a new stop.
      const snapshot = {
        phase: current.status,
        ready: ready && !disposed,
        error: current.status === RecordingStatus.ERROR ? current.statusMessage || 'Recording failed' : null,
      };
      reports = reports.catch(console.error).then(() => invoke<void>('companion_report', { report: snapshot }));
      return reports;
    };

    const setup = async () => {
      // Emitted only by the winning native start, after its lifecycle lock and
      // duplicate/model checks. A rejected pet request must never clear a live session.
      const startingCleanup = await listen('recording-starting', () => {
        if (!disposed) latest.current.clearTranscripts();
      });
      if (disposed) { startingCleanup(); return; }
      unlistenStarting = startingCleanup;
      const cleanup = await listen<CompanionCommand>('companion-command', async ({ payload }) => {
        if (disposed || handled.has(payload.operation_id)) return;
        handled.add(payload.operation_id);
        if (handled.size > 128) handled.delete(handled.values().next().value!);
        try {
          if (busy) throw new Error('Another companion command is running');
          if (!latest.current.enabled) throw new Error('Complete Meetily setup before recording');
          busy = true;
          if (payload.action === 'start') {
            const config = await invoke<{ provider?: string } | null>('api_get_transcript_config');
            if (!await checkTranscriptionReady(config?.provider || 'parakeet', invoke)) {
              throw new Error('Set up a transcription model or custom server in Meetily settings before recording');
            }
            // Readiness checks await IPC. Another entry may start while they run;
            // never erase transcripts belonging to that live session.
            const native = await invoke<{ is_recording: boolean }>('get_recording_state');
            if (native.is_recording || latest.current.recording.isRecording ||
                ![RecordingStatus.IDLE, RecordingStatus.ERROR].includes(latest.current.recording.status)) {
              throw new Error('Meetily recording state changed; check the main panel');
            }
            const title = `Meeting ${new Date().toLocaleString().replace(/[/,:]/g, '_')}`;
            latest.current.recording.setStatus(RecordingStatus.STARTING);
            await invoke('companion_execute', { operationId: payload.operation_id, meetingName: title });
            latest.current.setIsMeetingActive(true);
          } else {
            if ([RecordingStatus.STOPPING, RecordingStatus.PROCESSING_TRANSCRIPTS,
              RecordingStatus.SAVING, RecordingStatus.COMPLETED].includes(latest.current.recording.status)) {
              throw new Error('Meetily is already finishing this recording');
            }
            latest.current.recording.setStatus(RecordingStatus.STOPPING);
            await invoke('companion_execute', { operationId: payload.operation_id, meetingName: null });
            // The existing global post-processing provider owns flush, save,
            // sidebar refresh, meeting-active reset, and navigation.
          }
        } catch (error) {
          const message = error instanceof Error ? error.message : String(error);
          // A rejected racing command must not overwrite another entry's live
          // recording or finishing phase; the operation itself carries the error.
          if ([RecordingStatus.IDLE, RecordingStatus.STARTING, RecordingStatus.ERROR].includes(latest.current.recording.status)
              && !latest.current.recording.isRecording) {
            latest.current.recording.setStatus(RecordingStatus.ERROR, message);
          }
          await invoke('companion_fail', { operationId: payload.operation_id, error: message }).catch(console.error);
        } finally {
          busy = false;
          await report().catch(console.error);
        }
      });
      if (disposed) { cleanup(); return; }
      unlisten = cleanup;
      reportNow.current = report;
      await report();
      if (!disposed) timer = setInterval(() => { void report().catch(console.error); }, 2000);
    };
    void setup().catch(error => console.error('[CompanionBridge] Setup failed', error));
    return () => {
      disposed = true;
      reportNow.current = null;
      unlisten?.();
      unlistenStarting?.();
      if (timer) clearInterval(timer);
      // Heartbeat expiry handles reload/crash without a fragile async cleanup IPC.
    };
  }, []);

  useEffect(() => {
    void reportNow.current?.().catch(console.error);
  }, [enabled, recording.status, recording.statusMessage]);

  return null;
}
