import { useEffect, useState, type ReactNode } from 'react';
import { listen } from '@tauri-apps/api/event';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { AlertTriangle, CheckCircle2, Download, FolderOpen, Loader2, X } from 'lucide-react';
import { api, errorText } from '../lib/api';
import type { AudioDevices, DownloadProgress, ModelInfo, Settings } from '../lib/types';
import { useApp } from '../stores/appStore';

const LANGUAGES: [string, string][] = [
  ['en', 'English'],
  ['tr', 'Turkish'],
  ['de', 'German'],
  ['nl', 'Dutch'],
  ['fr', 'French'],
  ['es', 'Spanish'],
  ['it', 'Italian'],
  ['pt', 'Portuguese'],
  ['auto', 'Auto-detect'],
];

const field = 'w-full px-3 py-2 rounded-lg bg-surface-800 border border-surface-700 text-sm text-surface-100 outline-none focus:border-surface-500';

function Row({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <label className="block space-y-1.5">
      <span className="text-xs font-medium text-surface-400">{label}</span>
      {children}
      {hint && <span className="block text-xs text-surface-500">{hint}</span>}
    </label>
  );
}

export function SettingsPanel() {
  const open = useApp((s) => s.settingsOpen);
  const settings = useApp((s) => s.settings);
  const recording = useApp((s) => s.recState !== 'idle');
  const { setSettingsOpen, setSettings, toast } = useApp.getState();

  const [devices, setDevices] = useState<AudioDevices | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [root, setRoot] = useState('');

  const loadModels = () => api.listModels().then(setModels).catch(() => {});

  useEffect(() => {
    if (!open) return;
    api.listAudioDevices().then(setDevices).catch((e) => toast('error', errorText(e)));
    api.meetingsRoot().then(setRoot).catch(() => {});
    loadModels();
  }, [open]);

  useEffect(() => {
    const un = listen<DownloadProgress>('model-download-progress', (e) => {
      setProgress((p) => ({ ...p, [e.payload.model_id]: e.payload }));
      if (e.payload.done) {
        loadModels();
        if (e.payload.error) toast('error', `Download failed: ${e.payload.error}`);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  if (!open || !settings) return null;

  const update = async (patch: Partial<Settings>) => {
    const next = { ...settings, ...patch };
    setSettings(next);
    try {
      await api.saveSettings(next);
    } catch (e) {
      toast('error', errorText(e));
    }
  };

  const download = async (id: string) => {
    setModels((ms) => ms.map((m) => (m.id === id ? { ...m, is_downloading: true } : m)));
    api.downloadModel(id).catch(() => {}); // errors arrive via the progress event
  };

  const sttModels = models.filter((m) => m.engine !== 'embedding');
  const liveModel = models.find((m) => m.id === settings.live_model);
  const englishOnlyMismatch = liveModel && !liveModel.multilingual && settings.language !== 'en';

  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <div className="absolute inset-0 bg-black/40" onClick={() => setSettingsOpen(false)} />
      <div className="relative w-full max-w-md h-full overflow-y-auto bg-surface-900 border-l border-surface-800 shadow-2xl">
        <div className="sticky top-0 flex items-center justify-between px-5 py-4 bg-surface-900 border-b border-surface-800">
          <h2 className="text-base font-semibold text-surface-100">Settings</h2>
          <button onClick={() => setSettingsOpen(false)} className="p-1 rounded-md text-surface-400 hover:text-surface-100">
            <X className="w-5 h-5" />
          </button>
        </div>

        <div className="p-5 space-y-8">
          {recording && (
            <p className="text-xs text-amber-300/90 bg-amber-500/10 rounded-lg px-3 py-2">
              Audio and model changes apply to the next meeting.
            </p>
          )}

          <section className="space-y-4">
            <h3 className="text-sm font-semibold text-surface-200">Audio sources</h3>
            <Row label="Your microphone">
              <select
                className={field}
                value={settings.mic_device ?? ''}
                onChange={(e) => update({ mic_device: e.target.value || null })}
              >
                <option value="">System default</option>
                {devices?.inputs.map((d) => (
                  <option key={d.name} value={d.name}>
                    {d.name}
                    {d.is_default ? ' (default)' : ''}
                  </option>
                ))}
              </select>
            </Row>

            <Row
              label="Other participants"
              hint={
                settings.them_source === 'system'
                  ? `Records everything playing on ${devices?.default_output ?? 'the default output'}. macOS asks for "System Audio Recording" permission the first time.`
                  : settings.them_source === 'device'
                    ? 'Pick a loopback device such as BlackHole that your meeting app outputs to.'
                    : 'Only your microphone is recorded.'
              }
            >
              <select
                className={field}
                value={settings.them_source}
                onChange={(e) => update({ them_source: e.target.value as Settings['them_source'] })}
              >
                <option value="system">System audio (recommended)</option>
                <option value="device">Loopback input device</option>
                <option value="off">Off</option>
              </select>
            </Row>

            {settings.them_source === 'device' && (
              <Row label="Loopback device">
                <select
                  className={field}
                  value={settings.them_device ?? ''}
                  onChange={(e) => update({ them_device: e.target.value || null })}
                >
                  <option value="">Select…</option>
                  {devices?.inputs.map((d) => (
                    <option key={d.name} value={d.name}>
                      {d.name}
                    </option>
                  ))}
                </select>
              </Row>
            )}

            <div className="grid grid-cols-2 gap-3">
              <Row label="Label for you">
                <input className={field} value={settings.me_label} onChange={(e) => update({ me_label: e.target.value })} />
              </Row>
              <Row label="Label for others">
                <input className={field} value={settings.them_label} onChange={(e) => update({ them_label: e.target.value })} />
              </Row>
            </div>
          </section>

          <section className="space-y-4">
            <h3 className="text-sm font-semibold text-surface-200">Transcription</h3>
            <Row label="Language">
              <select className={field} value={settings.language} onChange={(e) => update({ language: e.target.value })}>
                {LANGUAGES.map(([code, name]) => (
                  <option key={code} value={code}>
                    {name}
                  </option>
                ))}
              </select>
            </Row>

            <Row
              label="Live captions model"
              hint={
                englishOnlyMismatch ? (
                  <span className="flex items-center gap-1 text-amber-300">
                    <AlertTriangle className="w-3.5 h-3.5" /> {liveModel.name} is English only — pick Whisper Small for this language.
                  </span>
                ) : (
                  'Runs while you talk. Fast matters more than perfect here.'
                )
              }
            >
              <select className={field} value={settings.live_model} onChange={(e) => update({ live_model: e.target.value })}>
                {sttModels.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                    {m.is_downloaded ? '' : ' (not downloaded)'}
                  </option>
                ))}
              </select>
            </Row>

            <Row label="Accurate transcript model" hint="Runs on the recording after the meeting.">
              <select className={field} value={settings.refine_model} onChange={(e) => update({ refine_model: e.target.value })}>
                {sttModels.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}
                    {m.is_downloaded ? '' : ' (not downloaded)'}
                  </option>
                ))}
              </select>
            </Row>

            <label className="flex items-center justify-between gap-3 cursor-pointer">
              <span className="text-sm text-surface-300">Create accurate transcript automatically when a meeting ends</span>
              <input
                type="checkbox"
                checked={settings.auto_refine}
                onChange={(e) => update({ auto_refine: e.target.checked })}
                className="w-4 h-4 accent-accent-500"
              />
            </label>
          </section>

          <section className="space-y-3">
            <h3 className="text-sm font-semibold text-surface-200">Models</h3>
            {models.map((m) => {
              const p = progress[m.id];
              const pct = p && !p.done && p.total ? Math.round((p.downloaded / p.total) * 100) : null;
              return (
                <div key={m.id} className="rounded-lg border border-surface-800 px-3 py-2.5">
                  <div className="flex items-center gap-2">
                    <div className="flex-1 min-w-0">
                      <p className="text-sm text-surface-100">
                        {m.name} <span className="text-xs text-surface-500">{m.size_mb} MB</span>
                      </p>
                      <p className="text-xs text-surface-500">{m.description}</p>
                    </div>
                    {m.is_downloaded ? (
                      <CheckCircle2 className="w-4 h-4 text-accent-400 shrink-0" />
                    ) : m.is_downloading ? (
                      <span className="flex items-center gap-1 text-xs text-surface-400 shrink-0">
                        <Loader2 className="w-3.5 h-3.5 animate-spin" /> {pct !== null ? `${pct}%` : ''}
                      </span>
                    ) : (
                      <button
                        onClick={() => download(m.id)}
                        className="flex items-center gap-1 px-2.5 py-1 rounded-md bg-surface-700 hover:bg-surface-600 text-xs text-surface-100 shrink-0"
                      >
                        <Download className="w-3.5 h-3.5" /> Download
                      </button>
                    )}
                  </div>
                  {m.is_downloading && pct !== null && (
                    <div className="mt-2 h-1 rounded-full bg-surface-800 overflow-hidden">
                      <div className="h-full bg-accent-500" style={{ width: `${pct}%` }} />
                    </div>
                  )}
                </div>
              );
            })}
          </section>

          <section className="space-y-2">
            <h3 className="text-sm font-semibold text-surface-200">Storage</h3>
            <button
              onClick={() => root && revealItemInDir(root)}
              className="w-full flex items-center gap-2 px-3 py-2 rounded-lg bg-surface-800 hover:bg-surface-700 text-left"
            >
              <FolderOpen className="w-4 h-4 text-surface-400 shrink-0" />
              <span className="text-xs text-surface-300 truncate font-mono">{root}</span>
            </button>
          </section>
        </div>
      </div>
    </div>
  );
}
