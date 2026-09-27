import type { Address, Client } from 'viem';
import { getBlockNumber, multicall } from 'viem/actions';
import { actuaryAbi } from '@/sdk/abi.ts';
import { MULTICALL3 } from '@/sdk/chain.ts';
import { countries } from '@/sdk/countries.ts';
import { type Iso3, type Sex, cohortKey, fromWad, toWad } from '@/sdk/units.ts';

export type Odds = { iso3: Iso3; name: string; female: number; male: number };
export type OddsRead = { rows: Odds[]; born: number; block: bigint; calls: number; batches: number };

const FROM = 65;
const TO = 85;
// 64k gas a read; 280 reads stay well inside a node's 32M eth_call cap.
const CHUNK = 280;

export { cohortCount, countryCount } from './cohorts';

/** For people turning 65 this year, each country's chance of reaching 85, from the Actuary's own
 * monthly death rates (qMonth), all read at one block. Nothing here is precomputed. */
export async function oddsAt65(client: Client, actuary: Address): Promise<OddsRead> {
  const born = new Date().getUTCFullYear() - FROM;
  const list = countries.filter((c) => born >= c.birthYears[0] && born <= c.birthYears[1]);
  const ages = Array.from({ length: TO - FROM }, (_, i) => FROM + i);
  const sexes: Sex[] = ['female', 'male'];
  const calls = list.flatMap((c) =>
    sexes.flatMap((sex) => ages.map((a) => ({ address: actuary, abi: actuaryAbi, functionName: 'qMonth', args: [cohortKey(c.iso3 as Iso3, sex, born), toWad(a + 0.5), toWad(born + a + 1)] }) as const)),
  );
  const block = await getBlockNumber(client, { cacheTime: 0 });
  const chunks = Array.from({ length: Math.ceil(calls.length / CHUNK) }, (_, i) => calls.slice(i * CHUNK, (i + 1) * CHUNK));
  const q = (await Promise.all(chunks.map((contracts) => multicall(client, { multicallAddress: MULTICALL3, allowFailure: false, blockNumber: block, contracts })))).flat() as bigint[];
  // Same survival as the quote page's curve: a year survives (1 − qMonth)^12.
  const reach = (from: number) => q.slice(from, from + ages.length).reduce((alive, qm) => alive * (1 - fromWad(qm)) ** 12, 1);
  const rows = list.map((c, k) => ({ iso3: c.iso3 as Iso3, name: c.name, female: reach(k * 2 * ages.length), male: reach((k * 2 + 1) * ages.length) }));
  return { rows, born, block, calls: calls.length, batches: chunks.length };
}
