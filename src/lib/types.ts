export type Speaker = 'me' | 'them';

export interface Segment {
  id: string;
  speaker: Speaker;
  start_ms: number;
  end_ms: number;
  text: string;
}

export interface Caption extends Segment {
  is_final: boolean;
  meeting_id?: string;
}

export type MeetingStatus = 'recording' | 'recorded' | 'refining' | 'refined' | 'refine_failed';

export interface MeetingMeta {
  id: string;
  title: string;
  started_at: string;
  ended_at: string | null;
  duration_ms: number;
  status: MeetingStatus;
  language: string;
  live_model: string;
  refine_model: string | null;
  refine_error: string | null;
  me_label: string;
  them_label: string;
  me_device: string | null;
  them_device: string | null;
}

export interface MeetingDetail {
  meta: MeetingMeta;
  dir: string;
  live: Segment[];
  refined: Segment[] | null;
}

export interface ActiveSnapshot {
  meta: MeetingMeta;
  elapsed_ms: number;
  captions: Segment[];
}

export type ThemSource = 'system' | 'device' | 'off';

export interface Settings {
  mic_device: string | null;
  them_source: ThemSource;
  them_device: string | null;
  live_model: string;
  refine_model: string;
  language: string;
  auto_refine: boolean;
  me_label: string;
  them_label: string;
}

export interface ModelInfo {
  id: string;
  name: string;
  description: string;
  size_mb: number;
  engine: 'whisper' | 'parakeet';
  multilingual: boolean;
  is_downloaded: boolean;
  is_downloading: boolean;
}

export interface AudioDevices {
  inputs: { name: string; is_default: boolean }[];
  default_output: string | null;
}

export interface RefineProgress {
  meeting_id: string;
  done: number;
  total: number;
  stage: string;
}

export interface DownloadProgress {
  model_id: string;
  downloaded: number;
  total: number;
  done: boolean;
  error: string | null;
}
