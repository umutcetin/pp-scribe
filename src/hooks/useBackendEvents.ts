import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api } from '../lib/api';
import type { Caption, MeetingMeta, RefineProgress, Speaker } from '../lib/types';
import { useApp } from '../stores/appStore';

/** Wires backend events into the store and restores state after a reload. */
export function useBackendEvents() {
  useEffect(() => {
    const s = useApp.getState();

    api.getSettings().then(s.setSettings).catch(() => {});
    api.listMeetings().then(s.setMeetings).catch(() => {});
    api.getActiveMeeting().then((snap) => {
      if (snap) {
        s.beginMeeting(
          snap.meta,
          snap.elapsed_ms,
          snap.captions.map((c): Caption => ({ ...c, is_final: true, meeting_id: snap.meta.id })),
        );
      }
    });

    const unlisten = Promise.all([
      listen<Caption>('caption', (e) => useApp.getState().applyCaption(e.payload)),
      listen<{ speaker: Speaker; level: number }>('audio-level', (e) =>
        useApp.getState().setLevel(e.payload.speaker, e.payload.level),
      ),
      listen<{ state: string; meta: MeetingMeta | null }>('meeting-state', (e) => {
        const st = useApp.getState();
        if (e.payload.state === 'finalizing') st.setRecState('finalizing');
        if (e.payload.meta) st.upsertMeeting(e.payload.meta);
        if (e.payload.state === 'stopped') {
          st.endMeeting();
          if (e.payload.meta) st.select(e.payload.meta.id);
        }
      }),
      listen<string>('meeting-warning', (e) => useApp.getState().toast('warn', e.payload)),
      listen<MeetingMeta>('meeting-updated', (e) => {
        const st = useApp.getState();
        st.upsertMeeting(e.payload);
        if (e.payload.status !== 'refining' && st.refine?.meeting_id === e.payload.id) {
          st.setRefine(null);
        }
        if (e.payload.status === 'refined') st.toast('info', `Accurate transcript ready: ${e.payload.title}`);
        if (e.payload.status === 'refine_failed') st.toast('error', `Refine failed: ${e.payload.refine_error ?? 'unknown error'}`);
      }),
      listen<RefineProgress>('refine-progress', (e) => useApp.getState().setRefine(e.payload)),
    ]);

    return () => {
      unlisten.then((fns) => fns.forEach((f) => f()));
    };
  }, []);
}
