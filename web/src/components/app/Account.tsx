'use client';

import { useI18n } from '@/i18n/client';
import { short } from '@/lib/format';
import { useWallet } from './Wallet';
import { Primary, Secondary } from './ui';

/** Sign in, or use the browser's wallet; once signed in, who and how gas is paid. */
export function Account() {
  const { t, f } = useI18n();
  const w = useWallet();
  if (w.sender) {
    return (
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl bg-haze px-4 py-3">
        <div>
          <p className="font-mono text-body">{f(t.app.signedIn, { address: short(w.sender.address) })}</p>
          <p className="text-caption text-ink-2">{w.sender.gasless ? t.app.gasless : t.app.paysGas}</p>
        </div>
        <Secondary onClick={() => w.signOut()}>{t.app.signOut}</Secondary>
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
