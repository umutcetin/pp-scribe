import { useCallback, useEffect, useState } from 'react';
import { Check, ClipboardCopy, Copy } from 'lucide-react';
import { api, errorText } from '../lib/api';
import { toPlainText } from '../lib/format';
import type { Caption, Speaker } from '../lib/types';
import { useApp } from '../stores/appStore';

interface Props {
  /** Live captions sorted by start time (interims included). */
  segments: Caption[];
  labels: Record<Speaker, string>;
}

/** "Copy new" copies finished lines not yet copied; the icon copies everything. ⌘⇧C = copy new. */
export function LiveCopyControls({ segments, labels }: Props) {
  const copiedIds = useApp((s) => s.copiedIds);
  const { markCopied, toast } = useApp.getState();
  const [flash, setFlash] = useState<string | null>(null);

  const finals = segments.filter((s) => s.is_final && s.text);
  const fresh = finals.filter((s) => !copiedIds[s.id]);

  const copy = useCallback(
    async (items: Caption[], what: string) => {
      if (items.length === 0) return;
      try {
        await api.copyText(toPlainText(items, labels));
        markCopied(items.map((s) => s.id));
        setFlash(what);
        setTimeout(() => setFlash(null), 1500);
      } catch (e) {
        toast('error', `Copy failed: ${errorText(e)}`);
      }
    },
    [labels.me, labels.them],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.shiftKey && e.key.toLowerCase() === 'c') {
        e.preventDefault();
        copy(fresh, `Copied ${fresh.length}`);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [copy, fresh]);

  if (finals.length === 0) return null;

  return (
    <div className="flex items-center rounded-full border border-surface-700 text-xs text-surface-300">
      <button
        onClick={() => copy(fresh, `Copied ${fresh.length}`)}
        disabled={fresh.length === 0}
        title="Copy lines since your last copy (⌘⇧C)"
        className="flex items-center gap-1.5 pl-3 pr-2.5 py-1.5 rounded-l-full hover:bg-surface-800 hover:text-surface-100 disabled:opacity-50 disabled:hover:bg-transparent"
      >
        {flash ? <Check className="w-3.5 h-3.5 text-accent-400" /> : <ClipboardCopy className="w-3.5 h-3.5" />}
        {flash ?? 'Copy new'}
        {!flash && fresh.length > 0 && (
          <span className="min-w-[18px] px-1 rounded-full bg-surface-700 text-surface-100 text-[10px] text-center tabular-nums">
            {fresh.length}
          </span>
        )}
      </button>
      <div className="w-px h-4 bg-surface-700" />
      <button
        onClick={() => copy(finals, 'Copied all')}
        title="Copy entire transcript"
        className="px-2.5 py-1.5 rounded-r-full hover:bg-surface-800 hover:text-surface-100"
      >
        <Copy className="w-3.5 h-3.5" />
      </button>
    </div>
  );
}
