import { invoke } from '@tauri-apps/api/core';
import type {
  ActiveSnapshot,
  KbAddResult,
  KbHit,
  KbSource,
  KbSourceDetail,
  KbStatus,
  AudioDevices,
  MeetingDetail,
  MeetingMeta,
  ModelInfo,
  Settings,
} from './types';

export const api = {
  getSettings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),
  listAudioDevices: () => invoke<AudioDevices>('list_audio_devices'),
  listModels: () => invoke<ModelInfo[]>('list_models'),
  downloadModel: (modelId: string) => invoke<void>('download_model', { modelId }),

  startMeeting: (title?: string) => invoke<MeetingMeta>('start_meeting', { title }),
  stopMeeting: () => invoke<MeetingMeta>('stop_meeting'),
  getActiveMeeting: () => invoke<ActiveSnapshot | null>('get_active_meeting'),

  listMeetings: () => invoke<MeetingMeta[]>('list_meetings'),
  getMeeting: (id: string) => invoke<MeetingDetail>('get_meeting', { id }),
  renameMeeting: (id: string, title: string) => invoke<void>('rename_meeting', { id, title }),
  deleteMeeting: (id: string) => invoke<void>('delete_meeting', { id }),
  refineMeeting: (id: string) => invoke<void>('refine_meeting', { id }),
  exportMeeting: (id: string) => invoke<string>('export_meeting', { id }),
  meetingsRoot: () => invoke<string>('meetings_root'),
  copyText: (text: string) => invoke<void>('copy_text', { text }),

  kbStatus: () => invoke<KbStatus>('kb_status'),
  kbListSources: () => invoke<KbSource[]>('kb_list_sources'),
  kbGetSource: (id: number) => invoke<KbSourceDetail>('kb_get_source', { id }),
  /** One result per path: `{ Ok: KbAddResult } | { Err: string }`. */
  kbAddFiles: (paths: string[]) =>
    invoke<({ Ok: KbAddResult } | { Err: string })[]>('kb_add_files', { paths }),
  kbAddText: (title: string, text: string) => invoke<KbAddResult>('kb_add_text', { title, text }),
  kbAddUrl: (url: string) => invoke<KbAddResult>('kb_add_url', { url }),
  kbDeleteSource: (id: number) => invoke<void>('kb_delete_source', { id }),
  kbReindexSource: (id: number) => invoke<void>('kb_reindex_source', { id }),
  kbBackfill: () => invoke<void>('kb_backfill'),
  kbSearch: (query: string, filter?: { kinds?: string[]; source_id?: number }, limit?: number) =>
    invoke<KbHit[]>('kb_search', { query, filter, limit }),
};

export function errorText(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
