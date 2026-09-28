import { notFound } from 'next/navigation';
import { actuaryAbi } from '@/sdk/abi.ts';
import { format as f, getDictionary, hasLocale } from '@/i18n';
import { coreLive, dep, explorer } from '@/lib/chain';
import { app, pub } from '@/lib/app-chain';
import { date as fmtDate, num, short, usd } from '@/lib/format';
import { cohortCount, countryCount, oddsAt65 } from '@/lib/odds';
import { Notice } from '@/components/Notice';
import { OddsChart } from '@/components/OddsChart';

export const revalidate = 30;

/** The pool, read from the chain on every visit (cached for 30 seconds). */
export default async function PoolPage({ params }: PageProps<'/[locale]/pool'>) {
  const { locale } = await params;
  if (!hasLocale(locale)) notFound();
  const t = getDictionary(locale);
  const [sealed, odds, live] = await Promise.all([
    pub.readContract({ address: dep.actuary, abi: actuaryAbi, functionName: 'isSealed' }).catch(() => null),
    oddsAt65(pub, dep.actuary).catch(() => null),
    coreLive ? Promise.all([app.counts(), app.state(), app.holdings().catch(() => null)]).catch(() => null) : null,
  ]);
  const contracts = [
    ['Actuary', dep.actuary],
    ['TontiPool', dep.pool],
    ['Treasury', dep.treasury],
    ['LifeRegistry', dep.lifeRegistry],
    ['AttestedIdentity', dep.attestedIdentity],
    ['Timelock', (dep as { timelock?: string }).timelock],
  ].filter((c): c is [string, string] => !!c[1]);
  const stat = (label: string, value: string) => (
    <div className="rounded-2xl bg-paper p-5 shadow-sm">
      <dt className="text-caption text-ink-2">{label}</dt>
      <dd className="mt-1 font-mono text-title font-bold tabular">{value}</dd>
    </div>
  );
  return (
    <div className="mx-auto max-w-4xl px-4 pt-10 md:px-8 md:pt-14">
      <h1 className="text-title font-extrabold text-balance">{live ? t.pool.title : t.pool.titleSoon}</h1>
      <p className="mt-3 text-body-l text-ink-2">{t.pool.lead}</p>
      {!live && <Notice className="mt-6">{t.pool.notLive}</Notice>}
      <p className="mt-6 text-body">{f(t.pool.actuary, { cohorts: num(locale, cohortCount), countries: countryCount, sealed: sealed ? t.pool.yes : t.pool.no })}</p>
      {live && (
        <>
          <dl className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {stat(t.pool.members, num(locale, live[0].members))}
            {stat(t.pool.cohorts, num(locale, live[0].cohorts))}
            {stat(t.pool.epoch, num(locale, live[1].epoch))}
            {stat(t.pool.next, live[1].nextSettle ? fmtDate(locale, live[1].nextSettle) : t.pool.nextFirst)}
          </dl>
          {live[2] && (
            <section className="mt-10">
              <h2 className="text-heading font-bold">{t.pool.holdings}</h2>
              <dl className="mt-4 grid gap-4 sm:grid-cols-3">
                {stat(t.pool.cash, usd(locale, live[2].dollars.cash))}
                {stat(t.pool.sgov, usd(locale, live[2].dollars.sgov))}
                {stat(t.pool.spy, usd(locale, live[2].dollars.spy))}
              </dl>
              <p className="mt-2 text-caption text-ink-2">{f(t.pool.pricedAt, { date: fmtDate(locale, live[2].pricedAt) })}</p>
            </section>
          )}
          <p className="mt-6 text-body">{t.pool.phase}: <span className="font-mono">{live[1].phase === 'idle' ? t.pool.idle : live[1].phase}</span></p>
        </>
      )}
      <section className="mt-12" aria-labelledby="odds-title">
        <h2 id="odds-title" className="text-heading font-bold text-balance">{t.pool.oddsTitle}</h2>
        <p className="mt-2 max-w-[62ch] text-body text-ink-2">{t.pool.oddsLead}</p>
        <div className="mt-5 rounded-2xl bg-paper p-3 shadow-sm sm:p-5">
          {odds ? (
            <OddsChart rows={odds.rows} locale={locale} labels={{ women: t.pool.women, men: t.pool.men, row: (country, female, male) => f(t.pool.oddsRow, { country, female, male }) }} />
          ) : (
            <p className="p-2 text-body text-ink-2">{t.pool.oddsDown}</p>
          )}
        </div>
        {odds && (
          <p className="mt-3 max-w-[70ch] text-caption text-ink-2">
            {f(t.pool.oddsHow, { block: num(locale, odds.block), calls: num(locale, odds.calls), batches: odds.batches })}
          </p>
        )}
      </section>
      <section className="mt-12">
        <h2 className="text-heading font-bold">{t.pool.contracts}</h2>
        <ul className="mt-4 divide-y divide-border rounded-2xl bg-paper shadow-sm">
          {contracts.map(([name, at]) => (
            <li key={name}>
              <a href={explorer('address', at)} target="_blank" rel="noreferrer" className="flex min-h-14 items-center justify-between gap-3 px-5 font-mono text-body hover:bg-haze">
                <span>{name}</span>
                <span className="text-ink-2">{short(at)}</span>
              </a>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
