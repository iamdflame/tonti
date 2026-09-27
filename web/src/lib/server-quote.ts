import { createClient, fallback, http } from 'viem';
import { robinhood } from '@/sdk/chain.ts';
import { quote } from '@/sdk/quote.ts';
import type { Ask } from '@/lib/slug';
import { dep, PUBLIC_RPC } from '@/lib/chain';

const server = createClient({
  chain: robinhood,
  transport: fallback([process.env.ROBINHOOD_RPC, process.env.NEXT_PUBLIC_ROBINHOOD_RPC, PUBLIC_RPC].filter((u): u is string => !!u).map((u) => http(u, { timeout: 20_000 }))),
});

/** The on-chain quote for a shared question, dated at the start of today (UTC), as the page is. */
export async function serverQuote(a: Ask) {
  const now = new Date(Math.floor(Date.now() / 86_400_000) * 86_400_000);
  return quote(server, dep.actuary, { country: a.country, sex: a.sex, birthYear: a.born, startAge: a.startAge, lumpSum: a.lump, monthly: a.monthly, now });
}
