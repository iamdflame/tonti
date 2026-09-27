'use client';

import { useReducedMotion } from '@/lib/motion';

type Props = {
  startAge: number;
  p10: number;
  p50: number;
  p90: number;
  soloRunoutAge: number;
  survival: { age: number; alive: number }[];
  labels: { pool: string; alone: string; alive: string; runsOut: string; aliveAt: string; summary: string; title: string };
};

const W = 600;
const H = 240;
const T = 16;
const B = 8;

/** Income for life from the pool (the lamp) against the same income drawn alone (goes out), over
 * her chance of being alive (the light). Shapes are SVG; every word is HTML, so text stays at
 * readable sizes on a phone. Drawn once when the answer arrives, then still. */
export function LampChart({ startAge, p10, p50, p90, soloRunoutAge, survival, labels }: Props) {
  const reduce = useReducedMotion();
  const end = 100;
  const fx = (age: number) => (age - startAge) / (end - startAge); // 0..1
  const x = (age: number) => fx(age) * W;
  const top = Math.max(p90 * 1.35, p50 * 1.6);
  const y = (v: number) => H - B - (v / top) * (H - T - B);
  const ya = (a: number) => H - B - a * (H - T - B);
  const runout = Math.min(Math.max(soloRunoutAge, startAge), end);
  const aliveAtRunout = survival.find((s) => s.age >= runout)?.alive ?? 0;
  const pts = survival.filter((s) => s.age <= end);
  const area = pts.length > 1 ? `M0,${ya(0)} ` + pts.map((s) => `L${x(s.age)},${ya(s.alive)}`).join(' ') + ` L${x(pts[pts.length - 1].age)},${ya(0)} Z` : '';
  const ticks = [startAge, ...[70, 80, 90].filter((a) => a > startAge + 4 && a < end - 4), end];
  // Drawn once (900 ms, ease-out) by CSS: pathLength 1, dash offset from 1 to 0.
  const draw = reduce ? { className: 'animate-in fade-in duration-200' } : { pathLength: 1, className: 'lamp-draw' };
  const runX = fx(runout) * 100;

  return (
    <figure className="w-full" aria-label={labels.title}>
      <ul className="mb-4 flex flex-wrap gap-x-5 gap-y-2 text-caption text-foreground/90">
        <li className="inline-flex items-center gap-2">
          <span className="h-1 w-6 rounded-full bg-lamp shadow-[0_0_8px_rgba(255,178,63,0.8)]" aria-hidden="true" />
          {labels.pool}
        </li>
        <li className="inline-flex items-center gap-2">
          <span className="h-0.5 w-6 rounded-full bg-mist" aria-hidden="true" />
          {labels.alone}
        </li>
        <li className="inline-flex items-center gap-2">
          <span className="h-3 w-6 rounded-sm bg-mist/25" aria-hidden="true" />
          {labels.alive}
        </li>
      </ul>
      <div className="relative">
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label={labels.summary} className="block h-44 w-full overflow-visible sm:h-52">
          <defs>
            <linearGradient id="aliveFill" x1="0" x2="0" y1="0" y2="1">
              <stop offset="0" stopColor="#a9b4c6" stopOpacity="0.26" />
              <stop offset="1" stopColor="#a9b4c6" stopOpacity="0.05" />
            </linearGradient>
          </defs>
          {area && <path d={area} fill="url(#aliveFill)" />}
          <rect x={0} width={W} y={y(p90)} height={Math.max(3, y(p10) - y(p90))} fill="#ffb23f" fillOpacity={0.16} className="animate-in fade-in duration-500" />
          <line x1={x(runout)} x2={x(runout)} y1={ya(aliveAtRunout)} y2={ya(0)} stroke="#a9b4c6" strokeOpacity={0.45} strokeDasharray="3 5" vectorEffect="non-scaling-stroke" />
          <path d={`M0,${y(p50)} H${x(runout)} V${y(0)}`} fill="none" stroke="#a9b4c6" strokeWidth={2.5} strokeLinejoin="round" vectorEffect="non-scaling-stroke" {...draw} />
          <path d={`M0,${y(p50)} H${W}`} stroke="#ffb23f" strokeWidth={10} strokeOpacity={0.25} vectorEffect="non-scaling-stroke" />
          <path d={`M0,${y(p50)} H${W}`} fill="none" stroke="#ffb23f" strokeWidth={3.5} strokeLinecap="round" vectorEffect="non-scaling-stroke" {...draw} />
        </svg>
        {/* The moment the savings go out: a dimmed lamp, and how likely she is to be alive then. */}
        <div className="pointer-events-none absolute" style={{ left: `${runX}%`, top: `${(y(p50) / H) * 100}%` }} aria-hidden="true">
          <span className="block size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-mist bg-bay" />
        </div>
        <div className="pointer-events-none absolute" style={{ left: `${runX}%`, top: `${(ya(aliveAtRunout) / H) * 100}%` }} aria-hidden="true">
          <span className="block size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white" />
        </div>
        <div className="pointer-events-none absolute right-0" style={{ top: `${(y(p50) / H) * 100}%` }} aria-hidden="true">
          <span className="block size-3 translate-x-1/2 -translate-y-1/2 rounded-full bg-lamp-soft shadow-[0_0_14px_4px_rgba(255,178,63,0.6)]" />
        </div>
      </div>
      <div className="relative mt-2 h-5 font-mono text-caption text-mist tabular" aria-hidden="true">
        {ticks.map((a) => (
          <span key={a} className={`absolute ${a === startAge ? '' : a === end ? '-translate-x-full' : '-translate-x-1/2'}`} style={{ left: `${fx(a) * 100}%` }}>
            {a}
          </span>
        ))}
      </div>
      <p className="mt-3 text-caption text-foreground/90">
        <span className="font-bold text-white">{labels.runsOut}</span> · {labels.aliveAt}
      </p>
      <figcaption className="sr-only">{labels.summary}</figcaption>
    </figure>
  );
}
