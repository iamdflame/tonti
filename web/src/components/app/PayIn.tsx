'use client';

import { useEffect, useState } from 'react';
import { erc20Abi } from 'viem';
import { useI18n } from '@/i18n/client';
import { dep } from '@/lib/chain';
import { app, pub, PREVIEW_CAP_USDG, RELAY_USDG } from '@/lib/app-chain';
import { usd } from '@/lib/format';
import { Card, Label, Primary, reason } from './ui';
import { useWallet } from './Wallet';

/** Paying in, once the member's identity is checked: USDG approve and contribute in one go. */
export function PayIn({ memberId }: { memberId: bigint }) {
  const { t, f, locale } = useI18n();
  const w = useWallet();
  const [identified, setIdentified] = useState<boolean | null>(null);
  const [paid, setPaid] = useState(0);
  const [balance, setBalance] = useState<number | null>(null);
  const [amount, setAmount] = useState(5);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    let live = true;
    const load = async () => {
      const [id, m] = await Promise.all([app.identified(memberId), app.member(memberId).catch(() => null)]);
      if (!live) return;
      setIdentified(id);
      if (m) setPaid(m.contributedDollars);
      if (w.sender) {
        const b = await pub.readContract({ address: dep.usdg, abi: erc20Abi, functionName: 'balanceOf', args: [w.sender.address] });
        if (live) setBalance(Number(b) / 1e6);
      }
    };
    load().catch(() => undefined);
    return () => {
      live = false;
    };
  }, [memberId, w.sender, msg]);

  const room = Math.max(0, PREVIEW_CAP_USDG - paid);
  const pay = async () => {
    setBusy(true);
    setMsg(null);
    try {
      await w.sender!.send(app.calls.contribute(memberId, amount));
      setMsg({ ok: true, text: f(t.join.paidIn, { amount: usd(locale, amount) }) });
    } catch (e) {
      setMsg({ ok: false, text: f(t.app.failed, { detail: reason(e) }) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <div className="space-y-4">
        <h2 className="text-heading font-bold">{t.join.payInTitle}</h2>
        <p className="text-caption text-ink-2">{t.join.payInHint} {f(t.join.ofTotal, { paid: usd(locale, paid), cap: usd(locale, PREVIEW_CAP_USDG, 0) })}</p>
        {identified === false ? (
          <p className="text-body">{t.join.notIdentified}</p>
        ) : (
          <>
            <div>
              <Label htmlFor="amount">USDG</Label>
              <input id="amount" inputMode="decimal" value={amount} onChange={(e) => setAmount(Math.min(room, Math.max(0, Number(e.target.value.replace(/[^0-9.]/g, '')) || 0)))}
                className="h-12 w-full rounded-xl bg-haze px-3 font-mono text-body-l ring-1 ring-input" />
            </div>
            {balance !== null && balance < amount && (
              <p className="text-body">
                {t.join.needUsdg}{' '}
                <a className="font-bold underline underline-offset-4" href={RELAY_USDG} target="_blank" rel="noreferrer">{t.join.getUsdg}</a>
              </p>
            )}
            <Primary busy={busy} onClick={pay} disabled={!w.sender || !identified || amount < 1 || amount > room}>
              {busy ? t.app.working : f(t.join.payIn, { amount: usd(locale, amount) })}
            </Primary>
          </>
        )}
        {msg && <p role={msg.ok ? 'status' : 'alert'} className={`text-body ${msg.ok ? 'text-ok' : 'text-danger'}`}>{msg.text}</p>}
      </div>
    </Card>
  );
}
