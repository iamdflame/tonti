#!/usr/bin/env python3
"""Measure what buying SPY/SGOV with USDG actually costs on Robinhood Chain's Uniswap v4.

For the most liquid pools found by uniswap_pools.py, quotes USDG -> token at several sizes with
the official v4 Quoter and compares the fill against the Chainlink price. Read-only.

  python3 recon/quote_depth.py
"""
import json
import subprocess
from pathlib import Path

RPC = 'https://rpc.mainnet.chain.robinhood.com'
QUOTER = '0x8Dc178eFB8111BB0973Dd9d722ebeFF267c98F94'
USDG = '0x5fc5360D0400a0Fd4f2af552ADD042D716F1d168'
FEEDS = {'SPY': '0x319724394D3A0e3669269846abE664Cd621f9f6A',
         'SGOV': '0xa0DF4ee0fFf975306345875E3548Fcc519577A11',
         'QQQ': '0x80901d846d5D7B030F26B480776EE3b29374C2ae'}
import sys as _sys
_sys.path.insert(0, str(__import__('pathlib').Path(__file__).resolve().parents[1] / 'actuarial'))
from paths import RUNS as _RUNS  # noqa: E402
RUNS = _RUNS / 'recon'
SIZES_USD = (100, 1_000, 10_000)
QUOTE_SIG = 'quoteExactInputSingle(((address,address,uint24,int24,address),bool,uint128,bytes))(uint256,uint256)'


def cast(*args):
    """Run cast; return one value per output line, without cast's `[1.2e3]` annotations."""
    r = subprocess.run(['cast', *args, '--rpc-url', RPC], capture_output=True, text=True)
    return [line.split()[0] for line in r.stdout.strip().splitlines()] if r.returncode == 0 else None


def chainlink_price(token):
    out = cast('call', FEEDS[token], 'latestRoundData()(uint80,int256,uint256,uint256,uint80)')
    return int(out[1]) / 1e8  # feeds use 8 decimals; price already includes the ERC-8056 multiplier


def main():
    report = {}
    for token in ('SPY', 'SGOV', 'QQQ'):
        pools = json.loads((RUNS / f'uniswap-v4-{token}-USDG.json').read_text())['pools']
        ref = chainlink_price(token)
        rows = []
        for p in [p for p in pools if p['liquidity'] > 0][:4]:
            key = f"({p['currency0']},{p['currency1']},{p['fee']},{p['tickSpacing']},{p['hooks']})"
            zero_for_one = p['currency0'].lower() == USDG.lower()  # selling USDG
            for usd in SIZES_USD:
                out = cast('call', QUOTER, QUOTE_SIG, f"({key},{str(zero_for_one).lower()},{usd * 10**6},0x)")
                if out is None:
                    rows.append({'pool': p['id'], 'usd': usd, 'error': 'quote reverted'})
                    continue
                tokens_out = int(out[0]) / 1e18
                fill = usd / tokens_out if tokens_out else float('inf')
                rows.append({'pool': p['id'], 'fee': p['fee'], 'hooks': p['hooks'], 'usd': usd,
                             'tokensOut': tokens_out, 'fillPrice': fill, 'chainlink': ref,
                             'costBps': round((fill / ref - 1) * 1e4, 1)})
        report[token] = rows
        print(f'{token}  (Chainlink {ref:.4f})')
        for r in rows:
            if 'error' in r:
                print(f"   pool {r['pool'][:12]}…  ${r['usd']:>6}: {r['error']}")
            else:
                print(f"   pool {r['pool'][:12]}… fee {r['fee']:>7}  ${r['usd']:>6}: fill {r['fillPrice']:.4f}  cost {r['costBps']:+.1f} bps")
    (RUNS / 'quote-depth.json').write_text(json.dumps(report, indent=1))


if __name__ == '__main__':
    main()
