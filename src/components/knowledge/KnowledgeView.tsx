import { useEffect, useRef, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { Download, FilePlus2, Link2, Loader2, NotebookPen, Plus, Search, Upload, X } from 'lucide-react';
import { api, errorText } from '../../lib/api';
import type { KbAddResult } from '../../lib/types';
import { useApp } from '../../stores/appStore';
import { useKnowledge } from '../../stores/knowledgeStore';
import { ResultList } from './ResultList';
import { SourceDetail } from './SourceDetail';
import { SourceList } from './SourceList';

const DOC_EXTENSIONS = ['pdf', 'docx', 'md', 'markdown', 'txt', 'csv', 'json', 'log'];

type Composer = { kind: 'note'; title: string; text: string } | { kind: 'url'; url: string } | null;

export function KnowledgeView() {
  const { query, openSourceId, status } = useKnowledge();
  const { setQuery, setHits, setSearching, upsertSource, openSource } = useKnowledge.getState();
  const toast = useApp((s) => s.toast);

  const [composer, setComposer] = useState<Composer>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const searchSeq = useRef(0);

  // Search as you type (debounced); stale responses are dropped.
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setHits([]);
      return;
    }
    setSearching(true);
    const seq = ++searchSeq.current;
    const t = setTimeout(() => {
      api
        .kbSearch(q, undefined, 20)
        .then((hits) => seq === searchSeq.current && setHits(hits))
        .catch((e) => {
          if (seq === searchSeq.current) setHits([]);
          toast('error', errorText(e));
        });
    }, 200);
    return () => clearTimeout(t);
  }, [query, status?.missing_embeddings === 0]);

  const report = (results: KbAddResult[]) => {
    for (const r of results) {
      upsertSource(r.source);
      if (r.duplicate) toast('info', `Already in your knowledge base: ${r.source.title}`);
    }
  };

  const addPaths = async (paths: string[]) => {
    if (paths.length === 0) return;
    const results = await api.kbAddFiles(paths);
    const ok: KbAddResult[] = [];
    for (const r of results) {
      if ('Ok' in r) ok.push(r.Ok);
      else toast('error', r.Err);
    }
    report(ok);
  };

  const pickFiles = async () => {
    setMenuOpen(false);
    const picked = await open({ multiple: true, filters: [{ name: 'Documents', extensions: DOC_EXTENSIONS }] });
    if (picked) await addPaths(Array.isArray(picked) ? picked : [picked]);
  };

  const submitComposer = async () => {
    if (!composer) return;
    setBusy(true);
    try {
      const r =
        composer.kind === 'note'
          ? await api.kbAddText(composer.title, composer.text)
          : await api.kbAddUrl(composer.url);
      report([r]);
      setComposer(null);
    } catch (e) {
      toast('error', errorText(e));
    } finally {
      setBusy(false);
    }
  };

  // Drop files anywhere on this view.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    try {
      getCurrentWebview()
        .onDragDropEvent((e) => {
          if (e.payload.type === 'enter' || e.payload.type === 'over') setDragging(true);
          else if (e.payload.type === 'leave') setDragging(false);
          else if (e.payload.type === 'drop') {
            setDragging(false);
            addPaths(e.payload.paths);
          }
        })
        .then((f) => (unlisten = f));
    } catch {
      // not running inside Tauri (UI preview)
    }
    return () => unlisten?.();
  }, []);

  // ⌘V outside a text field turns clipboard text into a note (or a link).
  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const el = document.activeElement;
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return;
      if (e.clipboardData && [...e.clipboardData.items].some((i) => i.type.startsWith('image/'))) {
        toast('info', 'Pasting images arrives with image support (next phase).');
        return;
      }
      const text = e.clipboardData?.getData('text/plain')?.trim();
      if (!text) return;
      e.preventDefault();
      setMenuOpen(false);
      if (/^https?:\/\/\S+$/.test(text)) setComposer({ kind: 'url', url: text });
      else setComposer({ kind: 'note', title: '', text });
    };
    window.addEventListener('paste', onPaste);
    return () => window.removeEventListener('paste', onPaste);
  }, []);

  // ⌘K / ⌘F focuses search.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && (e.key === 'k' || e.key === 'f')) {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const menuItem = 'w-full flex items-center gap-2.5 px-3 py-2 text-sm text-surface-200 hover:bg-surface-700 text-left';

  return (
    <div className="relative flex flex-col h-full min-w-0">
      <header className="px-4 sm:px-6 pt-4 pb-3 border-b border-surface-800 space-y-3">
        <div className="flex items-center gap-2">
          <div className="flex-1 flex items-center gap-2 px-3 py-2 rounded-xl bg-surface-800 focus-within:ring-1 focus-within:ring-surface-600">
            <Search className="w-4 h-4 text-surface-500 shrink-0" />
            <input
              ref={searchRef}
              autoFocus
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                if (openSourceId !== null) openSource(null);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Escape') {
                  setQuery('');
                  openSource(null);
                }
              }}
              placeholder="Search your notes, documents and meetings — in any language"
              className="flex-1 min-w-0 bg-transparent text-[15px] text-surface-100 outline-none placeholder:text-surface-500"
            />
            {query && (
              <button
                onClick={() => {
                  setQuery('');
                  openSource(null);
                }}
                className="text-surface-500 hover:text-surface-300"
                title="Clear"
              >
                <X className="w-4 h-4" />
              </button>
            )}
          </div>

          <div className="relative">
            <button
              onClick={() => setMenuOpen((o) => !o)}
              className="flex items-center gap-1.5 px-3 py-2 rounded-xl bg-surface-700 hover:bg-surface-600 text-sm text-surface-100"
            >
              <Plus className="w-4 h-4" /> Add
            </button>
            {menuOpen && (
              <>
                <div className="fixed inset-0 z-10" onClick={() => setMenuOpen(false)} />
                <div className="absolute right-0 mt-1 z-20 w-52 py-1 rounded-lg bg-surface-800 border border-surface-700 shadow-xl">
                  <button className={menuItem} onClick={pickFiles}>
                    <FilePlus2 className="w-4 h-4 text-surface-400" /> Files…
                  </button>
                  <button
                    className={menuItem}
                    onClick={() => {
                      setMenuOpen(false);
                      setComposer({ kind: 'note', title: '', text: '' });
                    }}
                  >
                    <NotebookPen className="w-4 h-4 text-surface-400" /> Note / pasted text
                  </button>
                  <button
                    className={menuItem}
                    onClick={() => {
                      setMenuOpen(false);
                      setComposer({ kind: 'url', url: '' });
                    }}
                  >
                    <Link2 className="w-4 h-4 text-surface-400" /> Web page link
                  </button>
                  <p className="px-3 pt-1.5 pb-1 text-[11px] text-surface-500 border-t border-surface-700 mt-1">
                    Or drop files here, or press ⌘V
                  </p>
                </div>
              </>
            )}
          </div>
        </div>

        {composer && (
          <div className="rounded-xl border border-surface-700 bg-surface-850 p-3 space-y-2">
            {composer.kind === 'note' ? (
              <>
                <input
                  value={composer.title}
                  onChange={(e) => setComposer({ ...composer, title: e.target.value })}
                  placeholder="Title (optional — taken from the first line)"
                  className="w-full bg-transparent text-sm font-medium text-surface-100 outline-none placeholder:text-surface-500"
                />
                <textarea
                  autoFocus={!composer.text}
                  value={composer.text}
                  onChange={(e) => setComposer({ ...composer, text: e.target.value })}
                  placeholder="Paste or type text…"
                  rows={Math.min(10, Math.max(4, composer.text.split('\n').length))}
                  className="w-full resize-y bg-surface-900 rounded-lg px-3 py-2 text-sm text-surface-200 outline-none placeholder:text-surface-600"
                />
              </>
            ) : (
              <input
                autoFocus
                value={composer.url}
                onChange={(e) => setComposer({ ...composer, url: e.target.value })}
                onKeyDown={(e) => e.key === 'Enter' && submitComposer()}
                placeholder="https://…"
                className="w-full bg-surface-900 rounded-lg px-3 py-2 text-sm text-surface-200 outline-none placeholder:text-surface-600"
              />
            )}
            <div className="flex items-center justify-end gap-2">
              <button onClick={() => setComposer(null)} className="px-3 py-1.5 rounded-lg text-sm text-surface-400 hover:text-surface-200">
                Cancel
              </button>
              <button
                onClick={submitComposer}
                disabled={busy || (composer.kind === 'note' ? !composer.text.trim() : !composer.url.trim())}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-accent-600 hover:bg-accent-500 disabled:opacity-50 text-sm text-white"
              >
                {busy && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                {composer.kind === 'note' ? 'Add note' : 'Add page'}
              </button>
            </div>
          </div>
        )}

        {status && !status.semantic_ready && <ModelBanner />}
      </header>

      <div className="flex-1 min-h-0">
        {openSourceId !== null ? <SourceDetail id={openSourceId} /> : query.trim() ? <ResultList /> : <SourceList />}
      </div>

      {dragging && (
        <div className="absolute inset-3 z-30 flex flex-col items-center justify-center gap-2 rounded-2xl border-2 border-dashed border-accent-500/60 bg-surface-900/90 text-surface-200 pointer-events-none">
          <Upload className="w-7 h-7 text-accent-400" />
          <p className="text-sm">Drop to add to your knowledge base</p>
          <p className="text-xs text-surface-500">PDF, Word, Markdown, text</p>
        </div>
      )}
    </div>
  );
}

/** Keyword search works without it; the model adds meaning-based and cross-language search. */
function ModelBanner() {
  const [pct, setPct] = useState<number | null>(null);
  const toast = useApp((s) => s.toast);

  useEffect(() => {
    const un = listen<{ model_id: string; downloaded: number; total: number; done: boolean; error: string | null }>(
      'model-download-progress',
      (e) => {
        if (e.payload.model_id !== 'bge-m3') return;
        if (e.payload.done) {
          setPct(null);
          if (e.payload.error) toast('error', `Download failed: ${e.payload.error}`);
          else {
            api.kbBackfill();
            api.kbStatus().then(useKnowledge.getState().setStatus);
          }
        } else if (e.payload.total) {
          setPct(Math.round((e.payload.downloaded / e.payload.total) * 100));
        }
      },
    );
    return () => {
      un.then((f) => f());
    };
  }, []);

  const start = () => {
    setPct(0);
    api.downloadModel('bge-m3').catch(() => {});
  };

  return (
    <div className="flex items-center gap-3 rounded-xl bg-amber-500/10 px-4 py-2.5 text-sm">
      <p className="flex-1 text-amber-200/90">
        Keyword search is on. For search by meaning — and across Turkish and English — download the search model
        (587 MB, runs locally).
      </p>
      <button
        onClick={start}
        disabled={pct !== null}
        className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-surface-700 hover:bg-surface-600 disabled:opacity-70 text-xs text-surface-100 shrink-0"
      >
        {pct !== null ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Download className="w-3.5 h-3.5" />}
        {pct !== null ? `${pct}%` : 'Download'}
      </button>
    </div>
  );
}
