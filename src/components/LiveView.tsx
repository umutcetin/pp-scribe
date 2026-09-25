import { useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Circle, Download, Headphones, Loader2, Mic, MonitorSpeaker, Square } from 'lucide-react';
import { api, errorText } from '../lib/api';
import { clock } from '../lib/format';
import type { ModelInfo } from '../lib/types';
import { useApp } from '../stores/appStore';
import { LevelMeter } from './LevelMeter';
import { LiveCopyControls } from './LiveCopyControls';
import { TranscriptView } from './TranscriptView';

function useElapsed(t0: number, running: boolean) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(id);
  }, [running]);
  return running ? now - t0 : 0;
}

export function LiveView() {
  const { recState, active, activeT0, captions, copiedIds, levels, settings } = useApp();
  const beginMeeting = useApp((s) => s.beginMeeting);
  const setRecState = useApp((s) => s.setRecState);
  const upsertMeeting = useApp((s) => s.upsertMeeting);
  const toast = useApp((s) => s.toast);
  const [title, setTitle] = useState('');

  const recording = recState === 'recording' || recState === 'finalizing';
  const elapsed = useElapsed(activeT0, recording);

  useEffect(() => {
    setTitle(active?.title ?? '');
  }, [active?.id]);

  const segments = useMemo(
    () => Object.values(captions).sort((a, b) => a.start_ms - b.start_ms),
    [captions],
  );

  // The marker sits above the first line not yet copied, once anything has been copied.
  const markerBeforeId = Object.keys(copiedIds).length
    ? segments.find((s) => !copiedIds[s.id])?.id
    : undefined;

  const labels = {
    me: active?.me_label ?? settings?.me_label ?? 'Me',
    them: active?.them_label ?? settings?.them_label ?? 'Them',
  };

  const start = async () => {
    setRecState('starting');
    try {
      const meta = await api.startMeeting(title || undefined);
      upsertMeeting(meta);
      beginMeeting(meta, 0, []);
    } catch (e) {
      setRecState('idle');
      toast('error', errorText(e));
    }
  };

  const stop = async () => {
    setRecState('finalizing');
    try {
      await api.stopMeeting();
    } catch (e) {
      toast('error', errorText(e));
      setRecState('recording');
    }
  };

  const commitTitle = async () => {
    if (!active || !title.trim() || title === active.title) return;
    try {
      await api.renameMeeting(active.id, title);
      upsertMeeting({ ...active, title: title.trim() });
    } catch (e) {
      toast('error', errorText(e));
    }
  };

  return (
    <div className="flex flex-col h-full min-w-0">
      <header className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 sm:px-6 py-3 border-b border-surface-800">
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={commitTitle}
          onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
          placeholder={recording ? 'Untitled meeting' : 'Meeting title (optional)'}
          disabled={recState === 'finalizing'}
          className="flex-1 min-w-[180px] bg-transparent text-lg font-semibold text-surface-100 placeholder:text-surface-600 outline-none"
        />

        {recording && <LiveCopyControls segments={segments} labels={labels} />}

        {recording && (
          <div className="flex items-center gap-4">
            <LevelMeter speaker="me" label={labels.me} level={levels.me} enabled={!!active?.me_device} />
            <LevelMeter speaker="them" label={labels.them} level={levels.them} enabled={!!active?.them_device} />
            <span className="font-mono text-sm text-surface-300 tabular-nums">{clock(elapsed)}</span>
          </div>
        )}

        {recState === 'idle' || recState === 'starting' ? (
          <button
            onClick={start}
            disabled={recState === 'starting'}
            className="flex items-center gap-2 px-4 py-2 rounded-full bg-rec-500 hover:bg-rec-600 disabled:opacity-60 text-white text-sm font-medium transition-colors"
          >
            {recState === 'starting' ? <Loader2 className="w-4 h-4 animate-spin" /> : <Circle className="w-3.5 h-3.5 fill-current" />}
            Start meeting
          </button>
        ) : (
          <button
            onClick={stop}
            disabled={recState === 'finalizing'}
            className="flex items-center gap-2 px-4 py-2 rounded-full bg-surface-700 hover:bg-surface-600 disabled:opacity-60 text-white text-sm font-medium transition-colors"
          >
            {recState === 'finalizing' ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" /> Saving…
              </>
            ) : (
              <>
                <span className="w-2 h-2 rounded-full bg-rec-500 rec-pulse" />
                <Square className="w-3.5 h-3.5 fill-current" /> Stop
              </>
            )}
          </button>
        )}
      </header>

      {recording ? (
        <TranscriptView
          segments={segments}
          labels={labels}
          follow
          markerBeforeId={markerBeforeId}
          empty={<span>Listening… captions appear as people speak.</span>}
        />
      ) : (
        <IdleState />
      )}
    </div>
  );
}

/** Models the current settings need but that aren't on disk yet. */
function useMissingModels() {
  const settings = useApp((s) => s.settings);
  const [models, setModels] = useState<ModelInfo[]>([]);

  useEffect(() => {
    const load = () => api.listModels().then(setModels).catch(() => {});
    load();
    const un = listen<{ done: boolean }>('model-download-progress', (e) => e.payload.done && load());
    return () => {
      un.then((f) => f());
    };
  }, [settings?.live_model, settings?.refine_model]);

  const needed = [settings?.live_model, settings?.refine_model];
  return models.filter((m) => needed.includes(m.id) && !m.is_downloaded);
}

function IdleState() {
  const settings = useApp((s) => s.settings);
  const setSettingsOpen = useApp((s) => s.setSettingsOpen);
  const missing = useMissingModels();
  const [downloading, setDownloading] = useState<Record<string, boolean>>({});

  const download = (id: string) => {
    setDownloading((d) => ({ ...d, [id]: true }));
    api.downloadModel(id).catch(() => {}).finally(() => setDownloading((d) => ({ ...d, [id]: false })));
  };

  const themText =
    settings?.them_source === 'off'
      ? 'Off — only your mic'
      : settings?.them_source === 'device'
        ? settings.them_device ?? 'No device selected'
        : 'System audio (whatever the Mac plays)';

  return (
    <div className="flex-1 flex items-center justify-center p-6">
      <div className="max-w-md w-full space-y-5">
        <div>
          <h2 className="text-xl font-semibold text-surface-100">Ready to transcribe</h2>
          <p className="text-sm text-surface-400 mt-1">
            Live captions for both sides while you talk. When you stop, the recording is transcribed again with a
            more accurate model and saved.
          </p>
        </div>

        {missing.map((m) => (
          <div key={m.id} className="flex items-center gap-3 rounded-xl bg-amber-500/10 px-4 py-3 text-sm">
            <p className="flex-1 text-amber-200/90">
              {m.id === settings?.live_model ? 'Live caption' : 'Accurate transcript'} model <b>{m.name}</b> is not
              downloaded yet ({m.size_mb} MB).
            </p>
            <button
              onClick={() => download(m.id)}
              disabled={downloading[m.id] || m.is_downloading}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-surface-700 hover:bg-surface-600 disabled:opacity-60 text-xs text-surface-100 shrink-0"
            >
              {downloading[m.id] || m.is_downloading ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Download className="w-3.5 h-3.5" />
              )}
              {downloading[m.id] || m.is_downloading ? 'Downloading…' : 'Download'}
            </button>
          </div>
        ))}

        <div className="rounded-xl border border-surface-800 divide-y divide-surface-800 text-sm">
          <div className="flex items-center gap-3 px-4 py-3">
            <Mic className="w-4 h-4 text-accent-400 shrink-0" />
            <span className="text-surface-400 w-24 shrink-0">{settings?.me_label ?? 'Me'}</span>
            <span className="text-surface-200 truncate">{settings?.mic_device ?? 'Default microphone'}</span>
          </div>
          <div className="flex items-center gap-3 px-4 py-3">
            <MonitorSpeaker className="w-4 h-4 text-them-400 shrink-0" />
            <span className="text-surface-400 w-24 shrink-0">{settings?.them_label ?? 'Them'}</span>
            <span className="text-surface-200 truncate">{themText}</span>
          </div>
        </div>

        <div className="flex items-start gap-2 text-xs text-surface-500">
          <Headphones className="w-4 h-4 shrink-0" />
          <p>
            Use headphones: with speakers your mic also hears the other side and their words show up twice.{' '}
            <button onClick={() => setSettingsOpen(true)} className="text-surface-300 underline underline-offset-2">
              Change sources
            </button>
          </p>
        </div>
      </div>
    </div>
  );
}
