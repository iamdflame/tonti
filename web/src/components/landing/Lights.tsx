'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { useReducedMotion } from '@/lib/motion';
import { useI18n } from '@/i18n/client';
import { num, pct, usd } from '@/lib/format';

type Cohort = { alive: number[]; income_real: number[]; shadow_runout_year: number; shadow_alive_share: number; rule4_runout_year: number | null };
type Replay = { setup: { members: number; pot: number }; cohorts: Record<string, Cohort> };

const START = 1965;
// Five towers, 2,000 windows: one for each woman in the replay.
const TOWERS: [number, number][] = [
  [10, 40],
  [12, 35],
  [8, 45],
  [14, 30],
  [10, 40],
];

function seededOrder(n: number, seed: number) {
  const idx = Array.from({ length: n }, (_, i) => i);
  let s = seed >>> 0;
  for (let i = n - 1; i > 0; i--) {
    s = (s * 1664525 + 1013904223) >>> 0;
    const j = s % (i + 1);
    [idx[i], idx[j]] = [idx[j], idx[i]];
  }
  const rank = new Array<number>(n);
  idx.forEach((w, k) => (rank[w] = k));
  return rank;
}

/** 2,000 lit windows: the replay's women. Deaths turn windows off; the survivors' income, shared
 * from those deaths, makes the lit ones brighter. The lamp beside them is the same money drawn
 * alone: it goes out while half the windows are still lit. */
export function Lights() {
  const { t, f, locale } = useI18n();
  const reduce = useReducedMotion();
  const [data, setData] = useState<Replay | null>(null);
  const [year, setYear] = useState(START);
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    fetch('/data/replay.json').then((r) => r.json()).then(setData).catch(() => setData(null));
  }, []);
  const c = data?.cohorts[String(START)];
  const years = c ? c.alive.length - 1 : 34;
  const rank = useMemo(() => seededOrder(2000, 1965), []);

  // Scrolling through the section runs the years (one passive listener, one frame at a time).
  useEffect(() => {
    if (reduce) return;
    let raf = 0;
    const on = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        const el = wrap.current;
        if (!el) return;
        const r = el.getBoundingClientRect();
        const p = Math.min(1, Math.max(0, -r.top / Math.max(1, r.height - window.innerHeight)));
        setYear(START + Math.round(p * years));
      });
    };
    window.addEventListener('scroll', on, { passive: true });
    return () => {
      window.removeEventListener('scroll', on);
      cancelAnimationFrame(raf);
    };
  }, [reduce, years]);
  useEffect(() => {
    if (reduce && c) setYear(c.shadow_runout_year);
  }, [reduce, c]);

  const i = Math.min(years, Math.max(0, year - START));
  const alive = c ? c.alive[i] : 2000;
  const income = c ? c.income_real[i] : 0;
  const bright = c ? Math.min(2.6, Math.max(0.8, c.income_real[i] / c.income_real[0])) : 1;
  const soloOut = c ? year >= c.shadow_runout_year : false;

  useEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const ctx = el.getContext('2d');
    if (!ctx) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const cell = 7;
    const gap = 3;
    const towerGap = 14;
    const width = TOWERS.reduce((w, [cols]) => w + cols * (cell + gap), 0) + towerGap * (TOWERS.length - 1);
    const height = Math.max(...TOWERS.map(([, rows]) => rows)) * (cell + gap);
    el.width = width * dpr;
    el.height = height * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const dark = 2000 - alive;
    let w = 0;
    let x0 = 0;
    for (const [cols, rows] of TOWERS) {
      const top = height - rows * (cell + gap);
      ctx.fillStyle = '#101d35';
      ctx.fillRect(x0 - 2, top - 4, cols * (cell + gap) + 1, rows * (cell + gap) + 4);
      for (let r = 0; r < rows; r++) {
        for (let k = 0; k < cols; k++) {
          const lit = rank[w] >= dark;
          const x = x0 + k * (cell + gap);
          const y = top + r * (cell + gap);
          if (lit) {
            ctx.fillStyle = `rgba(255,178,63,${0.1 * bright})`;
            ctx.fillRect(x - 2, y - 2, cell + 4, cell + 4);
            const warm = Math.min(1, (bright - 0.8) / 1.8);
            ctx.fillStyle = `rgb(255,${Math.round(178 + 60 * warm)},${Math.round(63 + 120 * warm)})`;
          } else {
            ctx.fillStyle = '#1c3157';
          }
          ctx.fillRect(x, y, cell, cell);
          w++;
        }
      }
      x0 += cols * (cell + gap) + towerGap;
    }
  }, [alive, bright, rank]);

  return (
    <section className="dusk bg-night text-foreground" aria-labelledby="lights-title">
      <div className="mx-auto max-w-6xl px-4 pt-24 md:px-8">
        <h2 id="lights-title" className="max-w-[22ch] text-title font-extrabold text-white">
          {t.lights.title}
        </h2>
        <p className="mt-5 max-w-[62ch] text-body-l text-mist">{t.lights.lead}</p>
      </div>
      <div ref={wrap} className={reduce ? '' : 'h-[260vh]'}>
        <div className={`${reduce ? '' : 'sticky top-0'} flex min-h-[100svh] items-center`}>
          <div className="mx-auto grid w-full max-w-6xl items-end gap-10 px-4 py-16 md:px-8 lg:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]">
            <div className="relative">
              <canvas ref={canvas} className="h-auto w-full" role="img" aria-label={f(t.lights.alive, { count: num(locale, alive) })} />
              <div className="mt-3 h-px w-full bg-white/10" aria-hidden="true" />
            </div>
            <div className="space-y-6">
              <p className="font-mono text-[clamp(3rem,8vw,5rem)] leading-none font-bold text-white tabular" aria-live="polite">
                <span className="sr-only">{t.lights.year} </span>
                {year}
              </p>
              <dl className="space-y-3 text-body-l">
                <div>
                  <dt className="sr-only">{t.lights.alive}</dt>
                  <dd className="text-white">{f(t.lights.alive, { count: num(locale, alive) })}</dd>
                </div>
                <div>
                  <dt className="sr-only">{t.lights.income}</dt>
                  <dd className="min-h-[1.5em] text-lamp">{c ? f(t.lights.income, { amount: usd(locale, income, 0) }) : ''}</dd>
                </div>
              </dl>
              <div className="flex items-start gap-3 rounded-2xl bg-bay/70 p-4 ring-1 ring-white/10">
                <span className={`mt-1 size-4 shrink-0 rounded-full transition-colors duration-500 ${soloOut ? 'bg-bay-3' : 'bg-lamp shadow-[0_0_14px_3px_rgba(255,178,63,0.6)]'}`} aria-hidden="true" />
                <p className="text-body text-foreground/90">
                  {c ? f(t.lights.rule, { year: c.shadow_runout_year, share: pct(locale, c.shadow_alive_share) }) : ''}
                </p>
              </div>
              <p className="text-body text-white">{t.lights.pool}</p>
              <label className="block">
                <span className="mb-2 block text-caption text-mist">{t.lights.scrub}</span>
                <input type="range" min={START} max={START + years} value={year} onChange={(e) => setYear(Number(e.target.value))} className="w-full accent-lamp" />
              </label>
              <p className="text-caption text-mist">{f(t.lights.replay, { year: START })}</p>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
