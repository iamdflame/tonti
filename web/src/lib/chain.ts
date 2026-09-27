import { createClient, fallback, http, type Address } from 'viem';
import { robinhood } from '@/sdk/chain.ts';
import deployment from '@/sdk/deployment.json';

export const PUBLIC_RPC = 'https://rpc.mainnet.chain.robinhood.com';
/** Alchemy (restricted to this site's domain) when configured; the public endpoint always as fallback. */
const rpcs = [process.env.NEXT_PUBLIC_ROBINHOOD_RPC, PUBLIC_RPC].filter((u): u is string => !!u);

// Bare clients with tree-shakable actions (viem/actions): the landing page ships only what it calls.
export const client = createClient({
  chain: robinhood,
  transport: fallback(rpcs.map((u) => http(u, { timeout: 20_000, retryCount: 1 }))),
});

/** A client that talks only to the public node: "Run it yourself" re-asks a node we don't run. */
export const publicOnly = createClient({ chain: robinhood, transport: http(PUBLIC_RPC, { timeout: 20_000 }) });

type Dep = { actuary: Address; pool?: Address; treasury?: Address; lifeRegistry?: Address; usdg: Address; attestedIdentity?: Address; chainId: number; deployBlock?: number };
export const dep = deployment as unknown as Dep;
export const EXPLORER = 'https://robinhoodchain.blockscout.com';
export const explorer = (kind: 'address' | 'tx' | 'block', id: string | number | bigint) => `${EXPLORER}/${kind}/${id}`;

/** True once the core contracts are live (the pool is deployed); until then the site can only quote. */
export const coreLive = Boolean(dep.pool && dep.treasury && dep.lifeRegistry);

/** The per-transaction (and per-call) gas cap on Robinhood Chain. */
export const CALL_GAS_CAP = 32_000_000;
