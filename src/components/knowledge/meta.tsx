import { FileText, FileType2, Globe, Mic, NotebookPen, ScrollText } from 'lucide-react';
import { clock } from '../../lib/format';
import type { KbKind, KbLocator } from '../../lib/types';

const ICONS: Record<KbKind, typeof FileText> = {
  pdf: FileType2,
  docx: FileText,
  md: ScrollText,
  txt: ScrollText,
  note: NotebookPen,
  url: Globe,
  meeting: Mic,
};

export const KIND_LABEL: Record<KbKind, string> = {
  pdf: 'PDF',
  docx: 'Word',
  md: 'Markdown',
  txt: 'Text',
  note: 'Note',
  url: 'Web page',
  meeting: 'Meeting',
};

export function KindIcon({ kind, className = 'w-4 h-4' }: { kind: KbKind; className?: string }) {
  const Icon = ICONS[kind] ?? FileText;
  const color = kind === 'meeting' ? 'text-them-400' : kind === 'url' ? 'text-sky-300' : 'text-surface-400';
  return <Icon className={`${className} ${color} shrink-0`} />;
}

/** "p. 3–4", "12:30", or the heading path — whatever says where in the source this is. */
export function locatorLabel(l: KbLocator): string | null {
  if (l.start_ms !== undefined) return clock(l.start_ms);
  if (l.page !== undefined) return l.page_end ? `p. ${l.page}–${l.page_end}` : `p. ${l.page}`;
  return l.heading ?? null;
}
