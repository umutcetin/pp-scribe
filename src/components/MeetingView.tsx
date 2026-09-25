import { useEffect, useState } from 'react';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { Check, Copy, FileDown, FolderOpen, Loader2, RefreshCw, Search, Sparkles, Trash2 } from 'lucide-react';
import { api, errorText } from '../lib/api';
import { dayLabel, duration, timeOfDay, toPlainText } from '../lib/format';
import type { MeetingDetail } from '../lib/types';
import { useApp } from '../stores/appStore';
import { TranscriptView } from './TranscriptView';

type Tab = 'accurate' | 'live';

export function MeetingView({ id }: { id: string }) {
  const meta = useApp((s) => s.meetings.find((m) => m.id === id));
  const refine = useApp((s) => (s.refine?.meeting_id === id ? s.refine : null));
  const { toast, upsertMeeting, removeMeeting } = useApp.getState();

  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [tab, setTab] = useState<Tab>('accurate');
  const [title, setTitle] = useState('');
  const [query, setQuery] = useState('');
  const [copied, setCopied] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  // Reload whenever the meeting's status changes (e.g. refine finished).
  useEffect(() => {
    let alive = true;
    api
      .getMeeting(id)
      .then((d) => {
        if (!alive) return;
        setDetail(d);
        setTitle(d.meta.title);
        setTab(d.refined ? 'accurate' : 'live');
      })
      .catch((e) => toast('error', errorText(e)));
    return () => {
      alive = false;
    };
  }, [id, meta?.status]);

  useEffect(() => {
    setConfirmDelete(false);
    setQuery('');
  }, [id]);

  if (!detail || !meta) {
    return (
      <div className="h-full flex items-center justify-center text-surface-500">
        <Loader2 className="w-5 h-5 animate-spin" />
      </div>
    );
  }

  const labels = { me: meta.me_label, them: meta.them_label };
  const segments = tab === 'accurate' ? detail.refined ?? [] : detail.live;

  const commitTitle = async () => {
    const t = title.trim();
    if (!t || t === meta.title) return setTitle(meta.title);
    try {
      await api.renameMeeting(id, t);
      upsertMeeting({ ...meta, title: t });
    } catch (e) {
      toast('error', errorText(e));
    }
  };

  const copy = async () => {
    await navigator.clipboard.writeText(`${meta.title}\n\n${toPlainText(segments, labels)}`);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const exportMd = async () => {
    try {
      const path = await api.exportMeeting(id);
      await revealItemInDir(path);
    } catch (e) {
      toast('error', errorText(e));
    }
  };

  const runRefine = async () => {
    try {
      await api.refineMeeting(id);
    } catch (e) {
      toast('error', errorText(e));
    }
  };

  const doDelete = async () => {
    try {
      await api.deleteMeeting(id);
      removeMeeting(id);
      toast('info', 'Meeting moved to Trash');
    } catch (e) {
      toast('error', errorText(e));
    }
  };

  const iconBtn =
    'p-2 rounded-lg text-surface-400 hover:text-surface-100 hover:bg-surface-800 transition-colors disabled:opacity-40';

  return (
    <div className="flex flex-col h-full min-w-0">
      <header className="px-4 sm:px-6 pt-4 pb-3 border-b border-surface-800 space-y-3">
        <div className="flex items-start gap-3">
          <div className="flex-1 min-w-0">
            <input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              onBlur={commitTitle}
              onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
              className="w-full bg-transparent text-lg font-semibold text-surface-100 outline-none"
            />
            <p className="text-xs text-surface-500">
              {dayLabel(meta.started_at)} · {timeOfDay(meta.started_at)} · {duration(meta.duration_ms)}
            </p>
          </div>

          <div className="flex items-center gap-0.5 shrink-0">
            <button onClick={copy} className={iconBtn} title="Copy transcript" disabled={segments.length === 0}>
              {copied ? <Check className="w-4 h-4 text-accent-400" /> : <Copy className="w-4 h-4" />}
            </button>
            <button onClick={exportMd} className={iconBtn} title="Export Markdown and show in Finder">
              <FileDown className="w-4 h-4" />
            </button>
            <button onClick={() => revealItemInDir(`${detail.dir}/meta.json`)} className={iconBtn} title="Show recordings in Finder">
              <FolderOpen className="w-4 h-4" />
            </button>
            {confirmDelete ? (
              <button
                onClick={doDelete}
                onBlur={() => setConfirmDelete(false)}
                autoFocus
                className="ml-1 px-2.5 py-1.5 rounded-lg bg-rec-600 hover:bg-rec-500 text-white text-xs font-medium"
              >
                Move to Trash?
              </button>
            ) : (
              <button onClick={() => setConfirmDelete(true)} className={iconBtn} title="Delete meeting">
                <Trash2 className="w-4 h-4" />
              </button>
            )}
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="flex p-0.5 rounded-lg bg-surface-800 text-sm">
            {(['accurate', 'live'] as Tab[]).map((t) => (
              <button
                key={t}
                onClick={() => setTab(t)}
                className={`px-3 py-1 rounded-md transition-colors ${
                  tab === t ? 'bg-surface-600 text-surface-50' : 'text-surface-400 hover:text-surface-200'
                }`}
              >
                {t === 'accurate' ? 'Accurate' : 'Live captions'}
              </button>
            ))}
          </div>

          <div className="flex items-center gap-1.5 flex-1 min-w-[140px] max-w-xs ml-auto px-2.5 py-1.5 rounded-lg bg-surface-800">
            <Search className="w-3.5 h-3.5 text-surface-500" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search transcript"
              className="flex-1 min-w-0 bg-transparent text-sm outline-none placeholder:text-surface-500"
            />
          </div>
        </div>
      </header>

      {tab === 'accurate' && !detail.refined ? (
        <RefineState status={meta.status} error={meta.refine_error} progress={refine} onRefine={runRefine} />
      ) : (
        <>
          {tab === 'accurate' && meta.status !== 'refining' && (
            <div className="flex justify-end px-4 sm:px-6 pt-2 -mb-4">
              <button onClick={runRefine} className="flex items-center gap-1 text-xs text-surface-500 hover:text-surface-300">
                <RefreshCw className="w-3 h-3" /> Re-run
              </button>
            </div>
          )}
          <TranscriptView
            segments={segments}
            labels={labels}
            query={query}
            empty={<span>{tab === 'live' ? 'No live captions were saved.' : 'No speech found in the recording.'}</span>}
          />
        </>
      )}
    </div>
  );
}

function RefineState({
  status,
  error,
  progress,
  onRefine,
}: {
  status: string;
  error: string | null;
  progress: { done: number; total: number; stage: string } | null;
  onRefine: () => void;
}) {
  if (status === 'refining') {
    const pct = progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : null;
    return (
      <div className="flex-1 flex items-center justify-center p-6">
        <div className="w-full max-w-sm space-y-3 text-center">
          <Loader2 className="w-6 h-6 animate-spin mx-auto text-accent-400" />
          <p className="text-sm text-surface-300">
            {progress?.stage ?? 'Starting'}
            {pct !== null && ` · ${progress!.done}/${progress!.total}`}
          </p>
          <div className="h-1.5 rounded-full bg-surface-800 overflow-hidden">
            <div className="h-full bg-accent-500 transition-all" style={{ width: `${pct ?? 3}%` }} />
          </div>
          <p className="text-xs text-surface-500">Live captions are available in the other tab meanwhile.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex items-center justify-center p-6">
      <div className="max-w-sm text-center space-y-3">
        <p className="text-sm text-surface-300">
          {status === 'refine_failed'
            ? `Refining failed: ${error ?? 'unknown error'}`
            : 'Transcribe the recording with the accurate model for a cleaner transcript.'}
        </p>
        <button
          onClick={onRefine}
          className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-accent-600 hover:bg-accent-500 text-white text-sm font-medium"
        >
          <Sparkles className="w-4 h-4" />
          {status === 'refine_failed' ? 'Try again' : 'Create accurate transcript'}
        </button>
      </div>
    </div>
  );
}
