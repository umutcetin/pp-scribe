// Browser-only UI harness: `npm run dev` then open /preview.html.
// Mocks the Tauri backend with canned data so layout can be checked without the native app.
import { emit } from '@tauri-apps/api/event';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type { Caption, KbHit, KbSource, MeetingDetail, MeetingMeta, Settings } from '../lib/types';

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

const src = (id: number, kind: KbSource['kind'], title: string, agoMs: number, extra: Partial<KbSource> = {}): KbSource => ({
  id, kind, title, origin: kind === 'meeting' ? 'm1' : kind === 'url' ? 'https://en.wikipedia.org/wiki/Retrieval-augmented_generation' : null,
  file_rel: kind === 'url' ? null : `files/${id}.${kind}`, added_at: iso(agoMs), status: 'ready', error: null,
  chunk_count: 12, char_count: 14200, ...extra,
});
const kbSources: KbSource[] = [
  src(6, 'pdf', 'Scanned contract', 60_000, { status: 'indexing', chunk_count: 0 }),
  src(5, 'note', 'Onboarding checklist for new engineers', 3_600_000, { chunk_count: 2 }),
  src(4, 'url', 'Retrieval-augmented generation', 7_200_000, { chunk_count: 19 }),
  src(3, 'meeting', 'Roadmap sync with Platform team', 7_300_000, { chunk_count: 41 }),
  src(2, 'docx', 'Billing Migration Plan', 90_000_000, { chunk_count: 2 }),
  src(1, 'pdf', '2027 Bütçe Planı', 95_000_000, { chunk_count: 3 }),
  src(7, 'pdf', 'Old scan.pdf', 99_000_000, { status: 'error', error: 'No text layer in this PDF (4 pages) — scanned PDFs need OCR', chunk_count: 0 }),
];
const kbHits: KbHit[] = [
  { chunk_id: 11, source_id: 1, source_title: '2027 Bütçe Planı', source_kind: 'pdf', origin: null, locator: { page: 1, page_end: 2 }, score: 0.02, keyword: false, semantic: 0.52,
    text: '2027 Bütçe Planı. Altyapı maliyetlerini yüzde yirmi azaltmayı hedefliyoruz. Bulut sunucu faturaları en büyük gider kalemi ve rezerve kapasiteye geçiş planlanıyor.' },
  { chunk_id: 21, source_id: 3, source_title: 'Roadmap sync with Platform team', source_kind: 'meeting', origin: 'm1', locator: { start_ms: 20000, end_ms: 50000 }, score: 0.018, keyword: true, semantic: 0.45,
    text: '[00:20] Them: Great. So today I want to go over the quarterly roadmap and the infrastructure cost targets, then spend some time on the billing migration.' },
  { chunk_id: 31, source_id: 2, source_title: 'Billing Migration Plan', source_kind: 'docx', origin: null, locator: { heading: 'Billing Migration Plan › Risks' }, score: 0.015, keyword: true, semantic: 0.41,
    text: 'Currency conversion differences between the old and new systems can cause reconciliation errors, which increases infrastructure cost. We will run both systems in parallel for two billing cycles.' },
];

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
      case 'kb_status': return { sources: kbSources.length, indexing: 1, semantic_ready: false, missing_embeddings: 0, root: '/Users/you/Documents/PPScribe/Knowledge' };
      case 'kb_list_sources': return kbSources;
      case 'kb_search': return new Promise((r) => setTimeout(() => r(kbHits), 150));
      case 'kb_get_source': {
        const id = (args as { id: number }).id;
        const source = kbSources.find((x) => x.id === id)!;
        return { source, file_path: '/tmp/x.pdf', chunks: kbHits.filter((h) => h.source_id === id).map((h, i) => ({ id: h.chunk_id, source_id: id, ord: i, text: h.text, locator: h.locator })) };
      }
      case 'kb_add_text': return { source: src(99, 'note', (args as { title: string }).title || 'Pasted note', 0, { status: 'pending', chunk_count: 0 }), duplicate: false };
      case 'kb_add_url': return { source: src(98, 'url', (args as { url: string }).url, 0, { status: 'pending', chunk_count: 0 }), duplicate: false };
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
