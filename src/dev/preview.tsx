// Browser-only UI harness: `npm run dev` then open /preview.html.
// Mocks the Tauri backend with canned data so layout can be checked without the native app.
import { emit } from '@tauri-apps/api/event';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type { Caption, MeetingDetail, MeetingMeta, Settings } from '../lib/types';

const now = Date.now();
const iso = (msAgo: number) => new Date(now - msAgo).toISOString();

const settings: Settings = {
  mic_device: null,
  them_source: 'system',
  them_device: null,
  live_model: 'parakeet-v2',
  refine_model: 'whisper-large-v3-turbo',
  language: 'en',
  auto_refine: true,
  me_label: 'Me',
  them_label: 'Them',
};

const meta = (id: string, title: string, agoMs: number, status: MeetingMeta['status']): MeetingMeta => ({
  id, title, started_at: iso(agoMs), ended_at: iso(agoMs - 1_800_000), duration_ms: 1_830_000, status,
  language: 'en', live_model: 'parakeet-v2', refine_model: 'whisper-large-v3-turbo', refine_error: null,
  me_label: 'Me', them_label: 'Them', me_device: 'MacBook Pro Microphone', them_device: 'MacBook Pro Speakers',
});

const meetings = [
  meta('m1', 'Roadmap sync with Platform team', 3_600_000, 'refined'),
  meta('m2', 'Billing migration kickoff', 7_200_000, 'refining'),
  meta('m3', 'Weekly 1:1', 90_000_000, 'recorded'),
];

const lines: [Caption['speaker'], string][] = [
  ['them', 'Hi everyone, thanks for joining. Can you all hear me okay?'],
  ['me', 'Yes, loud and clear.'],
  ['them', 'Great. So today I want to go over the quarterly roadmap and then spend some time on the billing migration.'],
  ['them', 'We are a bit behind on the invoicing service, mostly because of the tax provider integration.'],
  ['me', 'Understood. What is the new estimate for the tax integration?'],
  ['them', 'Probably two more sprints. We need sign-off from finance on the rounding rules first.'],
  ['me', 'Okay, I can set up that meeting with finance this week.'],
];
const segs = lines.map(([speaker, text], i) => ({ id: `${speaker}-${i}`, speaker, start_ms: i * 9000 + 2000, end_ms: i * 9000 + 8000, text }));

mockWindows('main');
mockIPC(
  (cmd, args) => {
    switch (cmd) {
      case 'get_settings': return settings;
      case 'list_meetings': return meetings;
      case 'get_active_meeting': return null;
      case 'meetings_root': return '/Users/you/Documents/PPScribe';
      case 'list_audio_devices': return { inputs: [{ name: 'MacBook Pro Microphone', is_default: true }, { name: 'BlackHole 2ch', is_default: false }], default_output: 'MacBook Pro Speakers' };
      case 'list_models': return [
        { id: 'parakeet-v2', name: 'Parakeet V2', description: 'English only. Very fast — best for live captions.', size_mb: 473, engine: 'parakeet', multilingual: false, is_downloaded: true, is_downloading: false },
        { id: 'whisper-small', name: 'Whisper Small', description: 'Multilingual, fast.', size_mb: 487, engine: 'whisper', multilingual: true, is_downloaded: true, is_downloading: false },
        { id: 'whisper-large-v3-turbo', name: 'Whisper Large v3 Turbo', description: 'Multilingual, most accurate.', size_mb: 874, engine: 'whisper', multilingual: true, is_downloaded: false, is_downloading: false },
      ];
      case 'get_meeting': {
        const m = meetings.find((x) => x.id === (args as { id: string }).id)!;
        const d: MeetingDetail = { meta: m, dir: '/tmp', live: segs, refined: m.status === 'refined' ? segs : null };
        return d;
      }
      case 'start_meeting': {
        const m = meta('live', 'Design review', 0, 'recording');
        setTimeout(() => runLiveDemo(), 300);
        return m;
      }
      default: return null;
    }
  },
  { shouldMockEvents: true },
);

async function runLiveDemo() {
  for (const [i, s] of segs.entries()) {
    const words = s.text.split(' ');
    const half = words.slice(0, Math.ceil(words.length / 2)).join(' ');
    await emit('caption', { ...s, meeting_id: 'live', text: half, is_final: false });
    await new Promise((r) => setTimeout(r, 250));
    await emit('caption', { ...s, meeting_id: 'live', is_final: i < segs.length - 1 });
    await emit('audio-level', { speaker: s.speaker, level: 0.3 });
  }
}

import('../main');
