import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { ArrowDown } from 'lucide-react';
import { clock, toTurns } from '../lib/format';
import type { Caption, Segment, Speaker } from '../lib/types';

interface Props {
  segments: (Segment | Caption)[];
  labels: Record<Speaker, string>;
  /** Follow new text like a live caption feed. */
  follow?: boolean;
  query?: string;
  empty?: ReactNode;
}

const speakerStyle: Record<Speaker, { name: string; bar: string }> = {
  me: { name: 'text-accent-400', bar: 'bg-accent-500/60' },
  them: { name: 'text-them-400', bar: 'bg-them-500/60' },
};

function highlight(text: string, query?: string): ReactNode {
  const q = query?.trim();
  if (!q) return text;
  const parts = text.split(new RegExp(`(${q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi'));
  return parts.map((p, i) =>
    p.toLowerCase() === q.toLowerCase() ? (
      <mark key={i} className="bg-amber-400/30 text-inherit rounded-sm px-0.5">
        {p}
      </mark>
    ) : (
      p
    ),
  );
}

export function TranscriptView({ segments, labels, follow = false, query, empty }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [stuck, setStuck] = useState(true);

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (follow && stuck && el) el.scrollTop = el.scrollHeight;
  }, [segments, follow, stuck]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    setStuck(el.scrollHeight - el.scrollTop - el.clientHeight < 80);
  };

  const q = query?.trim().toLowerCase();
  const visible = q ? segments.filter((s) => s.text.toLowerCase().includes(q)) : segments;
  const turns = toTurns(visible);

  return (
    <div className="relative flex-1 min-h-0">
      <div ref={scrollRef} onScroll={onScroll} className="h-full overflow-y-auto px-4 sm:px-8 py-6">
        {turns.length === 0 ? (
          <div className="h-full flex items-center justify-center text-surface-500 text-sm">
            {q ? 'No matches' : empty}
          </div>
        ) : (
          <div className="max-w-3xl mx-auto space-y-5">
            {turns.map((turn) => (
              <div key={turn.key} className="flex gap-3">
                <div className={`w-0.5 shrink-0 rounded-full ${speakerStyle[turn.speaker].bar}`} />
                <div className="min-w-0 flex-1">
                  <div className="flex items-baseline gap-2 mb-0.5">
                    <span className={`text-sm font-semibold ${speakerStyle[turn.speaker].name}`}>
                      {labels[turn.speaker]}
                    </span>
                    <span className="text-xs font-mono text-surface-500">{clock(turn.start_ms)}</span>
                  </div>
                  <p className="text-[15px] leading-relaxed text-surface-100 select-text">
                    {turn.items.map((s) => {
                      const interim = 'is_final' in s && !s.is_final;
                      return (
                        <span key={s.id} className={interim ? 'text-surface-400 italic' : undefined}>
                          {highlight(s.text, query)}{' '}
                        </span>
                      );
                    })}
                  </p>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {follow && !stuck && turns.length > 0 && (
        <button
          onClick={() => setStuck(true)}
          className="absolute bottom-4 left-1/2 -translate-x-1/2 flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-surface-700 hover:bg-surface-600 text-xs text-surface-100 shadow-lg"
        >
          <ArrowDown className="w-3.5 h-3.5" />
          Jump to live
        </button>
      )}
    </div>
  );
}
