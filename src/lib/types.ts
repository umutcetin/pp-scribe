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
  engine: 'whisper' | 'parakeet' | 'embedding';
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

// ---------------------------------------------------------------- knowledge

export type KbKind = 'pdf' | 'docx' | 'md' | 'txt' | 'note' | 'url' | 'meeting';
export type KbSourceStatus = 'pending' | 'indexing' | 'ready' | 'error' | 'excluded';

export interface KbLocator {
  page?: number;
  page_end?: number;
  heading?: string;
  start_ms?: number;
  end_ms?: number;
}

export interface KbSource {
  id: number;
  kind: KbKind;
  title: string;
  origin: string | null;
  file_rel: string | null;
  added_at: string;
  status: KbSourceStatus;
  error: string | null;
  chunk_count: number;
  char_count: number;
}

export interface KbChunk {
  id: number;
  source_id: number;
  ord: number;
  text: string;
  locator: KbLocator;
}

export interface KbSourceDetail {
  source: KbSource;
  chunks: KbChunk[];
  file_path: string | null;
}

export interface KbHit {
  chunk_id: number;
  source_id: number;
  source_title: string;
  source_kind: KbKind;
  origin: string | null;
  text: string;
  locator: KbLocator;
  score: number;
  keyword: boolean;
  semantic: number | null;
}

export interface KbStatus {
  sources: number;
  indexing: number;
  semantic_ready: boolean;
  missing_embeddings: number;
  root: string;
}

export interface KbAddResult {
  source: KbSource;
  duplicate: boolean;
}
