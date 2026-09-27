'use client';

import { PrivyProvider, toViemAccount, usePrivy, useWallets, type PrivyClientConfig } from '@privy-io/react-auth';
import { useEffect, useMemo, useRef } from 'react';
import { robinhood } from '@/sdk/chain.ts';
import { type Sender, canSponsor, providerSender, smartSender } from '@/lib/wallet/senders';

type Props = { appId: string; onSender(s: Sender | null): void; onApi(a: { login(): void; logout(): Promise<void>; ready: boolean }): void };

// Built once: a config object made on each render made Privy re-initialise, and every render
// re-published its state upward: React's "maximum update depth" (#185) the moment it was enabled.
const CONFIG: PrivyClientConfig = {
  loginMethods: ['passkey', 'sms', 'email', 'google', 'wallet'],
  appearance: { theme: 'light', accentColor: '#14213d', landingHeader: 'Tonti', walletChainType: 'ethereum-only' },
  embeddedWallets: { ethereum: { createOnLogin: 'all-users' }, showWalletUIs: false },
  supportedChains: [robinhood],
  defaultChain: robinhood,
  ...(process.env.NEXT_PUBLIC_WALLETCONNECT_ID ? { walletConnectCloudProjectId: process.env.NEXT_PUBLIC_WALLETCONNECT_ID } : {}),
};

/** Privy sign-in beside the page (not around it): its state flows out as a Sender. An embedded
 * wallet becomes an EIP-7702 smart wallet whose gas the Alchemy policy pays; an external wallet
 * (WalletConnect, including Robinhood Wallet) sends with its own gas. */
export default function PrivyBridge({ appId, onSender, onApi }: Props) {
  return (
    <PrivyProvider appId={appId} config={CONFIG}>
      <Sync onSender={onSender} onApi={onApi} />
    </PrivyProvider>
  );
}

function Sync({ onSender, onApi }: Omit<Props, 'appId'>) {
  const { ready, authenticated, login, logout } = usePrivy();
  const { wallets } = useWallets();
  // Privy's functions aren't stable across renders: publish stable wrappers, and only when
  // readiness changes.
  const fns = useRef({ login, logout });
  fns.current = { login, logout };
  useEffect(() => {
    onApi({ login: () => fns.current.login(), logout: () => fns.current.logout(), ready });
  }, [ready, onApi]);
  // The wallets array is a new object on every render: key the effect on who the wallet is.
  const chosen = useMemo(() => wallets.find((w) => w.walletClientType === 'privy') ?? wallets[0], [wallets]);
  const id = authenticated && chosen ? `${chosen.walletClientType}:${chosen.address}` : '';
  const wallet = useRef(chosen);
  wallet.current = chosen;
  useEffect(() => {
    const w = wallet.current;
    if (!id || !w) return;
    let live = true;
    (async () => {
      const s = w.walletClientType === 'privy' && canSponsor ? await smartSender(await toViemAccount({ wallet: w })) : await providerSender((await w.getEthereumProvider()) as never);
      if (live) onSender(s);
    })().catch(() => undefined);
    return () => {
      live = false;
    };
  }, [id, onSender]);
  return null;
}
