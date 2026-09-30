'use client';

import { useEffect, useMemo, useState } from 'react';
import dynamic from 'next/dynamic';
import { countries } from '@/sdk/countries.ts';
import type { Quote } from '@/sdk/quote.ts';
import type { Iso3, Sex } from '@/sdk/units.ts';
import { Sky } from '@/components/sky/Sky';
import { LampChart } from '@/components/quote/LampChart';
import { ChainPulse } from '@/components/ChainPulse';
import { useI18n } from '@/i18n/client';
import { useReducedMotion } from '@/lib/motion';
import { toQuery, toSlug } from '@/lib/slug';

// Only shown after an answer: loaded with it.
const RunItYourself = dynamic(() => import('@/components/quote/RunItYourself').then((x) => x.RunItYourself));
import { CURRENCY, countryName, money, num, pct, usd } from '@/lib/format';

type Who = 'mother' | 'father' | 'me';
type Answer = { q: Quote; survival: { age: number; alive: number }[]; block: bigint; seconds: number; p50Raw: bigint; startAge: number; who: Who; country: Iso3 };
type State = { kind: 'idle' } | { kind: 'asking' } | { kind: 'done'; a: Answer } | { kind: 'error'; message: string };

const YEAR = new Date().getUTCFullYear();
/** Quotes are dated at the start of the day (UTC): the same inputs give the same answer all day. */
const today = () => new Date(Math.floor(Date.now() / 86_400_000) * 86_400_000);

// ISO 3166 alpha-2 (the proxy's `cc` cookie, from Vercel's geolocation) to the Actuary's alpha-3.
const FROM_ISO2: Record<string, Iso3> = { PH: 'PHL', ID: 'IDN', IN: 'IND', BD: 'BGD', MM: 'MMR', LK: 'LKA', NP: 'NPL', VN: 'VNM', TH: 'THA', MY: 'MYS', CN: 'CHN', GH: 'GHA' };

export function Hero() {
  const { t, f, locale } = useI18n();
  const reduce = useReducedMotion();
  const [who, setWho] = useState<Who>('mother');
  const [meSex, setMeSex] = useState<Sex>('female');
  const [country, setCountry] = useState<Iso3>('PHL');
  const [born, setBorn] = useState(1966);
  const [startAge, setStartAge] = useState(62);
  const [lump, setLump] = useState(3000);
  const [monthly, setMonthly] = useState(30);
  const [state, setState] = useState<State>({ kind: 'idle' });
  const [fx, setFx] = useState<{ rates: Record<string, number>; date: string } | null>(null);

  const sex: Sex = who === 'mother' ? 'female' : who === 'father' ? 'male' : meSex;
  const age = YEAR - born;
  const minStart = Math.max(50, Math.ceil(age + 0.5));
  // Adults whose start age can still be ≤ 80, and only birth years the Actuary has mortality for.
  const fitted = countries.find((x) => x.iso3 === country)?.birthYears ?? [YEAR - 80, YEAR - 18];
  // Born 80 years ago, income could only start at 81: the oldest who can join is 79.
  const bornMin = Math.max(YEAR - 79, fitted[0]);
  const bornMax = Math.min(YEAR - 18, fitted[1]);

  useEffect(() => {
    if (startAge < minStart) setStartAge(Math.min(80, minStart));
  }, [minStart, startAge]);
  useEffect(() => {
    fetch('/api/fx').then((r) => (r.ok ? r.json() : null)).then(setFx).catch(() => setFx(null));
    // A visitor in one of the priced countries starts with their own; everyone else (Singapore
    // included, where the workers are) with the Philippines, the largest group.
    const cc = /(?:^|; )cc=([A-Z]{2})/.exec(document.cookie)?.[1];
    if (cc && FROM_ISO2[cc]) setCountry(FROM_ISO2[cc]);
    // A shared or linked question arrives in the query string (?w=&s=&c=&b=&a=&l=&m=).
    const p = new URLSearchParams(window.location.search);
    const w = p.get('w');
    if (w === 'mother' || w === 'father' || w === 'me') setWho(w);
    if (p.get('s') === 'm') setMeSex('male');
    const c = p.get('c');
    if (c && countries.some((x) => x.iso3 === c)) setCountry(c as Iso3);
    const n = (k: string) => (p.get(k) !== null && Number.isFinite(Number(p.get(k))) ? Number(p.get(k)) : null);
    if (n('b') !== null) setBorn(n('b')!);
    if (n('a') !== null) setStartAge(n('a')!);
    // The same caps as typing (and the Actuary's own): a link can't ask for more.
    if (n('l') !== null) setLump(Math.min(Math.max(0, n('l')!), 10_000_000));
    if (n('m') !== null) setMonthly(Math.min(Math.max(0, n('m')!), 100_000));
  }, []);

  const title = who === 'mother' ? t.hero.title : who === 'father' ? t.hero.titleFather : t.hero.titleMe;
  const lead = who === 'mother' ? t.hero.lead : who === 'father' ? t.hero.leadFather : t.hero.leadMe;

  const ask = async (e: React.FormEvent) => {
    e.preventDefault();
    setState({ kind: 'asking' });
    try {
      // The chain client loads with the first question, not with the page.
      const [{ quote, survival }, { client, dep }, { getBlockNumber }] = await Promise.all([import('@/sdk/quote.ts'), import('@/lib/chain'), import('viem/actions')]);
      const now = today();
      let seconds = 0;
      const timed = async () => {
        const t0 = performance.now();
        const q = await quote(client, dep.actuary, { country, sex, birthYear: born, startAge, lumpSum: lump, monthly, now });
        seconds = (performance.now() - t0) / 1000;
        return q;
      };
      const [q, curve, block] = await Promise.all([timed(), survival(client, dep.actuary, country, sex, born, startAge), getBlockNumber(client, { cacheTime: 0 })]);
      setState({ kind: 'done', a: { q, survival: curve, block, seconds, p50Raw: q.raw[1], startAge, who, country } });
    } catch (err) {
      const { BaseError, ContractFunctionRevertedError } = await import('viem');
      const revert = err instanceof BaseError ? err.walk((x) => x instanceof ContractFunctionRevertedError) : null;
      // Only a failure to reach the chain is blamed on the chain; anything else is the question.
      const message = revert instanceof ContractFunctionRevertedError ? f(t.result.errorInput, { detail: revert.data?.errorName ?? revert.shortMessage }) : err instanceof BaseError ? t.result.error : f(t.result.errorInput, { detail: (err as Error).message });
      setState({ kind: 'error', message });
    }
  };

  const a = state.kind === 'done' ? state.a : null;
  const aliveAtRunout = useMemo(() => (a ? (a.survival.find((s) => s.age >= a.q.soloRunoutAge)?.alive ?? a.q.soloOutliveProbability) : null), [a]);
  const sun = a ? Math.max(0.08, Math.min(0.95, aliveAtRunout ?? 0.5)) : 0.82;
  const cur = a ? CURRENCY[a.country] : undefined;
  const rate = cur && fx?.rates[cur];

  return (
    <section id="ask" className="dusk relative isolate overflow-hidden bg-bay text-foreground" aria-labelledby="hero-title">
      {/* The sky fills the first screen; below it, night. */}
      <Sky sun={sun} className="absolute inset-x-0 top-0 -z-10 h-[100svh] min-h-[620px]" />
      <div className="absolute inset-x-0 top-[max(100svh,620px)] bottom-0 -z-10 bg-night" aria-hidden="true" />
      <div className="mx-auto grid max-w-6xl gap-10 px-4 pt-28 pb-16 md:px-8 lg:grid-cols-[minmax(0,1.08fr)_minmax(0,0.92fr)] lg:gap-12 lg:pt-36 lg:pb-24">
        <div className="max-w-[40rem]">
          <h1 id="hero-title" className="text-title font-extrabold tracking-[-0.015em] text-white md:text-display">
            {title}
          </h1>
          <p className="mt-5 max-w-[34rem] text-body-l text-mist">{lead}</p>
          <form onSubmit={ask} className="mt-8 grid gap-5 rounded-2xl bg-night/80 p-4 ring-1 ring-white/12 sm:p-6" noValidate>
            <fieldset>
              <legend className="mb-2 text-caption font-bold text-mist">{t.form.who}</legend>
              <div className="grid grid-cols-3 gap-2" role="radiogroup">
                {(['mother', 'father', 'me'] as const).map((w) => (
                  <label key={w} className={`press flex min-h-12 cursor-pointer items-center justify-center rounded-xl px-1 text-center text-body leading-tight font-bold ring-1 transition-colors duration-150 ${who === w ? 'bg-lamp text-ink ring-lamp' : 'bg-white/5 text-foreground ring-white/15 hover:bg-white/10'}`}>
                    <input type="radio" name="who" value={w} checked={who === w} onChange={() => setWho(w)} className="sr-only" />
                    {t.form[w]}
                  </label>
                ))}
              </div>
            </fieldset>
            {who === 'me' && (
              <fieldset>
                <legend className="mb-2 text-caption font-bold text-mist">{t.form.sex}</legend>
                <div className="grid grid-cols-2 gap-2">
                  {(['female', 'male'] as const).map((s) => (
                    <label key={s} className={`press flex min-h-12 cursor-pointer items-center justify-center rounded-xl text-body font-bold ring-1 ${meSex === s ? 'bg-white text-ink ring-white' : 'bg-white/5 ring-white/15 hover:bg-white/10'}`}>
                      <input type="radio" name="sex" value={s} checked={meSex === s} onChange={() => setMeSex(s)} className="sr-only" />
                      {s === 'female' ? t.form.woman : t.form.man}
                    </label>
                  ))}
                </div>
              </fieldset>
            )}
            <div className="grid gap-5 sm:grid-cols-2">
              <Field id="country" label={who === 'me' ? t.form.countryMe : who === 'father' ? t.form.countryFather : t.form.country}>
                <select id="country" value={country} onChange={(e) => setCountry(e.target.value as Iso3)} className="min-h-12 w-full rounded-xl bg-white/8 px-3 text-body text-foreground ring-1 ring-white/20 outline-none focus-visible:ring-2 focus-visible:ring-lamp">
                  {[...countries].sort((x, y) => countryName(locale, x.iso3, x.name).localeCompare(countryName(locale, y.iso3, y.name))).map((c) => (
                    <option key={c.iso3} value={c.iso3} className="bg-bay text-foreground">
                      {countryName(locale, c.iso3, c.name)}
                    </option>
                  ))}
                </select>
              </Field>
              <Stepper id="born" label={t.form.born} value={born} min={bornMin} max={bornMax} onChange={setBorn} hint={born > bornMax ? f(bornMax < YEAR - 18 ? t.form.notFitted : t.form.tooYoung, { year: bornMax }) : born < bornMin ? f(t.form.tooOld, { year: bornMin }) : undefined} />
              <Stepper id="start" label={t.form.startAge} value={startAge} min={minStart} max={80} onChange={setStartAge} hint={f(t.form.startAgeHint, { min: minStart })} />
              <Money id="lump" label={t.form.lump} value={lump} onChange={setLump} />
              <Money id="monthly" label={t.form.monthly} value={monthly} onChange={setMonthly} hint={t.form.monthlyHint} />
            </div>
            <button type="submit" disabled={state.kind === 'asking' || born < bornMin || born > bornMax || (lump <= 0 && monthly <= 0)} className="press inline-flex min-h-14 w-full items-center justify-center gap-3 rounded-xl bg-lamp px-5 text-body-l font-extrabold text-ink shadow-[0_8px_30px_-8px_rgba(255,178,63,0.55)] hover:bg-lamp-soft disabled:cursor-wait disabled:opacity-80">
              {state.kind === 'asking' ? (
                <>
                  <Orbit />
                  <span>{t.form.asking}</span>
                </>
              ) : (
                t.form.ask
              )}
            </button>
            {lump <= 0 && monthly <= 0 && <p className="-mt-2 text-caption text-mist">{t.form.nothingPaid}</p>}
            <ChainPulse />
          </form>
        </div>

        <div className="lg:pt-6" aria-live="polite">
            {a && (
              <div key={`${a.block}-${a.q.incomeStart.p50}`} className={`space-y-6 rounded-3xl bg-night/88 p-5 ring-1 ring-white/10 sm:p-7 ${reduce ? 'animate-in fade-in duration-200' : 'animate-in fade-in slide-in-from-bottom-3 duration-500 ease-[cubic-bezier(0.23,1,0.32,1)]'}`}>
                <div>
                  <p className="font-mono text-[clamp(2.75rem,11vw,4.75rem)] leading-none font-bold tracking-[-0.03em] break-all text-lamp tabular lg:text-[clamp(3rem,4.6vw,4.75rem)]" style={{ textShadow: '0 0 36px rgba(255,178,63,0.35)' }}>
                    {usd(locale, a.q.incomeStart.p50)}
                  </p>
                  <div className="mt-2 flex flex-wrap items-baseline gap-x-4">
                    <span className="text-body-l font-bold text-white">{f(t.result.perMonth, { amount: '' }).trim()}</span>
                    {/* Handwritten once (Tegaki, Tillana), pre-rendered to SVG by scripts/handwriting.mjs. */}
                    {/* eslint-disable-next-line @next/next/no-img-element */}
                    <img src={`/hand/${locale === 'fil' ? 'habambuhay' : 'for-life'}-${reduce ? 'static' : 'once'}.svg`} alt={t.result.forLife} className="h-12 w-auto translate-y-[38%] sm:h-14" />
                  </div>
                </div>
                <div className="max-w-[34rem] space-y-2 text-body text-foreground/90">
                  <p>{f(t.result.range, { low: usd(locale, a.q.incomeStart.p10), high: usd(locale, a.q.incomeStart.p90) })}</p>
                  {rate ? <p>{f(t.result.local, { amount: money(locale, a.q.incomeStart.p50 * rate, cur!), currency: cur! })} <span className="text-caption text-mist">{f(t.result.fxSource, { date: fx!.date })}</span></p> : null}
                  <p className="text-white">
                    {f(a.who === 'me' ? t.result.soloMe : a.who === 'father' ? t.result.soloHe : t.result.soloShe, {
                      amount: usd(locale, a.q.incomeStart.p50),
                      age: num(locale, a.q.soloRunoutAge, 0),
                      chance: pct(locale, aliveAtRunout ?? a.q.soloOutliveProbability),
                    })}
                  </p>
                </div>
                <div className="rounded-2xl bg-night/40 p-3 ring-1 ring-white/10 sm:p-4">
                  <LampChart
                    startAge={a.startAge}
                    p10={a.q.incomeStart.p10}
                    p50={a.q.incomeStart.p50}
                    p90={a.q.incomeStart.p90}
                    soloRunoutAge={a.q.soloRunoutAge}
                    survival={a.survival}
                    labels={{
                      title: a.who === 'me' ? t.result.chartTitleMe : t.result.chartTitle,
                      pool: t.result.chartPool,
                      alone: t.result.chartAlone,
                      alive: t.result.chartAlive,
                      runsOut: f(t.result.chartRunsOut, { age: num(locale, a.q.soloRunoutAge, 0) }),
                      aliveAt: f(t.result.chartAliveAt, { chance: pct(locale, aliveAtRunout ?? 0) }),
                      summary: f(t.result.chartSummary, { age: num(locale, a.q.soloRunoutAge, 0) }),
                    }}
                  />
                </div>
                <p className="text-caption text-mist">{f(t.result.computedAt, { seconds: num(locale, a.seconds, 1), block: num(locale, a.block) })}</p>
                <a href={`/${locale}/join${toQuery({ who: a.who, sex, country: a.country, born, startAge: a.startAge, lump, monthly })}`} className="press inline-flex min-h-14 w-full items-center justify-center rounded-xl bg-lamp px-5 text-body-l font-extrabold text-ink hover:bg-lamp-soft">
                  {a.who === 'me' ? t.result.startMe : a.who === 'father' ? t.result.startHim : t.result.startHer}
                </a>
                <RunItYourself call={a.q.call} p50Raw={a.p50Raw} />
                <ShareQuote slug={toSlug({ who: a.who, sex, country: a.country, born, startAge: a.startAge, lump, monthly })} label={t.result.share} copied={t.result.copied} />
                <p className="text-caption text-mist">{t.result.preview}</p>
              </div>
            )}
          {state.kind === 'error' && (
            <p role="alert" className="rounded-xl bg-night/60 p-4 text-body text-white ring-1 ring-white/15">
              {state.message}
            </p>
          )}
        </div>
      </div>
    </section>
  );
}

function Field({ id, label, hint, children }: { id: string; label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div>
      <label htmlFor={id} className="mb-2 block text-caption font-bold text-mist">
        {label}
      </label>
      {children}
      {hint && (
        <p id={`${id}-hint`} className="mt-1.5 text-caption text-mist/90">
          {hint}
        </p>
      )}
    </div>
  );
}

function Stepper({ id, label, value, min, max, onChange, hint }: { id: string; label: string; value: number; min: number; max: number; onChange: (v: number) => void; hint?: string }) {
  const btn = 'press inline-flex size-12 shrink-0 items-center justify-center rounded-xl bg-white/8 text-2xl font-bold ring-1 ring-white/20 hover:bg-white/14 disabled:opacity-40';
  return (
    <Field id={id} label={label} hint={hint}>
      <div className="flex items-center gap-2">
        <button type="button" className={btn} onClick={() => onChange(Math.max(min, value - 1))} disabled={value <= min} aria-label={`${label} −1`}>
          −
        </button>
        <input
          id={id}
          inputMode="numeric"
          pattern="[0-9]*"
          value={value}
          aria-describedby={hint ? `${id}-hint` : undefined}
          onChange={(e) => {
            const v = Number(e.target.value.replace(/\D/g, ''));
            if (Number.isFinite(v)) onChange(v);
          }}
          onBlur={() => onChange(Math.min(max, Math.max(min, value)))}
          className="h-12 w-full min-w-0 rounded-xl bg-white/8 text-center font-mono text-body-l font-bold text-foreground ring-1 ring-white/20 outline-none tabular focus-visible:ring-2 focus-visible:ring-lamp"
        />
        <button type="button" className={btn} onClick={() => onChange(Math.min(max, value + 1))} disabled={value >= max} aria-label={`${label} +1`}>
          +
        </button>
      </div>
    </Field>
  );
}

function Money({ id, label, value, onChange, hint }: { id: string; label: string; value: number; onChange: (v: number) => void; hint?: string }) {
  return (
    <Field id={id} label={label} hint={hint}>
      <div className="relative">
        <span className="pointer-events-none absolute inset-y-0 left-3 flex items-center font-mono text-body-l text-mist" aria-hidden="true">
          $
        </span>
        <input
          id={id}
          inputMode="decimal"
          value={value.toLocaleString('en-US')}
          aria-describedby={hint ? `${id}-hint` : undefined}
          onChange={(e) => {
            const v = Number(e.target.value.replace(/[^0-9.]/g, ''));
            if (Number.isFinite(v)) onChange(Math.min(v, id === 'lump' ? 10_000_000 : 100_000));
          }}
          className="h-12 w-full rounded-xl bg-white/8 pr-3 pl-8 font-mono text-body-l font-bold text-foreground ring-1 ring-white/20 outline-none tabular focus-visible:ring-2 focus-visible:ring-lamp"
        />
      </div>
    </Field>
  );
}

/** Computing on-chain: a small orbit, not a spinner-shaped cliché. Still under reduced motion. */
function Orbit() {
  return (
    <svg viewBox="0 0 24 24" className="size-6 motion-safe:animate-spin [animation-duration:1.6s]" aria-hidden="true">
      <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeOpacity="0.25" strokeWidth="2" />
      <circle cx="12" cy="3" r="2.6" fill="currentColor" />
    </svg>
  );
}

/** Shares the question as a link whose preview shows the answer (Messenger, WhatsApp, Viber). */
function ShareQuote({ slug, label, copied }: { slug: string; label: string; copied: string }) {
  const { locale } = useI18n();
  const [done, setDone] = useState(false);
  const share = async () => {
    const url = `${window.location.origin}/${locale}/q/${slug}`;
    if (navigator.share) {
      await navigator.share({ url }).catch(() => undefined);
      return;
    }
    await navigator.clipboard.writeText(url);
    setDone(true);
    setTimeout(() => setDone(false), 1800);
  };
  return (
    <button type="button" onClick={share} className="press inline-flex min-h-12 items-center justify-center gap-2 rounded-xl px-4 font-bold text-white ring-1 ring-white/20 hover:bg-white/10">
      {done ? copied : label}
    </button>
  );
}
