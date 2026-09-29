import type { ReactNode } from 'react';

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Query words worth highlighting (prefix match, so "bütçe" also marks "bütçesi"). */
export function queryTerms(query: string): string[] {
  return query
    .split(/[^\p{L}\p{N}]+/u)
    .filter((w) => w.length >= 3)
    .sort((a, b) => b.length - a.length);
}

export function highlight(text: string, terms: string[]): ReactNode {
  if (terms.length === 0) return text;
  const re = new RegExp(`(${terms.map(escape).join('|')})`, 'giu');
  return text.split(re).map((part, i) =>
    i % 2 === 1 ? (
      <mark key={i} className="bg-amber-400/30 text-inherit rounded-sm px-0.5">
        {part}
      </mark>
    ) : (
      part
    ),
  );
}

/** ~`max` chars around the first term match, so result cards show why they matched. */
export function snippet(text: string, terms: string[], max = 320): string {
  const flat = text.replace(/\s+/g, ' ').trim();
  if (flat.length <= max) return flat;
  const lower = flat.toLowerCase();
  const hit = terms.map((t) => lower.indexOf(t.toLowerCase())).filter((i) => i >= 0).sort((a, b) => a - b)[0];
  const start = hit === undefined ? 0 : Math.max(0, hit - Math.floor(max / 3));
  const body = flat.slice(start, start + max);
  return `${start > 0 ? '…' : ''}${body}${start + max < flat.length ? '…' : ''}`;
}
