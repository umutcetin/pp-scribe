import { invoke } from '@tauri-apps/api/core';
import type {
  ActiveSnapshot,
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
};

export function errorText(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
