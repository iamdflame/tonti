import { createPublicClient, fallback, http, type Address } from 'viem';
import { robinhood } from '@/sdk/chain.ts';
import { tonti } from '@/sdk/client.ts';
import { PUBLIC_RPC, dep } from '@/lib/chain';

// The app screens' full client (logs, multicall, simulation): loaded by those screens only.
export const pub = createPublicClient({
  chain: robinhood,
  transport: fallback([process.env.NEXT_PUBLIC_ROBINHOOD_RPC, PUBLIC_RPC].filter((u): u is string => !!u).map((u) => http(u, { timeout: 20_000 }))),
  batch: { multicall: true },
});

const Z = '0x0000000000000000000000000000000000000000' as Address;
/** The typed pool client; before the core contracts are live its pool calls have nowhere to go. */
export const app = tonti(pub as never, { actuary: dep.actuary, pool: dep.pool ?? Z, treasury: dep.treasury ?? Z, lifeRegistry: dep.lifeRegistry ?? Z, usdg: dep.usdg, attestedIdentity: dep.attestedIdentity });
export const FROM_BLOCK = BigInt(dep.deployBlock ?? 0);
export const PREVIEW_CAP_USDG = 25;
export const RELAY_USDG = `https://relay.link/bridge/robinhood?toCurrency=${dep.usdg}`;
