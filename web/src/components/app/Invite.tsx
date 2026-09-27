'use client';

import { useEffect, useState } from 'react';
import { type Address, isAddress } from 'viem';
import { memberIdFromLogs, type JoinInput } from '@/sdk/client.ts';
import type { Iso3 } from '@/sdk/units.ts';
import { useI18n } from '@/i18n/client';
import { coreLive } from '@/lib/chain';
import { createLifeKey, passkeysAvailable } from '@/lib/passkey';
import { canSponsor, deviceSender } from '@/lib/wallet/senders';
import { app } from '@/lib/app-chain';
import { Notice } from '@/components/Notice';
import { AddressInput, Card, Page, Primary, Secondary, b64, reason } from './ui';

type Inv = { v: 1; c: Iso3; s: 'f' | 'm'; b: number; a: number; e: 0 | 1; q: number; n: string; g: Address };

export function Invite() {
  const { t, f, locale } = useI18n();
  const [inv, setInv] = useState<Inv | null | undefined>(undefined);
  const [key, setKey] = useState<{ qx: `0x${string}`; qy: `0x${string}` } | null>(null);
  const [dest, setDest] = useState<'child' | 'own'>('child');
  const [own, setOwn] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [memberId, setMemberId] = useState<bigint | null>(null);

  useEffect(() => {
    const x = b64.dec<Inv>(window.location.hash.slice(1));
    setInv(x && x.v === 1 && isAddress(x.g) ? x : null);
  }, []);
  if (inv === undefined) return null;
  if (inv === null) return <Page title={t.checkin.title}><p role="alert" className="text-body-l">{t.invite.broken}</p></Page>;

  const name = inv.n || t.invite.someone;
  const payout = (dest === 'child' ? inv.g : own) as Address;
  const input = (): JoinInput => ({
    country: inv.c,
    sex: inv.s === 'f' ? 'female' : 'male',
    birthYear: inv.b,
    startAge: inv.a,
    bequestShare: inv.q / 10_000,
    escalating: inv.e === 1,
    payout,
    beneficiary: inv.g,
    passkey: key!,
    guardians: [inv.g, '0x0000000000000000000000000000000000000000', '0x0000000000000000000000000000000000000000'],
  });
  const join = async () => {
    setBusy(true);
    setErr(null);
    try {
      const s = await deviceSender();
      const mined = await s.send([app.calls.join(input())]);
      setMemberId(memberIdFromLogs(mined.flatMap((m) => m.logs)));
    } catch (e) {
      setErr(reason(e));
    } finally {
      setBusy(false);
    }
  };
  // Without sponsorship her phone can't pay gas: her child sends the same call from their wallet.
  const relayUrl = key ? `${window.location.origin}/${locale}/relay#${b64.enc({ v: 1, t: 'join', j: { ...input(), passkey: key } })}` : '';

  return (
    <Page title={inv.n ? f(t.invite.hello, { name }) : t.invite.helloAnon} lead={f(t.invite.plan, { age: inv.a })}>
      {!coreLive && <Notice>{t.app.notLive}</Notice>}
      {memberId !== null ? (
        <Card>
          <div className="space-y-4">
            <p className="text-title font-extrabold">{f(t.invite.done, { id: String(memberId) })}</p>
            <p className="text-body-l">{t.invite.home}</p>
            <a href={`/${locale}/checkin/${memberId}`} className="press inline-flex min-h-14 w-full items-center justify-center rounded-xl bg-lamp text-body-l font-extrabold">{t.invite.open}</a>
          </div>
        </Card>
      ) : (
        <>
          <Card>
            <div className="space-y-4">
              <h2 className="text-heading font-bold">1. {t.invite.step1}</h2>
              <p className="text-body-l text-ink-2">{t.invite.step1Body}</p>
              {!passkeysAvailable() ? (
                <p role="alert" className="text-body text-danger">{t.join.noPasskeys}</p>
              ) : key ? (
                <p className="text-body-l font-bold text-ok">✓ {t.invite.step1Done}</p>
              ) : (
                <Primary busy={busy} onClick={async () => {
                  setBusy(true);
                  setErr(null);
                  try {
                    const k = await createLifeKey('Tonti');
                    setKey({ qx: k.qx, qy: k.qy });
                  } catch (e) {
                    setErr(reason(e));
                  } finally {
                    setBusy(false);
                  }
                }} className="min-h-16 text-heading">{t.invite.step1Button}</Primary>
              )}
            </div>
          </Card>
          <Card locked={!key}>
            <fieldset className="space-y-3" disabled={!key}>
              <legend className="text-heading font-bold">2. {t.invite.step2}</legend>
              {!key && <p className="text-caption text-ink-2">{t.invite.afterStep1}</p>}
              {(['child', 'own'] as const).map((d) => (
                <label key={d} className={`flex min-h-14 cursor-pointer items-start gap-3 rounded-xl p-3 ring-1 ${dest === d ? 'bg-ink/5 ring-ink' : 'ring-ink/20'}`}>
                  <input type="radio" name="dest" checked={dest === d} onChange={() => setDest(d)} className="mt-1.5 size-5 accent-ink" />
                  <span>
                    <span className="block text-body-l font-bold">{d === 'child' ? f(t.invite.toChild, { name }) : t.invite.pasteAddress}</span>
                    {d === 'child' && <span className="text-caption text-ink-2">{t.invite.toChildHint}</span>}
                  </span>
                </label>
              ))}
              {dest === 'own' && <AddressInput id="own" label={t.app.address} value={own} onChange={setOwn} />}
            </fieldset>
          </Card>
          <Card locked={!key}>
            <div className="space-y-3">
              <h2 className="text-heading font-bold">3. {t.invite.step3}</h2>
              {!key && <p className="text-caption text-ink-2">{t.invite.afterStep1}</p>}
              {canSponsor ? (
                <Primary busy={busy} onClick={join} disabled={!key || !coreLive || !isAddress(payout)} className="min-h-16 text-heading">{busy ? t.app.working : t.invite.join}</Primary>
              ) : (
                <>
                  <Primary onClick={() => (navigator.share ? navigator.share({ url: relayUrl }).catch(() => undefined) : navigator.clipboard.writeText(relayUrl))} disabled={!key || !isAddress(payout)} className="min-h-16 text-heading">
                    {f(t.invite.viaChild, { name })}
                  </Primary>
                  <p className="text-caption text-ink-2">{t.invite.viaChildHint}</p>
                </>
              )}
              {err && <p role="alert" className="text-body text-danger">{f(t.app.failed, { detail: err })}</p>}
            </div>
          </Card>
        </>
      )}
    </Page>
  );
}
