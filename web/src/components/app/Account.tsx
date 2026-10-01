'use client';

import { useEffect, useState } from 'react';
import { type Address, erc20Abi } from 'viem';
import { useI18n } from '@/i18n/client';
import { dep } from '@/lib/chain';
import { num, short } from '@/lib/format';
import { Notice } from '@/components/Notice';
import { useWallet } from './Wallet';
import { Primary, Secondary } from './ui';

// Joining is about 534k gas; at Robinhood Chain's prices that is about 0.00001 ETH.
const FEE_FLOOR = 20_000_000_000_000n;

/** Sign in, or use the browser's wallet; once signed in, which address it is, what it holds on
 * Robinhood Chain, and who pays gas. */
export function Account() {
  const { t, f, locale } = useI18n();
  const w = useWallet();
  const address = w.sender?.address;
  const [held, setHeld] = useState<{ of: Address; eth: bigint; usdg: bigint } | null>(null);
  useEffect(() => {
    if (!address) return;
    let live = true;
    (async () => {
      const { pub } = await import('@/lib/app-chain');
      const [eth, usdg] = await Promise.all([pub.getBalance({ address }), pub.readContract({ address: dep.usdg, abi: erc20Abi, functionName: 'balanceOf', args: [address] })]);
      if (live) setHeld({ of: address, eth, usdg });
    })().catch(() => undefined);
    return () => {
      live = false;
    };
  }, [address]);
  if (w.sender) {
    const h = held?.of === w.sender.address ? held : null;
    return (
      <div className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl bg-haze px-4 py-3">
          <div className="min-w-0">
            <p className="font-mono text-body" title={w.sender.address}>{f(t.app.signedIn, { address: short(w.sender.address) })}</p>
            {h && <p className="font-mono text-caption tabular text-ink-2">{f(t.app.balances, { eth: num(locale, Number(h.eth) / 1e18, 6), usdg: num(locale, Number(h.usdg) / 1e6, 2) })}</p>}
            <p className="text-caption text-ink-2">{w.sender.gasless ? t.app.gasless : t.app.paysGas}</p>
          </div>
          <Secondary onClick={() => w.signOut()}>{t.app.signOut}</Secondary>
        </div>
        {h && !w.sender.gasless && h.eth < FEE_FLOOR && <Notice kind="hold">{t.app.noGas}</Notice>}
      </div>
    );
  }
  return (
    <div className="space-y-3">
      {w.privy && (
        <>
          <Primary onClick={() => w.signIn()} disabled={!w.privyReady}>
            {t.app.signIn}
          </Primary>
          <p className="text-caption text-ink-2">{t.app.signInHint}</p>
        </>
      )}
      {w.privy ? <Secondary onClick={() => w.useBrowserWallet()} className="w-full">{t.app.connect}</Secondary> : <Primary onClick={() => w.useBrowserWallet()}>{t.app.connect}</Primary>}
      {w.error && <p role="alert" className="text-caption text-danger">{w.error === 'no-wallet' ? t.app.noWallet : f(t.app.failed, { detail: w.error })}</p>}
    </div>
  );
}
