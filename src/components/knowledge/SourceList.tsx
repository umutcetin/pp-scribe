import { useState } from 'react';
import { AlertCircle, Library, Loader2 } from 'lucide-react';
import { dayLabel } from '../../lib/format';
import type { KbKind, KbSource } from '../../lib/types';
import { useApp } from '../../stores/appStore';
import { useKnowledge } from '../../stores/knowledgeStore';
import { KIND_LABEL, KindIcon } from './meta';

const FILTERS: { label: string; kinds: KbKind[] | null }[] = [
  { label: 'All', kinds: null },
  { label: 'Documents', kinds: ['pdf', 'docx', 'md', 'txt'] },
  { label: 'Notes', kinds: ['note'] },
  { label: 'Web', kinds: ['url'] },
  { label: 'Meetings', kinds: ['meeting'] },
];

export function SourceList() {
  const sources = useKnowledge((s) => s.sources);
  const [filter, setFilter] = useState(0);
  const kinds = FILTERS[filter].kinds;
  const shown = kinds ? sources.filter((s) => kinds.includes(s.kind)) : sources;

  if (sources.length === 0) {
    return (
      <div className="h-full flex items-center justify-center p-6">
        <div className="max-w-sm text-center space-y-2">
          <Library className="w-8 h-8 mx-auto text-surface-600" />
          <p className="text-sm text-surface-300">Your knowledge base is empty</p>
          <p className="text-xs text-surface-500">
            Add PDFs, Word files, notes or web pages with <b>Add</b>, drop files here, or paste text with ⌘V. Meetings are
            added automatically. Everything stays on this Mac.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto px-4 sm:px-6 py-4">
      <div className="max-w-3xl mx-auto">
        <div className="flex flex-wrap gap-1.5 mb-3">
          {FILTERS.map((f, i) => {
            const n = f.kinds ? sources.filter((s) => f.kinds!.includes(s.kind)).length : sources.length;
            if (n === 0 && i !== 0) return null;
            return (
              <button
                key={f.label}
                onClick={() => setFilter(i)}
                className={`px-2.5 py-1 rounded-full text-xs transition-colors ${
                  filter === i ? 'bg-surface-600 text-surface-50' : 'bg-surface-800 text-surface-400 hover:text-surface-200'
                }`}
              >
                {f.label} <span className="text-surface-500">{n}</span>
              </button>
            );
          })}
        </div>
        <div className="divide-y divide-surface-800">
          {shown.map((s) => (
            <SourceRow key={s.id} source={s} />
          ))}
        </div>
      </div>
    </div>
  );
}

function SourceRow({ source: s }: { source: KbSource }) {
  const openSource = useKnowledge((st) => st.openSource);
  const select = useApp((st) => st.select);
  const open = () => (s.kind === 'meeting' && s.origin ? select(s.origin) : openSource(s.id));

  return (
    <button onClick={open} className="w-full flex items-center gap-3 py-2.5 px-2 -mx-2 rounded-lg text-left hover:bg-surface-800/60">
      <KindIcon kind={s.kind} />
      <div className="flex-1 min-w-0">
        <p className="text-sm text-surface-100 truncate">{s.title}</p>
        <p className="text-xs text-surface-500 truncate">
          {KIND_LABEL[s.kind]} · {dayLabel(s.added_at)}
          {s.status === 'ready' && ` · ${s.chunk_count} ${s.chunk_count === 1 ? 'passage' : 'passages'}`}
          {s.status === 'error' && <span className="text-rec-400"> · {s.error}</span>}
        </p>
      </div>
      {(s.status === 'pending' || s.status === 'indexing') && (
        <span className="flex items-center gap-1 text-xs text-surface-500 shrink-0">
          <Loader2 className="w-3.5 h-3.5 animate-spin" /> {s.status === 'pending' ? 'Queued' : 'Indexing'}
        </span>
      )}
      {s.status === 'error' && <AlertCircle className="w-4 h-4 text-rec-400 shrink-0" />}
    </button>
  );
}
