import { BadgeCheck, Clock3, FileCode2, Fingerprint, Hourglass, Radar, RotateCcw } from 'lucide-react';
import type { Dict, Locale } from '@/i18n';
import { format as f } from '@/i18n';
import { coreLive, dep, explorer, CALL_GAS_CAP } from '@/lib/chain';
import { cohortCount, countryCount } from '@/lib/cohorts';
import { canSponsor } from '@/lib/wallet/sponsor';
import { num, pct, short } from '@/lib/format';
import gas from '@/sdk/gas.json';
import fair100 from '@/sdk/fairness-100.json';
import mutations from '@/sdk/mutations-pool.json';
import registryMutations from '@/sdk/mutations-registry.json';
import { Wordmark } from '@/components/Nav';

const SKY = {
  morning: 'linear-gradient(90deg,#9fc3e6,#dbe8f4)',
  noon: 'linear-gradient(90deg,#dbe8f4,#f6f1d8)',
  dusk: 'linear-gradient(90deg,#ffd48a,#f2785c)',
  night: 'linear-gradient(90deg,#26406b,#0d1a30)',
} as const;

/** How it works, as one day: morning (saving), noon (invested), dusk (income), night (estate). */
export function Day({ t }: { t: Dict }) {
  const steps = (['morning', 'noon', 'dusk', 'night'] as const).map((k) => ({
    k,
    label: t.how[k],
    title: t.how[`${k}Title` as const],
    body: t.how[`${k}Body` as const],
  }));
  return (
    <section id="how" className="bg-haze text-ink" aria-labelledby="how-title">
      <div className="mx-auto max-w-6xl px-4 py-24 md:px-8">
        <h2 id="how-title" className="max-w-[20ch] text-title font-extrabold">{t.how.title}</h2>
        <ol className="mt-14 grid gap-10 md:grid-cols-4 md:gap-0">
          {steps.map((s) => (
            <li key={s.k} className="relative pl-8 md:pl-0">
              <div className="absolute top-1 bottom-[-2.5rem] left-0 w-2 rounded-full md:static md:h-2 md:w-full md:rounded-none md:first:rounded-l-full md:last:rounded-r-full" style={{ background: SKY[s.k] }} aria-hidden="true" />
              <div className="md:pt-6 md:pr-8">
                <h3 className="text-heading font-bold">
                  <span className="font-normal text-ink-2">{s.label}.</span> {s.title}
                </h3>
                <p className="mt-3 text-body text-ink-2">{s.body}</p>
              </div>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

/** Why nobody can take the pot, each claim linked to the contract that enforces it. */
export function Safety({ t }: { t: Dict }) {
  const items = [
    { icon: FileCode2, text: t.safe.code, at: dep.treasury ?? dep.pool },
    { icon: Fingerprint, text: canSponsor ? t.safe.alive : t.safe.aliveRelay, at: dep.lifeRegistry },
    { icon: BadgeCheck, text: t.safe.identity, at: dep.attestedIdentity },
    { icon: RotateCcw, text: t.safe.report, at: dep.lifeRegistry },
    { icon: Clock3, text: coreLive ? t.safe.rules : t.safe.rulesSoon, at: (dep as { timelock?: string }).timelock ?? dep.actuary },
    { icon: Radar, text: t.safe.ghosts, at: dep.pool },
  ];
  return (
    <section id="safe" className="bg-paper text-ink" aria-labelledby="safe-title">
      <div className="mx-auto grid max-w-6xl gap-12 px-4 py-24 md:px-8 lg:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
        <div>
          <h2 id="safe-title" className="max-w-[18ch] text-title font-extrabold">{t.safe.title}</h2>
          <p className="mt-6 max-w-[52ch] text-body-l text-ink-2">{t.safe.lead}</p>
          {!coreLive && (
            <p className="mt-6 flex max-w-[52ch] items-start gap-3 text-body text-ink">
              <Hourglass className="mt-0.5 size-5 shrink-0 text-ink-2" aria-hidden="true" />
              {t.safe.pending}
            </p>
          )}
        </div>
        <ul className="divide-y divide-border">
          {items.map(({ icon: Icon, text, at }) => (
            <li key={text} className="flex gap-4 py-5 first:pt-0">
              <Icon className="mt-0.5 size-6 shrink-0 text-ink" strokeWidth={1.75} aria-hidden="true" />
              <div className="space-y-1.5">
                <p className="text-body-l">{text}</p>
                {at && (
                  <a href={explorer('address', at)} target="_blank" rel="noreferrer" className="inline-flex min-h-11 items-center font-mono text-caption text-ink-2 underline decoration-ink/25 underline-offset-4 hover:text-ink hover:decoration-ink">
                    {t.safe.see} ({short(at)})
                  </a>
                )}
              </div>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

/** Measured, not claimed: every number here comes from a run or the chain (web/scripts/sync.mjs). */
export function Hood({ t, locale }: { t: Dict; locale: Locale }) {
  const q512 = (gas.quote as { paths: number; escalating: boolean; gas: number }[]).find((r) => r.paths === 512 && !r.escalating)?.gas ?? 0;
  const rows = [
    f(t.hood.quote, { gas: num(locale, q512), share: pct(locale, q512 / CALL_GAS_CAP) }),
    f(coreLive ? t.hood.mortality : t.hood.mortalitySoon, { cohorts: num(locale, cohortCount), countries: countryCount }),
    f(t.hood.fair, { after: new Intl.NumberFormat(locale === 'fil' ? 'fil-PH' : 'en-SG', { style: 'percent', maximumFractionDigits: 2 }).format(fair100.rms_bias), before: pct(locale, fair100.uncorrected_rms_bias) }),
    f(t.hood.tests, { killed: mutations.killed + registryMutations.killed, total: mutations.total + registryMutations.total }),
    t.hood.stylus,
  ];
  return (
    <section className="dusk bg-bay text-foreground" aria-labelledby="hood-title">
      <div className="mx-auto max-w-6xl px-4 py-20 md:px-8">
        <h2 id="hood-title" className="text-heading font-bold text-white">{t.hood.title}</h2>
        <ul className="mt-8 grid gap-x-10 gap-y-6 md:grid-cols-2">
          {rows.map((r) => (
            <li key={r} className="border-l-2 border-lamp/60 pl-4 text-body text-foreground/90">{r}</li>
          ))}
        </ul>
      </div>
    </section>
  );
}

export function Limits({ t }: { t: Dict }) {
  return (
    <section className="bg-haze text-ink" aria-labelledby="limits-title">
      <div className="mx-auto max-w-6xl px-4 py-20 md:px-8">
        <h2 id="limits-title" className="text-heading font-bold">{t.limits.title}</h2>
        <ul className="mt-6 grid max-w-[70ch] gap-4 text-body text-ink-2">
          {[...(coreLive ? [] : [t.limits.v1]), t.limits.preview, t.limits.license, t.limits.identity, t.limits.usdg].map((x) => (
            <li key={x} className="flex gap-3">
              <span className="mt-2.5 size-1.5 shrink-0 rounded-full bg-ink/40" aria-hidden="true" />
              {x}
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

export function Cta({ t, locale }: { t: Dict; locale: Locale }) {
  return (
    <section className="dusk relative overflow-hidden bg-bay text-foreground" aria-labelledby="cta-title">
      <div className="absolute inset-x-0 bottom-0 h-40 bg-gradient-to-t from-coral/25 to-transparent" aria-hidden="true" />
      <div className="relative mx-auto max-w-6xl px-4 py-24 text-center md:px-8">
        <h2 id="cta-title" className="text-title font-extrabold text-white">{t.cta.title}</h2>
        <div className="mt-8 flex flex-col items-center justify-center gap-3 sm:flex-row">
          <a href={`/${locale}#ask`} className="press inline-flex min-h-14 items-center justify-center rounded-xl bg-lamp px-6 text-body-l font-extrabold text-ink hover:bg-lamp-soft">{t.cta.her}</a>
          <a href={`/${locale}?who=me#ask`} className="press inline-flex min-h-14 items-center justify-center rounded-xl px-6 text-body-l font-bold text-white ring-1 ring-white/25 hover:bg-white/10">{t.cta.me}</a>
        </div>
      </div>
    </section>
  );
}

export function Footer({ t }: { t: Dict }) {
  const contracts = [
    ['Actuary', dep.actuary],
    ['TontiPool', dep.pool],
    ['Treasury', dep.treasury],
    ['LifeRegistry', dep.lifeRegistry],
    ['AttestedIdentity', dep.attestedIdentity],
    ['Timelock', (dep as { timelock?: string }).timelock],
  ].filter((c): c is [string, string] => !!c[1]);
  const repo = process.env.NEXT_PUBLIC_REPO_URL;
  return (
    <footer className="dusk bg-night text-foreground">
      <div className="mx-auto grid max-w-6xl gap-10 px-4 py-16 md:grid-cols-[1fr_1.4fr] md:px-8">
        <div className="space-y-4">
          <Wordmark />
          <p className="max-w-[40ch] text-caption text-mist">{t.footer.disclaimer}</p>
          <p className="text-caption text-mist">{t.footer.built}</p>
          {repo && (
            <p className="flex gap-4 text-caption">
              <a className="underline decoration-white/30 underline-offset-4 hover:decoration-white" href={repo}>{t.footer.source}</a>
              <a className="underline decoration-white/30 underline-offset-4 hover:decoration-white" href={`${repo}/blob/main/docs/protocol.md`}>{t.footer.spec}</a>
            </p>
          )}
        </div>
        <div>
          <h2 className="text-caption font-bold text-mist">{t.footer.contracts}</h2>
          <ul className="mt-3 grid gap-1 sm:grid-cols-2">
            {contracts.map(([name, at]) => (
              <li key={name}>
                <a href={explorer('address', at)} target="_blank" rel="noreferrer" className="flex min-h-11 items-center justify-between gap-3 rounded-lg px-2 font-mono text-caption hover:bg-white/5">
                  <span className="text-foreground">{name}</span>
                  <span className="text-mist">{short(at)}</span>
                </a>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </footer>
  );
}
