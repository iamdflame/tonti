'use client';

import { useCallback, useEffect, useState } from 'react';
import { useI18n } from '@/i18n/client';
import { coreLive, dep } from '@/lib/chain';
import { date as fmtDate } from '@/lib/format';
import { signCheckIn, passkeyFailure, passkeysAvailable } from '@/lib/passkey';
import { OpenInBrowser, useInApp } from './InApp';
import { canSponsor } from '@/lib/wallet/sponsor';
import { Notice } from '@/components/Notice';
import { Card, Page, Primary, b64, reason } from './ui';

type Info = { status: string; due: Date; grace: Date; recovery: Date | null; identityBy: Date | null; held: boolean };

export function CheckIn({ id }: { id: bigint | null }) {
  const { t, f, locale } = useI18n();
  const [info, setInfo] = useState<Info | null | 'none'>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<Date | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [relay, setRelay] = useState<string | null>(null);
  const inApp = useInApp();

  const load = useCallback(async () => {
    if (id === null || !coreLive) return setInfo('none');
    // The chain client and ABIs load after the page has painted (a parent's phone is often slow).
    const [{ app, pub }, { lifeRegistryAbi }] = await Promise.all([import('@/lib/app-chain'), import('@/sdk/abi.ts')]);
    const [status, life, period, grace, pending, strongPeriod, can] = await Promise.all([
      app.status(id),
      app.life(id),
      pub.readContract({ address: dep.lifeRegistry!, abi: lifeRegistryAbi, functionName: 'checkInPeriod' }),
      pub.readContract({ address: dep.lifeRegistry!, abi: lifeRegistryAbi, functionName: 'grace' }),
      pub.readContract({ address: dep.lifeRegistry!, abi: lifeRegistryAbi, functionName: 'pendingRecovery', args: [id] }),
      pub.readContract({ address: dep.lifeRegistry!, abi: lifeRegistryAbi, functionName: 'strongPeriod' }),
      pub.readContract({ address: dep.lifeRegistry!, abi: lifeRegistryAbi, functionName: 'canReceiveIncome', args: [id] }),
    ]);
    if (status === 'none') return setInfo('none');
    const last = Number(life.lastProof);
    // A recovery someone asked for: it moves the account after the challenge window unless she checks in.
    const readyAt = Number((pending as readonly unknown[])[3] ?? 0);
    // The yearly identity check: without it income is held, whatever the check-ins say.
    const strong = Number(life.lastStrong);
    const identityBy = life.identified && strong ? new Date((strong + Number(strongPeriod)) * 1000) : null;
    setInfo({ status, due: new Date((last + Number(period)) * 1000), grace: new Date((last + Number(period) + Number(grace)) * 1000), recovery: readyAt ? new Date(readyAt * 1000) : null, identityBy, held: Boolean(identityBy && identityBy.getTime() < Date.now()) && !can });
  }, [id]);
  useEffect(() => {
    load().catch(() => setInfo('none'));
  }, [load]);

  const checkIn = async () => {
    setBusy(true);
    setErr(null);
    try {
      const { app } = await import('@/lib/app-chain');
      const auth = await signCheckIn(await app.challenge(id!));
      if (canSponsor) {
        const { deviceSender } = await import('@/lib/wallet/senders');
        const s = await deviceSender();
        await s.send([app.calls.checkIn(id!, auth)]);
        await load();
        setDone(new Date(Date.now() + 90 * 86_400_000));
      } else {
        const payload = { v: 1, t: 'checkin', id: String(id), auth: { ...auth, challengeIndex: String(auth.challengeIndex), typeIndex: String(auth.typeIndex) } };
        setRelay(`${window.location.origin}/${locale}/relay#${b64.enc(payload)}`);
      }
    } catch (e) {
      const why = passkeyFailure(e);
      setErr(why === 'blocked' ? t.checkin.blocked : why === 'cancelled' ? t.checkin.cancelled : f(t.app.failed, { detail: reason(e) }));
    } finally {
      setBusy(false);
    }
  };

  if (!coreLive) return <Page title={t.checkin.title}><Notice>{t.checkin.notLive}</Notice></Page>;
  if (id === null || info === 'none') return <Page title={t.checkin.title}><Notice kind="hold">{f(t.checkin.notMember, { id: String(id ?? '?') })}</Notice></Page>;
  if (info === null) return <Page title={t.checkin.title}><p className="text-body-l text-ink-2" role="status">…</p></Page>;
  const closed = info.status === 'deceased' || info.status === 'presumedDeceased';
  const line = info.status === 'active' ? f(t.checkin.active, { date: fmtDate(locale, info.due) }) : info.status === 'due' ? f(t.checkin.due, { date: fmtDate(locale, info.grace) }) : t.checkin.lapsed;

  return (
    <Page title={`${t.checkin.title} · ${f(t.app.member, { id: String(id) })}`}>
      <Card>
        <div className="space-y-5">
          <p className={`text-heading font-bold ${info.status === 'lapsed' ? 'text-danger' : ''}`} role="status">{closed ? t.checkin.deceased : done ? f(t.checkin.done, { date: fmtDate(locale, done) }) : line}</p>
          {info.recovery && !done && !closed && <Notice kind="hold">{f(t.checkin.recovery, { date: fmtDate(locale, info.recovery) })}</Notice>}
          {!closed && info.held && <Notice kind="hold">{t.checkin.identityLapsed}</Notice>}
          {!closed && !info.held && info.identityBy && <p className="text-body text-ink-2">{f(t.checkin.identityDue, { date: fmtDate(locale, info.identityBy) })}</p>}
          {!closed && !done && !relay && inApp !== null && <OpenInBrowser app={inApp} use />}
          {!closed && !done && !relay && (
            passkeysAvailable() ? (
              <Primary busy={busy} onClick={checkIn} className="min-h-20 text-heading">{busy ? t.checkin.signing : t.checkin.button}</Primary>
            ) : (
              <p role="alert" className="text-body text-danger">{t.join.noPasskeys}</p>
            )
          )}
          {relay && (
            <div className="space-y-3">
              <p className="text-body">{t.checkin.relayHint}</p>
              <Primary onClick={() => (navigator.share ? navigator.share({ url: relay }).catch(() => undefined) : navigator.clipboard.writeText(relay))}>{t.checkin.relay}</Primary>
            </div>
          )}
          {err && <p role="alert" className="text-body text-danger">{err}</p>}
          <a download="tonti-check-in.ics" href={ics(info.due, `${window.location.origin}/${locale}/checkin/${id}`)} className="inline-flex min-h-12 items-center text-body underline underline-offset-4">{t.checkin.calendar}</a>
        </div>
      </Card>
    </Page>
  );
}

/** A calendar file that reminds her every 90 days, starting at her next check-in date. */
function ics(first: Date, url: string) {
  const d = (x: Date) => x.toISOString().replace(/[-:]/g, '').slice(0, 8);
  const body = ['BEGIN:VCALENDAR', 'VERSION:2.0', 'PRODID:-//Tonti//Check-in//EN', 'BEGIN:VEVENT', `UID:tonti-checkin-${d(first)}@tonti`, `DTSTAMP:${d(new Date())}T000000Z`, `DTSTART;VALUE=DATE:${d(first)}`, 'RRULE:FREQ=DAILY;INTERVAL=90', 'SUMMARY:Tonti check-in', `DESCRIPTION:${url}`, `URL:${url}`, 'BEGIN:VALARM', 'TRIGGER:-P3D', 'ACTION:DISPLAY', 'DESCRIPTION:Tonti check-in', 'END:VALARM', 'END:VEVENT', 'END:VCALENDAR'].join('\r\n');
  return `data:text/calendar;charset=utf-8,${encodeURIComponent(body)}`;
}
