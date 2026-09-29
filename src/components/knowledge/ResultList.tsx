import { Loader2, Sparkles } from 'lucide-react';
import { highlight, queryTerms, snippet } from '../../lib/highlight';
import type { KbHit } from '../../lib/types';
import { useApp } from '../../stores/appStore';
import { useKnowledge } from '../../stores/knowledgeStore';
import { KindIcon, locatorLabel } from './meta';

export function ResultList() {
  const { hits, searching, query, status } = useKnowledge();
  const terms = queryTerms(query);

  if (hits.length === 0) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-surface-500">
        {searching ? <Loader2 className="w-5 h-5 animate-spin" /> : 'Nothing found'}
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto px-4 sm:px-6 py-4">
      <div className="max-w-3xl mx-auto space-y-2.5">
        {status && status.missing_embeddings > 0 && status.semantic_ready && (
          <p className="text-xs text-surface-500">Still indexing {status.missing_embeddings} passages — results will improve.</p>
        )}
        {hits.map((h) => (
          <ResultCard key={h.chunk_id} hit={h} terms={terms} />
        ))}
      </div>
    </div>
  );
}

function ResultCard({ hit: h, terms }: { hit: KbHit; terms: string[] }) {
  const openSource = useKnowledge((s) => s.openSource);
  const select = useApp((s) => s.select);
  const where = locatorLabel(h.locator);

  const open = () =>
    h.source_kind === 'meeting' && h.origin
      ? select(h.origin, h.locator.start_ms)
      : openSource(h.source_id, h.chunk_id);

  return (
    <button
      onClick={open}
      className="w-full text-left rounded-xl border border-surface-800 hover:border-surface-600 bg-surface-900 hover:bg-surface-850 px-4 py-3 transition-colors"
    >
      <div className="flex items-center gap-2 mb-1">
        <KindIcon kind={h.source_kind} className="w-3.5 h-3.5" />
        <span className="text-sm font-medium text-surface-100 truncate">{h.source_title}</span>
        {where && <span className="text-xs font-mono text-surface-500 truncate">{where}</span>}
        {!h.keyword && (
          <span title="Matched by meaning (no shared words)" className="ml-auto shrink-0">
            <Sparkles className="w-3.5 h-3.5 text-violet-300/80" />
          </span>
        )}
      </div>
      <p className="text-sm leading-relaxed text-surface-300">{highlight(snippet(h.text, terms), terms)}</p>
    </button>
  );
}
