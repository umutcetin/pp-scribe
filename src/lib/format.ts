import type { Segment, Speaker } from './types';

export function clock(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const mm = String(m).padStart(2, '0');
  const ss = String(sec).padStart(2, '0');
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function duration(ms: number): string {
  const min = Math.round(ms / 60000);
  if (min < 1) return '<1 min';
  if (min < 60) return `${min} min`;
  return `${Math.floor(min / 60)} h ${min % 60} min`;
}

export function dayLabel(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (d.toDateString() === today.toDateString()) return 'Today';
  if (d.toDateString() === yesterday.toDateString()) return 'Yesterday';
  return d.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric', year: d.getFullYear() === today.getFullYear() ? undefined : 'numeric' });
}

export function timeOfDay(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

export interface Turn<T extends Segment = Segment> {
  key: string;
  speaker: Speaker;
  start_ms: number;
  items: T[];
}

/** Consecutive segments from the same speaker become one turn. `breakBeforeId` forces a
 *  new turn at that segment (used for the "copied up to here" marker). */
export function toTurns<T extends Segment>(segments: T[], breakBeforeId?: string): Turn<T>[] {
  const turns: Turn<T>[] = [];
  for (const s of segments) {
    const last = turns[turns.length - 1];
    if (last && last.speaker === s.speaker && s.id !== breakBeforeId) {
      last.items.push(s);
    } else {
      turns.push({ key: s.id, speaker: s.speaker, start_ms: s.start_ms, items: [s] });
    }
  }
  return turns;
}

export function toPlainText(segments: Segment[], labels: Record<Speaker, string>): string {
  return toTurns(segments)
    .map((t) => `[${clock(t.start_ms)}] ${labels[t.speaker]}: ${t.items.map((i) => i.text).join(' ')}`)
    .join('\n\n');
}
