'use client';

import { useCallback, useEffect, useState } from 'react';
import { type Address, createWalletClient, custom } from 'viem';
import { attestedIdentityAbi, lifeRegistryAbi } from '@/sdk/abi.ts';
import { robinhood } from '@/sdk/chain.ts';
import { attestIdentity } from '@/sdk/client.ts';
import { countries } from '@/sdk/countries.ts';
import { type Iso3, type Sex, cohortKey } from '@/sdk/units.ts';
import { useI18n } from '@/i18n/client';
import { coreLive, dep } from '@/lib/chain';
import { short } from '@/lib/format';
import { app, pub, FROM_BLOCK } from '@/lib/app-chain';
import { injected } from '@/lib/wallet/senders';
import { Notice } from '@/components/Notice';
import { Card, Label, Page, Primary, reason } from './ui';
import { Account } from './Account';
import { useWallet } from './Wallet';

/** The identity attester's desk (the research preview's trusted step, disclosed): members waiting
 * for a check; after a video call, the document's fields are checked against the cohort key the
 * member joined with, and only a match is signed (EIP-712) and submitted. */
export function Operator() {
  const { t, f } = useI18n();
  const w = useWallet();
  const [attester, setAttester] = useState<Address | null>(null);
  const [pending, setPending] = useState<{ id: bigint; key: bigint }[] | null>(null);
  const [sel, setSel] = useState<{ id: bigint; key: bigint } | null>(null);
  const [country, setCountry] = useState<Iso3>('PHL');
  const [sex, setSex] = useState<Sex>('female');
  const [born, setBorn] = useState(1966);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);

  const load = useCallback(async () => {
    if (!coreLive || !dep.attestedIdentity) return;
    setAttester(await pub.readContract({ address: dep.attestedIdentity, abi: attestedIdentityAbi, functionName: 'attester' }));
    const [enrolled, identified] = await Promise.all([
      pub.getContractEvents({ address: dep.lifeRegistry!, abi: lifeRegistryAbi, eventName: 'Enrolled', fromBlock: FROM_BLOCK }),
      pub.getContractEvents({ address: dep.lifeRegistry!, abi: lifeRegistryAbi, eventName: 'Identified', fromBlock: FROM_BLOCK }),
    ]);
    const done = new Set(identified.map((e) => e.args.memberId));
    setPending(enrolled.filter((e) => !done.has(e.args.memberId)).map((e) => ({ id: e.args.memberId!, key: e.args.key! })));
  }, []);
  useEffect(() => {
    load().catch((e) => setMsg({ ok: false, text: reason(e) }));
  }, [load]);

  const isAttester = attester && w.sender && w.sender.address.toLowerCase() === attester.toLowerCase();
  const docKey = (() => {
    try {
      return cohortKey(country, sex, born);
    } catch {
      return null;
    }
  })();
  const match = sel && docKey === sel.key;
  const decode = (k: bigint) => `${countries.find((c) => c.isoNumeric === Number(k / 20_000n))?.name ?? '?'} · ${(k / 10_000n) % 2n === 0n ? t.form.woman : t.form.man} · ${k % 10_000n}`;

  const sign = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const eth = injected();
      if (!eth) throw new Error(t.app.noWallet);
      const wallet = createWalletClient({ account: w.sender!.address, chain: robinhood, transport: custom(eth as never) });
      const proof = await attestIdentity(wallet as never, { verifier: dep.attestedIdentity!, registry: dep.lifeRegistry!, memberId: sel!.id, key: sel!.key });
      await w.sender!.send([app.calls.strongProof(sel!.id, dep.attestedIdentity!, proof)]);
      setMsg({ ok: true, text: f(t.operator.signed, { id: String(sel!.id) }) });
      setSel(null);
      await load();
    } catch (e) {
      setMsg({ ok: false, text: f(t.app.failed, { detail: reason(e) }) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Page title={t.operator.title} lead={t.operator.lead}>
      {!coreLive && <Notice>{t.app.notLive}</Notice>}
      <Card><Account /></Card>
      {w.sender && attester && !isAttester && <p role="alert" className="text-body text-danger">{f(t.operator.notAttester, { attester: short(attester) })}</p>}
      {isAttester && (
        <Card>
          <h2 className="text-heading font-bold">{t.operator.pending}</h2>
          {pending?.length === 0 && <p className="mt-2 text-body text-ink-2">{t.operator.none}</p>}
          <ul className="mt-3 divide-y divide-border">
            {pending?.map((p) => (
              <li key={String(p.id)}>
                <button type="button" onClick={() => setSel(p)} className={`flex min-h-14 w-full items-center justify-between gap-3 px-2 text-left ${sel?.id === p.id ? 'bg-haze' : ''}`}>
                  <span className="font-bold">{f(t.app.member, { id: String(p.id) })}</span>
                  <span className="text-ink-2">{decode(p.key)}</span>
                </button>
              </li>
            ))}
          </ul>
        </Card>
      )}
      {isAttester && sel && (
        <Card>
          <div className="space-y-4">
            <div className="grid gap-4 sm:grid-cols-3">
              <div>
                <Label htmlFor="doc-country">{t.operator.country}</Label>
                <select id="doc-country" value={country} onChange={(e) => setCountry(e.target.value as Iso3)} className="h-12 w-full rounded-xl bg-haze px-3 ring-1 ring-input">
                  {countries.map((c) => <option key={c.iso3} value={c.iso3}>{c.name}</option>)}
                </select>
              </div>
              <div>
                <Label htmlFor="doc-sex">{t.operator.sex}</Label>
                <select id="doc-sex" value={sex} onChange={(e) => setSex(e.target.value as Sex)} className="h-12 w-full rounded-xl bg-haze px-3 ring-1 ring-input">
                  <option value="female">{t.form.woman}</option>
                  <option value="male">{t.form.man}</option>
                </select>
              </div>
              <div>
                <Label htmlFor="doc-born">{t.operator.born}</Label>
                <input id="doc-born" inputMode="numeric" value={born} onChange={(e) => setBorn(Number(e.target.value.replace(/\D/g, '')) || 0)} className="h-12 w-full rounded-xl bg-haze px-3 font-mono ring-1 ring-input" />
              </div>
            </div>
            <p role="status" className={`text-body font-bold ${match ? 'text-ok' : 'text-danger'}`}>{match ? t.operator.match : t.operator.mismatch}</p>
            <Primary busy={busy} disabled={!match} onClick={sign}>{t.operator.sign}</Primary>
          </div>
        </Card>
      )}
      {msg && <p role={msg.ok ? 'status' : 'alert'} className={`text-body ${msg.ok ? 'text-ok' : 'text-danger'}`}>{msg.text}</p>}
    </Page>
  );
}
