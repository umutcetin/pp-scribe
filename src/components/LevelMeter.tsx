import type { Speaker } from '../lib/types';

interface Props {
  speaker: Speaker;
  label: string;
  level: number;
  enabled: boolean;
}

/** Five-bar peak meter; levels arrive ~10×/s per channel. */
export function LevelMeter({ speaker, label, level, enabled }: Props) {
  // map peak amplitude to a perceptual 0..1 (≈ -50 dB .. 0 dB)
  const db = 20 * Math.log10(Math.max(level, 1e-5));
  const v = Math.min(1, Math.max(0, (db + 50) / 50));
  const lit = Math.round(v * 5);
  const on = speaker === 'me' ? 'bg-accent-400' : 'bg-them-400';

  return (
    <div className="flex items-center gap-1.5" title={enabled ? `${label} input level` : `${label}: not captured`}>
      <span className={`text-xs ${enabled ? 'text-surface-300' : 'text-surface-600 line-through'}`}>{label}</span>
      <div className="flex items-end gap-[2px] h-3.5">
        {[0, 1, 2, 3, 4].map((i) => (
          <div
            key={i}
            style={{ height: `${40 + i * 15}%` }}
            className={`w-[3px] rounded-sm transition-colors duration-75 ${enabled && i < lit ? on : 'bg-surface-700'}`}
          />
        ))}
      </div>
    </div>
  );
}
