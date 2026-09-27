// A few bytes of JSON-RPC for what the landing page needs before anyone asks a question: the live
// block number. The full client (viem) loads only when a quote is asked for.
const RPCS = [process.env.NEXT_PUBLIC_ROBINHOOD_RPC, 'https://rpc.mainnet.chain.robinhood.com'].filter((u): u is string => !!u);

export async function blockNumber(): Promise<bigint> {
  let last: unknown;
  for (const url of RPCS) {
    try {
      const r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'eth_blockNumber', params: [] }) });
      const j = (await r.json()) as { result?: string };
      if (j.result) return BigInt(j.result);
    } catch (e) {
      last = e;
    }
  }
  throw last ?? new Error('no RPC answered');
}
