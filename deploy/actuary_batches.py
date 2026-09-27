#!/usr/bin/env python3
"""Emit the Actuary's on-chain data as cast-ready argument lines.

  python3 deploy/actuary_batches.py [ISO3,ISO3,...] > batches.txt

Each output line is: <keys> <a> <b> <theta> <kappa>   (Solidity array literals, raw Q64.64)
for one (country, sex) group of 71 birth-year cohorts. The last line starts with MARKET and holds
the setMarket arguments: SPY return, SPY volatility, safe rate, valuation rate (raw Q64.64).
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
Q64 = 2 ** 64
# Disclosed capital-market assumptions (docs/protocol.md §4, §8).
MARKET = {'spy_return': 0.06, 'spy_vol': 0.16, 'safe_rate': 0.035, 'valuation_rate': 0.035}


def main():
    wanted = set(sys.argv[1].split(',')) if len(sys.argv) > 1 and sys.argv[1] else None
    params = json.loads((ROOT / 'config' / 'mortality.json').read_text())['params']
    groups = {}
    for key, p in params.items():
        if wanted and p['iso3'] not in wanted:
            continue
        groups.setdefault((p['iso3'], p['sex']), []).append((int(key), p['q64']))
    for (iso, sex), rows in sorted(groups.items()):
        rows.sort()
        arr = lambda f: '[' + ','.join(str(v) for v in f) + ']'
        print(arr(k for k, _ in rows), arr(q['A'] for _, q in rows), arr(q['B'] for _, q in rows),
              arr(q['theta'] for _, q in rows), arr(q['kappa'] for _, q in rows))
    print('MARKET', *(int(round(v * Q64)) for v in MARKET.values()))


if __name__ == '__main__':
    main()
