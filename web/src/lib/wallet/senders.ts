import { type Address, type Hex, type LocalAccount, type Log, createWalletClient, custom } from 'viem';
import { waitForTransactionReceipt } from 'viem/actions';
import { robinhood } from '@/sdk/chain.ts';
import type { Call } from '@/sdk/client.ts';
import { client } from '@/lib/chain';

export type Mined = { hash: Hex; logs: Log[] };

/** Something that can send calls on Robinhood Chain for the person using the page. */
export type Sender = {
  kind: 'smart' | 'injected' | 'device';
  address: Address;
  /** Gas paid by the Alchemy policy (smart wallets), or by the wallet itself. */
  gasless: boolean;
  /** Sends the calls, as one batch where the wallet can, and resolves once they are mined. */
  send(calls: Call[]): Promise<Mined[]>;
};

import { alchemy } from './sponsor';
export { alchemy, canSponsor } from './sponsor';

/** A smart wallet (EIP-7702, Alchemy Wallet APIs) signed by `signer`, its gas paid by the policy. */
export async function smartSender(signer: LocalAccount, kind: 'smart' | 'device' = 'smart'): Promise<Sender> {
  const { createSmartWalletClient, alchemyWalletTransport } = await import('@alchemy/wallet-apis');
  const wallet = createSmartWalletClient({ signer, transport: alchemyWalletTransport({ apiKey: alchemy.key! }), chain: robinhood, paymaster: { policyId: alchemy.policy! } });
  return {
    kind,
    address: signer.address,
    gasless: true,
    async send(calls) {
      const { id } = await wallet.sendCalls({ calls: calls.map((c) => ({ to: c.to, data: c.data, value: c.value ?? 0n })) });
      const r = await wallet.waitForCallsStatus({ id });
      if (r.status !== 'success') throw new Error(`the transaction didn't go through (${r.status})`);
      return (r.receipts ?? []).map((x) => ({ hash: x.transactionHash, logs: x.logs as Log[] }));
    },
  };
}

type Eip1193 = { request(a: { method: string; params?: unknown[] }): Promise<unknown> };
const CHAIN_HEX = `0x${robinhood.id.toString(16)}`;

/** A wallet's own provider (Robinhood Wallet, MetaMask, a WalletConnect wallet): it pays its gas. */
export async function providerSender(eth: Eip1193): Promise<Sender> {
  const [address] = (await eth.request({ method: 'eth_requestAccounts' })) as Address[];
  try {
    await eth.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: CHAIN_HEX }] });
  } catch (e) {
    if ((e as { code?: number }).code !== 4902) throw e;
    await eth.request({
      method: 'wallet_addEthereumChain',
      params: [{ chainId: CHAIN_HEX, chainName: robinhood.name, nativeCurrency: robinhood.nativeCurrency, rpcUrls: robinhood.rpcUrls.default.http, blockExplorerUrls: [robinhood.blockExplorers!.default.url] }],
    });
  }
  const wallet = createWalletClient({ account: address, chain: robinhood, transport: custom(eth) });
  return {
    kind: 'injected',
    address,
    gasless: false,
    async send(calls) {
      const out: Mined[] = [];
      for (const c of calls) {
        const hash = await wallet.sendTransaction({ to: c.to, data: c.data, value: c.value ?? 0n });
        const r = await waitForTransactionReceipt(client, { hash });
        if (r.status !== 'success') throw new Error(`the transaction didn't go through: ${hash}`);
        out.push({ hash, logs: r.logs });
      }
      return out;
    },
  };
}

/** The browser's own wallet, if there is one. */
export const injected = (): Eip1193 | null => (typeof window !== 'undefined' ? ((window as { ethereum?: Eip1193 }).ethereum ?? null) : null);

const DEVICE_KEY = 'tonti:device-key';

/** A parent's phone: a throwaway signing key kept on the device, used only to relay calls anyone
 * may make (joining, check-ins). It never holds money; the policy pays its gas. */
export async function deviceSender(): Promise<Sender> {
  const { generatePrivateKey, privateKeyToAccount } = await import('viem/accounts');
  let k = localStorage.getItem(DEVICE_KEY) as Hex | null;
  if (!k) {
    k = generatePrivateKey();
    localStorage.setItem(DEVICE_KEY, k);
  }
  return smartSender(privateKeyToAccount(k), 'device');
}
