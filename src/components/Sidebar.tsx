import { Loader2, Mic, Settings } from 'lucide-react';
import { dayLabel, duration, timeOfDay } from '../lib/format';
import type { MeetingMeta } from '../lib/types';
import { useApp } from '../stores/appStore';

function groupByDay(meetings: MeetingMeta[]) {
  const groups: { day: string; items: MeetingMeta[] }[] = [];
  for (const m of meetings) {
    const day = dayLabel(m.started_at);
    const last = groups[groups.length - 1];
    if (last?.day === day) last.items.push(m);
    else groups.push({ day, items: [m] });
  }
  return groups;
}

export function Sidebar() {
  const { meetings, selectedId, view, recState, active } = useApp();
  const { select, showLive, setSettingsOpen } = useApp.getState();
  const recording = recState !== 'idle';
  const past = meetings.filter((m) => m.id !== active?.id);

  return (
    <aside className="w-64 shrink-0 flex flex-col border-r border-surface-800 bg-surface-950/40">
      <div className="flex items-center justify-between px-4 pt-4 pb-3">
        <span className="text-sm font-semibold tracking-wide text-surface-200">PP Scribe</span>
        <button
          onClick={() => setSettingsOpen(true)}
          className="p-1.5 rounded-md text-surface-400 hover:text-surface-100 hover:bg-surface-800"
          title="Settings"
        >
          <Settings className="w-4 h-4" />
        </button>
      </div>

      <div className="px-2">
        <button
          onClick={showLive}
          className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm transition-colors ${
            view === 'live' ? 'bg-surface-800 text-surface-50' : 'text-surface-300 hover:bg-surface-800/60'
          }`}
        >
          {recording ? (
            <span className="w-2 h-2 rounded-full bg-rec-500 rec-pulse" />
          ) : (
            <Mic className="w-4 h-4 text-surface-400" />
          )}
          <span className="truncate">{recording ? active?.title ?? 'Recording…' : 'New meeting'}</span>
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-2 pb-4 mt-3">
        {past.length === 0 && <p className="px-3 text-xs text-surface-600">Saved meetings appear here.</p>}
        {groupByDay(past).map((g) => (
          <div key={g.day} className="mb-3">
            <p className="px-3 py-1 text-[11px] font-medium uppercase tracking-wider text-surface-500">{g.day}</p>
            {g.items.map((m) => {
              const selected = view === 'meeting' && selectedId === m.id;
              return (
                <button
                  key={m.id}
                  onClick={() => select(m.id)}
                  className={`w-full text-left px-3 py-2 rounded-lg transition-colors ${
                    selected ? 'bg-surface-800' : 'hover:bg-surface-800/60'
                  }`}
                >
                  <p className={`text-sm truncate ${selected ? 'text-surface-50' : 'text-surface-200'}`}>{m.title}</p>
                  <p className="flex items-center gap-1.5 text-xs text-surface-500">
                    {timeOfDay(m.started_at)} · {duration(m.duration_ms)}
                    {m.status === 'refining' && <Loader2 className="w-3 h-3 animate-spin text-accent-400" />}
                    {(m.status === 'recorded' || m.status === 'refine_failed') && (
                      <span className="text-[10px] px-1 rounded bg-surface-800 text-surface-400">live only</span>
                    )}
                  </p>
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </aside>
  );
}
