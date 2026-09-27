'use client';

import dynamic from 'next/dynamic';
import { createContext, useCallback, useContext, useState, type ReactNode } from 'react';
import { type Sender, injected, providerSender } from '@/lib/wallet/senders';

/** Sign-in (Privy: Face ID, email, phone, Google, or a WalletConnect wallet) loads only on app
 * screens and only when configured; a wallet already in the browser always works. */
const PRIVY_APP_ID = process.env.NEXT_PUBLIC_PRIVY_APP_ID;
const PrivyBridge = dynamic(() => import('./PrivyBridge'), { ssr: false });

type Api = { login(): void; logout(): Promise<void>; ready: boolean };
type Ctx = {
  sender: Sender | null;
  privy: boolean;
  privyReady: boolean;
  signIn(): void;
  useBrowserWallet(): Promise<void>;
  signOut(): Promise<void>;
  error: string | null;
};

const WalletCtx = createContext<Ctx | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [sender, setSender] = useState<Sender | null>(null);
  const [api, setApi] = useState<Api | null>(null);
  const [error, setError] = useState<string | null>(null);
  const useBrowserWallet = useCallback(async () => {
    setError(null);
    const eth = injected();
    if (!eth) {
      setError('no-wallet');
      return;
    }
    try {
      setSender(await providerSender(eth));
    } catch (e) {
      setError((e as Error).message.slice(0, 160));
    }
  }, []);
  const value: Ctx = {
    sender,
    privy: Boolean(PRIVY_APP_ID),
    privyReady: Boolean(api?.ready),
    signIn: () => api?.login(),
    useBrowserWallet,
    signOut: async () => {
      if (sender?.kind === 'smart' && api) await api.logout();
      setSender(null);
    },
    error,
  };
  return (
    <WalletCtx.Provider value={value}>
      {children}
      {PRIVY_APP_ID && <PrivyBridge appId={PRIVY_APP_ID} onSender={setSender} onApi={setApi} />}
    </WalletCtx.Provider>
  );
}

export function useWallet() {
  const c = useContext(WalletCtx);
  if (!c) throw new Error('useWallet outside WalletProvider');
  return c;
}
