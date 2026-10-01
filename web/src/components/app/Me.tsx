'use client';

import { useCallback, useEffect, useState } from 'react';
import type { Address } from 'viem';
import { useI18n } from '@/i18n/client';
import { coreLive, dep } from '@/lib/chain';
import { date as fmtDate, usd } from '@/lib/format';
import { app, FROM_BLOCK, pub } from '@/lib/app-chain';
import { lifeRegistryAbi } from '@/sdk/abi.ts';
import { Notice } from '@/components/Notice';
import { Card, Page, Primary, Secondary, reason } from './ui';
import { Account } from './Account';
import { PayIn } from './PayIn';
import { useWallet } from './Wallet';

type Row = Awaited<ReturnType<typeof app.accountsOf>>[number] & {
  m: Awaited<ReturnType<typeof app.member>>;
  status: Awaited<ReturnType<typeof app.status>>;
  identified: boolean;
  held: Awaited<ReturnType<typeof app.held>>;
  report: Awaited<ReturnType<typeof app.report>>;
  payout: Address;
  nextCheckIn: Date | null;
};

/** Every account a wallet is part of, found from the chain's own logs, with what it can do now. */
export function Me() {
  const { t, f, locale } = useI18n();
  const w = useWallet();
  const [rows, setRows] = useState<Row[] | null>(null);
  const [claimable, setClaimable] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!w.sender || !coreLive) return;
    const me = w.sender.address;
    const [accts, owedToMe, period] = await Promise.all([app.accountsOf(me, FROM_BLOCK), app.claimable(me), pub.readContract({ address: dep.lifeRegistry!, abi: lifeRegistryAbi, functionName: 'checkInPeriod' })]);
    setClaimable(owedToMe);
    setRows(
      await Promise.all(
        accts.map(async (a) => {
          const [m, status, identified, held, report, life] = await Promise.all([app.member(a.memberId), app.status(a.memberId), app.identified(a.memberId), app.held(a.memberId), app.report(a.memberId), app.life(a.memberId)]);
          return { ...a, m, status, identified, held, report, payout: life.payout, nextCheckIn: life.lastProof ? new Date((Number(life.lastProof) + Number(period)) * 1000) : null };
        }),
      ),
    );
  }, [w.sender]);
  useEffect(() => {
    load().catch((e) => setMsg(reason(e)));
  }, [load]);

  const act = async (label: string, run: () => Promise<unknown>) => {
    setBusy(label);
    setMsg(null);
    try {
      await run();
      await load();
    } catch (e) {
      setMsg(f(t.app.failed, { detail: reason(e) }));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Page title={t.me.title}>
      {!coreLive && <Notice>{t.app.notLive}</Notice>}
      <Card><Account /></Card>
      {w.sender && claimable > 0 && (
        <Card>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-body-l">{f(t.me.claimable, { amount: usd(locale, claimable) })}</p>
            <Primary className="sm:w-auto" busy={busy === 'withdraw'} onClick={() => act('withdraw', () => w.sender!.send([app.calls.withdraw()]))}>{f(t.me.withdraw, { amount: usd(locale, claimable) })}</Primary>
          </div>
        </Card>
      )}
      {w.sender && rows?.length === 0 && (
        <Card>
          <p className="text-body-l">{t.me.empty}</p>
          <a href={`/${locale}/join`} className="mt-3 inline-flex min-h-12 items-center font-bold underline underline-offset-4">{t.me.start}</a>
        </Card>
      )}
      {rows?.map((r) => {
        const closed = r.m.released;
        const state = closed ? t.me.closed : !r.identified ? t.me.awaitingIdentity : r.m.exitRequested && r.m.exitAt ? f(t.me.notice, { date: fmtDate(locale, r.m.exitAt) }) : r.m.units.atRisk > 0n && r.m.owedDollars > 0 ? t.me.paying : t.me.saving;
        return (
          <Card key={String(r.memberId)}>
            <div className="space-y-4">
              <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h2 className="text-heading font-bold">{f(t.app.member, { id: String(r.memberId) })}</h2>
                <p className="text-body text-ink-2">{state}</p>
              </div>
              <ul className="flex flex-wrap gap-2 text-caption">
                {r.roles.map((role) => (
                  <li key={role} className="rounded-full bg-haze px-3 py-1 ring-1 ring-ink/10">{t.me[role]}</li>
                ))}
              </ul>
              <dl className="grid gap-1 text-body">
                <div className="flex justify-between gap-4"><dt className="text-ink-2">{f(t.me.startsAt, { age: r.m.startAge })}</dt><dd>{r.m.escalating ? t.join.rising : t.join.level}</dd></div>
                <div className="flex justify-between gap-4"><dt className="text-ink-2">{t.me.state}</dt><dd className="text-right">{t.me.statuses[r.status]}</dd></div>
                {!closed && r.nextCheckIn && <div className="flex justify-between gap-4"><dt className="text-ink-2">{t.checkin.title}</dt><dd className="text-right">{f(t.me.nextCheckIn, { date: fmtDate(locale, r.nextCheckIn) })}</dd></div>}
                <div>{r.m.valueLive ? f(t.me.value, { amount: usd(locale, r.m.valueDollars) }) : f(t.me.valueStale, { amount: usd(locale, r.m.valueDollars), date: fmtDate(locale, r.m.valuedAt) })}</div>
                {r.m.owedDollars > 0 && <div className="font-bold">{f(t.me.owed, { amount: usd(locale, r.m.owedDollars) })}</div>}
              </dl>
              {!closed && !r.identified && (
                <Notice>
                  {t.me.identityHow}
                  {process.env.NEXT_PUBLIC_OPERATOR_CONTACT && (
                    <a href={process.env.NEXT_PUBLIC_OPERATOR_CONTACT} className="mt-2 block font-bold underline underline-offset-4">{t.join.identityContact}</a>
                  )}
                </Notice>
              )}
              {r.report && <p role="alert" className="rounded-xl bg-danger/10 p-3 text-body text-danger">{f(t.me.reported, { date: fmtDate(locale, r.report.answerBy) })}</p>}
              {r.held.renewBy && <Notice kind="hold">{f(t.me.held, { date: fmtDate(locale, r.held.renewBy) })}</Notice>}
              <div className="flex flex-wrap gap-2">
                {r.m.owedDollars > 0 && <Secondary disabled={!!busy} onClick={() => act('claim', () => w.sender!.send([app.calls.claim(r.memberId)]))}>{t.me.claim}</Secondary>}
                {r.roles.includes('guardian') && !closed && <Secondary disabled={!!busy} onClick={() => act('attest', () => w.sender!.send([app.calls.attest(r.memberId)]))}>{t.me.attest}</Secondary>}
                <a href={`/${locale}/checkin/${r.memberId}`} className="press inline-flex min-h-12 items-center rounded-xl px-4 font-bold ring-1 ring-ink/20">{t.checkin.title}</a>
              </div>
            </div>
          </Card>
        );
      })}
      {rows?.filter((r) => !r.m.released && r.identified && (r.roles.includes('payer') || r.roles.includes('payout'))).map((r) => (
        <PayIn key={`pay-${r.memberId}`} memberId={r.memberId} />
      ))}
      {msg && <p role="alert" className="text-body text-danger">{msg}</p>}
    </Page>
  );
}
