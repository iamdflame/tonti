#!/usr/bin/env python3
"""Rank every Uniswap v4 pool for a token pair on Robinhood Chain by in-range liquidity.

Finds pools from PoolManager `Initialize` logs, then reads slot0 and liquidity from StateView
in batched eth_calls. Read-only. Output: one JSON file per pair under the runs directory.

  python3 recon/uniswap_pools.py SPY SGOV QQQ
"""
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

RPC = 'https://rpc.mainnet.chain.robinhood.com'
POOL_MANAGER = '0x8366a39CC670B4001A1121B8F6A443A643e40951'
STATE_VIEW = '0xF3334192D15450CdD385c8B70e03f9A6bD9E673b'
USDG = '0x5fc5360D0400a0Fd4f2af552ADD042D716F1d168'
TOKENS = {  # verified on-chain 2026-09-26: name, symbol, shared Robinhood beacon 0xe10b…1b00
    'SPY': '0x117cc2133c37B721F49dE2A7a74833232B3B4C0C',
    'SGOV': '0x92FD66527192E3e61d4DDd13322Aa222DE86F9B5',
    'QQQ': '0xD5f3879160bc7c32ebb4dC785F8a4F505888de68',
}
import sys as _sys
_sys.path.insert(0, str(__import__('pathlib').Path(__file__).resolve().parents[1] / 'actuarial'))
from paths import RUNS as _RUNS  # noqa: E402
OUT = _RUNS / 'recon'
HEADERS = {'Content-Type': 'application/json', 'User-Agent': 'Mozilla/5.0 (X11; Linux x86_64)'}
DYNAMIC_FEE = 0x800000


def cast(*args):
    return subprocess.check_output(['cast', *args]).decode().strip()


def post(payload, attempts=6):
    for i in range(attempts):
        try:
            req = urllib.request.Request(RPC, json.dumps(payload).encode(), HEADERS)
            return json.load(urllib.request.urlopen(req, timeout=180))
        except urllib.error.HTTPError as e:
            if e.code != 429 or i == attempts - 1:
                raise
            time.sleep(2 ** i)  # back off on rate limiting


def pools_for(token):
    c0, c1 = sorted([TOKENS[token], USDG], key=lambda a: int(a, 16))
    topic0 = cast('keccak', 'Initialize(bytes32,address,address,uint24,int24,address,uint160,int24)')
    pad = lambda a: '0x' + '0' * 24 + a[2:].lower()
    logs = post({'jsonrpc': '2.0', 'id': 1, 'method': 'eth_getLogs', 'params': [{
        'address': POOL_MANAGER, 'fromBlock': '0x0', 'toBlock': 'latest',
        'topics': [topic0, None, pad(c0), pad(c1)]}]})['result']
    pools = []
    for lg in logs:
        w = [lg['data'][2:][i:i + 64] for i in range(0, len(lg['data']) - 2, 64)]
        ts = int(w[1], 16)
        pools.append({'id': lg['topics'][1], 'currency0': c0, 'currency1': c1, 'fee': int(w[0], 16),
                      'tickSpacing': ts - 2 ** 256 if ts > 2 ** 255 else ts, 'hooks': '0x' + w[2][24:],
                      'createdBlock': int(lg['blockNumber'], 16)})
    return pools


def read_state(pools):
    sel_liq = cast('sig', 'getLiquidity(bytes32)')
    sel_slot0 = cast('sig', 'getSlot0(bytes32)')
    calls = []
    for i, p in enumerate(pools):
        for j, sel in enumerate((sel_liq, sel_slot0)):
            calls.append({'jsonrpc': '2.0', 'id': 2 * i + j, 'method': 'eth_call',
                          'params': [{'to': STATE_VIEW, 'data': sel + p['id'][2:]}, 'latest']})
    results = {}
    for k in range(0, len(calls), 40):  # small batches stay under the rate limit
        for r in post(calls[k:k + 40]):
            results[r['id']] = r.get('result', '0x')
        time.sleep(0.5)
    for i, p in enumerate(pools):
        liq, slot0 = results.get(2 * i, '0x'), results.get(2 * i + 1, '0x')
        p['liquidity'] = int(liq, 16) if len(liq) > 2 else 0
        p['sqrtPriceX96'] = int(slot0[2:66], 16) if len(slot0) >= 66 else 0
    return pools


def main(tokens):
    OUT.mkdir(parents=True, exist_ok=True)
    block = int(cast('block-number', '--rpc-url', RPC))
    for t in tokens:
        pools = read_state(pools_for(t))
        pools.sort(key=lambda p: p['liquidity'], reverse=True)
        live = [p for p in pools if p['liquidity'] > 0]
        path = OUT / f'uniswap-v4-{t}-USDG.json'
        path.write_text(json.dumps({'block': block, 'pair': f'{t}/USDG', 'pools': pools}, indent=1))
        print(f'{t}/USDG: {len(pools)} pools, {len(live)} with in-range liquidity -> {path}')
        for p in live[:6]:
            fee = 'dynamic' if p['fee'] & DYNAMIC_FEE else f"{p['fee'] / 1e4:g}%"
            print(f"   L={p['liquidity']:.3e}  fee={fee:>8}  ts={p['tickSpacing']:<6} hooks={p['hooks']}  id={p['id'][:18]}…")
        time.sleep(2)


if __name__ == '__main__':
    main(sys.argv[1:] or list(TOKENS))
