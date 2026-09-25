import { AlertTriangle, CheckCircle2, X, XCircle } from 'lucide-react';
import { useApp } from '../stores/appStore';

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);

  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 w-[min(380px,calc(100vw-2rem))]">
      {toasts.map((t) => (
        <div
          key={t.id}
          className="flex items-start gap-2 px-3 py-2.5 rounded-lg bg-surface-800 border border-surface-700 shadow-xl text-sm"
        >
          {t.kind === 'error' ? (
            <XCircle className="w-4 h-4 mt-0.5 shrink-0 text-rec-400" />
          ) : t.kind === 'warn' ? (
            <AlertTriangle className="w-4 h-4 mt-0.5 shrink-0 text-amber-400" />
          ) : (
            <CheckCircle2 className="w-4 h-4 mt-0.5 shrink-0 text-accent-400" />
          )}
          <p className="flex-1 text-surface-200 whitespace-pre-line break-words">{t.text}</p>
          <button onClick={() => dismiss(t.id)} className="text-surface-500 hover:text-surface-300">
            <X className="w-4 h-4" />
          </button>
        </div>
      ))}
    </div>
  );
}
