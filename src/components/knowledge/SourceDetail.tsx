import { useEffect, useRef, useState } from 'react';
import { openPath, openUrl, revealItemInDir } from '@tauri-apps/plugin-opener';
import { AlertCircle, ArrowLeft, ExternalLink, FolderOpen, Loader2, RefreshCw, Trash2 } from 'lucide-react';
import { api, errorText } from '../../lib/api';
import { dayLabel, timeOfDay } from '../../lib/format';
import { highlight, queryTerms } from '../../lib/highlight';
import type { KbSourceDetail } from '../../lib/types';
import { useApp } from '../../stores/appStore';
import { useKnowledge } from '../../stores/knowledgeStore';
import { KIND_LABEL, KindIcon, locatorLabel } from './meta';

export function SourceDetail({ id }: { id: number }) {
  const { focusChunkId, query } = useKnowledge();
  const live = useKnowledge((s) => s.sources.find((x) => x.id === id));
  const { openSource, removeSource } = useKnowledge.getState();
  const toast = useApp((s) => s.toast);

  const [detail, setDetail] = useState<KbSourceDetail | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const terms = queryTerms(query);

  // Reload when indexing finishes (status/chunk count change).
  useEffect(() => {
    api
      .kbGetSource(id)
      .then(setDetail)
      .catch((e) => toast('error', errorText(e)));
  }, [id, live?.status, live?.chunk_count]);

  useEffect(() => {
    if (!detail || focusChunkId == null) return;
    listRef.current?.querySelector(`[data-chunk="${focusChunkId}"]`)?.scrollIntoView({ block: 'center' });
  }, [detail, focusChunkId]);

  if (!detail) {
    return (
      <div className="h-full flex items-center justify-center text-surface-500">
        <Loader2 className="w-5 h-5 animate-spin" />
      </div>
    );
  }

  const s = live ?? detail.source;
  const indexing = s.status === 'pending' || s.status === 'indexing';

  const run = async (f: () => Promise<unknown>) => {
    try {
      await f();
    } catch (e) {
      toast('error', errorText(e));
    }
  };

  const doDelete = () =>
    run(async () => {
      await api.kbDeleteSource(id);
      removeSource(id);
      toast('info', s.kind === 'meeting' ? 'Removed from knowledge base' : 'Moved to Trash');
    });

  const iconBtn =
    'p-2 rounded-lg text-surface-400 hover:text-surface-100 hover:bg-surface-800 transition-colors disabled:opacity-40';

  return (
    <div className="h-full flex flex-col">
      <div className="px-4 sm:px-6 py-3 border-b border-surface-800">
        <div className="max-w-3xl mx-auto flex items-start gap-2">
          <button onClick={() => openSource(null)} className={iconBtn} title="Back">
            <ArrowLeft className="w-4 h-4" />
          </button>
          <div className="flex-1 min-w-0 pt-1">
            <div className="flex items-center gap-2">
              <KindIcon kind={s.kind} />
              <h2 className="text-base font-semibold text-surface-100 truncate">{s.title}</h2>
            </div>
            <p className="text-xs text-surface-500 truncate mt-0.5">
              {KIND_LABEL[s.kind]} · added {dayLabel(s.added_at)} {timeOfDay(s.added_at)}
              {s.status === 'ready' && ` · ${s.chunk_count} passages · ${s.char_count.toLocaleString()} characters`}
              {s.kind === 'url' && s.origin && ` · ${s.origin}`}
            </p>
          </div>
          <div className="flex items-center gap-0.5 shrink-0">
            {s.kind === 'url' && s.origin && (
              <button onClick={() => run(() => openUrl(s.origin!))} className={iconBtn} title="Open web page">
                <ExternalLink className="w-4 h-4" />
              </button>
            )}
            {detail.file_path && s.kind !== 'url' && (
              <button onClick={() => run(() => openPath(detail.file_path!))} className={iconBtn} title="Open original">
                <ExternalLink className="w-4 h-4" />
              </button>
            )}
            {detail.file_path && (
              <button onClick={() => run(() => revealItemInDir(detail.file_path!))} className={iconBtn} title="Show in Finder">
                <FolderOpen className="w-4 h-4" />
              </button>
            )}
            <button onClick={() => run(() => api.kbReindexSource(id))} disabled={indexing} className={iconBtn} title="Index again">
              <RefreshCw className={`w-4 h-4 ${indexing ? 'animate-spin' : ''}`} />
            </button>
            {confirmDelete ? (
              <button
                onClick={doDelete}
                onBlur={() => setConfirmDelete(false)}
                autoFocus
                className="ml-1 px-2.5 py-1.5 rounded-lg bg-rec-600 hover:bg-rec-500 text-white text-xs font-medium"
              >
                {s.kind === 'meeting' ? 'Remove from search?' : 'Move to Trash?'}
              </button>
            ) : (
              <button onClick={() => setConfirmDelete(true)} className={iconBtn} title="Delete">
                <Trash2 className="w-4 h-4" />
              </button>
            )}
          </div>
        </div>
      </div>

      <div ref={listRef} className="flex-1 overflow-y-auto px-4 sm:px-6 py-4">
        <div className="max-w-3xl mx-auto space-y-3">
          {s.status === 'error' && (
            <div className="flex items-start gap-2 rounded-lg bg-rec-500/10 px-3 py-2 text-sm text-rec-400">
              <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" /> {s.error}
            </div>
          )}
          {indexing && (
            <p className="flex items-center gap-2 text-sm text-surface-400">
              <Loader2 className="w-4 h-4 animate-spin" /> Reading and indexing…
            </p>
          )}
          {detail.chunks.map((c) => {
            const where = locatorLabel(c.locator);
            const focused = c.id === focusChunkId;
            return (
              <div
                key={c.id}
                data-chunk={c.id}
                className={`rounded-lg px-4 py-3 ${focused ? 'bg-amber-400/10 ring-1 ring-amber-400/30' : 'bg-surface-850'}`}
              >
                {where && <p className="text-[11px] font-mono text-surface-500 mb-1">{where}</p>}
                <p className="text-sm leading-relaxed text-surface-200 whitespace-pre-wrap select-text">
                  {focused ? highlight(c.text, terms) : c.text}
                </p>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
