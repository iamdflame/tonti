import type { Locale } from '@/i18n';
import type { Odds } from '@/lib/odds';
import { countryName, pct } from '@/lib/format';

const TICKS = [0, 0.25, 0.5, 0.75, 1];

/** A dumbbell per country: women (ink) and men (slate) on one 0–100% scale. The values sit beside
 * each row as text, so nothing needs a hover to be read; the dot beside each value names it. */
export function OddsChart({ rows, locale, labels }: { rows: Odds[]; locale: Locale; labels: { women: string; men: string; row: (country: string, female: string, male: string) => string } }) {
  const sorted = [...rows].sort((a, b) => b.female - a.female);
  const grid = 'grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 sm:grid-cols-[10rem_minmax(0,1fr)_8.5rem]';
  const key = (tone: string) => <span className={`inline-block size-2.5 shrink-0 rounded-full ${tone}`} aria-hidden="true" />;
  return (
    <figure>
      <ul className="mb-3 flex gap-5 text-caption text-ink-2" aria-hidden="true">
        <li className="inline-flex items-center gap-2">{key('bg-ink')}{labels.women}</li>
        <li className="inline-flex items-center gap-2">{key('bg-slate')}{labels.men}</li>
      </ul>
      <ul className="divide-y divide-haze-2">
        {sorted.map((r) => {
          const name = countryName(locale, r.iso3, r.name);
          const lo = Math.min(r.female, r.male);
          const hi = Math.max(r.female, r.male);
          return (
            <li key={r.iso3} tabIndex={0} aria-label={labels.row(name, pct(locale, r.female), pct(locale, r.male))} className={`${grid} items-center gap-y-1.5 px-2 py-2.5 outline-none hover:bg-haze focus-visible:ring-2 focus-visible:ring-ring`}>
              <span className="truncate text-body font-bold">{name}</span>
              <span className="flex justify-end gap-3 font-mono text-caption tabular sm:order-3">
                <span className="inline-flex items-center gap-1.5">{key('bg-ink')}{pct(locale, r.female)}</span>
                <span className="inline-flex items-center gap-1.5">{key('bg-slate')}{pct(locale, r.male)}</span>
              </span>
              <div className="relative col-span-2 h-4 sm:order-2 sm:col-span-1" aria-hidden="true">
                {TICKS.map((x) => (
                  <span key={x} className="absolute inset-y-0 w-px bg-haze-2" style={{ left: `${x * 100}%` }} />
                ))}
                <span className="absolute top-1/2 h-0.5 -translate-y-1/2 rounded-full bg-slate-soft" style={{ left: `${lo * 100}%`, width: `${(hi - lo) * 100}%` }} />
                <span className="absolute top-1/2 size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-slate ring-2 ring-paper" style={{ left: `${r.male * 100}%` }} />
                <span className="absolute top-1/2 size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-ink ring-2 ring-paper" style={{ left: `${r.female * 100}%` }} />
              </div>
            </li>
          );
        })}
      </ul>
      <div className={`${grid} px-2 pt-2`} aria-hidden="true">
        <span className="hidden sm:block" />
        <div className="relative col-span-2 h-5 font-mono text-caption text-ink-2 tabular sm:col-span-1">
          {TICKS.map((x) => (
            <span key={x} className={`absolute ${x === 0 ? '' : x === 1 ? '-translate-x-full' : '-translate-x-1/2'}`} style={{ left: `${x * 100}%` }}>
              {pct(locale, x)}
            </span>
          ))}
        </div>
      </div>
    </figure>
  );
}
