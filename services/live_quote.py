#!/usr/bin/env python3
"""The gasp test against the live Actuary on Robinhood Chain: a stranger's age, sex, country and
savings in; lifelong monthly income (P10/P50/P90) out, from a Monte Carlo run inside an eth_call.

  python3 services/live_quote.py [ISO3 sex birthYear startAge lump monthly]
  python3 services/live_quote.py --personas

Defaults to a Filipino woman born 1966: $3,000 now, $30 a month, income from 62. Records the result,
the wall time and the gas (`cast estimate`) in runs/mainnet-quote.json.

--personas quotes the README's people (both plans) on the live contract and writes
runs/personas-live.json: every persona number the README, the submission and the site show comes
from there, not from an off-chain run with a chosen seed. The quote clock is fixed to the start of
the day (UTC), so the numbers are reproducible for a day.
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'actuarial'))
from paths import RUNS  # noqa: E402

RPC = os.environ.get('ROBINHOOD_RPC', 'https://rpc.mainnet.chain.robinhood.com')
SIG = 'quote(uint256,uint256,uint256,uint256,uint256,uint256,uint32,bool)'
OUT = '(uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256)'
WAD = 10 ** 18
YEAR = 31_557_600

PERSONAS = {
    'maria': {'label': 'Maria: 36, from the Philippines, a domestic worker in Singapore; $50 a month until 60',
              'input': ('PHL', 'female', 1990, 60, 0, 50)},
    'mother': {'label': 'Her mother: 60, in the Philippines; $3,000 now and $30 a month, income from 62',
               'input': ('PHL', 'female', 1966, 62, 3000, 30)},
    'rahim': {'label': 'Rahim: 41, from Bangladesh, a construction worker in Singapore; $40 a month until 60',
              'input': ('BGD', 'male', 1985, 60, 0, 40)},
}


def deployment():
    return json.loads((ROOT / 'config' / 'deployment.json').read_text())


def key_of(iso, sex, born):
    params = json.loads((ROOT / 'config' / 'mortality.json').read_text())['params']
    num = {c['iso3']: c['isoNumeric'] for c in params.values()}[iso]
    return (num * 2 + (1 if sex == 'male' else 0)) * 10_000 + born


def quote(actuary, iso, sex, born, start, lump, monthly, escalating=False, at=None):
    at = at if at is not None else time.time()
    year = 1970 + at / YEAR
    args = [str(key_of(iso, sex, born)), str(int((year - born - 0.5) * WAD)), str(int(year * WAD)), str(int(start * WAD)),
            str(int(lump * 10 ** 6)), str(int(monthly * 10 ** 6)), '512', 'true' if escalating else 'false']
    t0 = time.time()
    out = subprocess.run(['cast', 'call', actuary, SIG + OUT, *args, '--rpc-url', RPC], capture_output=True, text=True, check=True).stdout
    seconds = time.time() - t0
    v = [int(line.split()[0]) for line in out.splitlines() if line.strip()]
    gas = int(subprocess.run(['cast', 'estimate', actuary, SIG, *args, '--rpc-url', RPC], capture_output=True, text=True, check=True).stdout.strip())
    return {
        'input': {'country': iso, 'sex': sex, 'born': born, 'start_age': start, 'lump_usd': lump, 'monthly_usd': monthly, 'escalating': escalating},
        'actuary': actuary, 'at': int(at), 'seconds': round(seconds, 2), 'gas': gas, 'paths': 512,
        'income_start': [x / 1e6 for x in v[0:3]], 'income_at_85': [x / 1e6 for x in v[3:6]],
        'solo_runout_age': v[6] / WAD, 'solo_outlive_probability': v[7] / WAD,
    }


def personas():
    dep = deployment()
    day = int(time.time()) // 86_400 * 86_400
    out = {'actuary': dep['actuary'], 'chainId': dep['chainId'], 'quote_clock': day, 'people': {}}
    for name, p in PERSONAS.items():
        out['people'][name] = {'label': p['label'], 'level': quote(dep['actuary'], *p['input'], False, day),
                               'escalating': quote(dep['actuary'], *p['input'], True, day)}
        lv = out['people'][name]['level']
        print(f"{name}: level P50 ${lv['income_start'][1]:.2f}/month (P10 ${lv['income_start'][0]:.2f}, P90 ${lv['income_start'][2]:.2f}); "
              f"alone it runs out at {lv['solo_runout_age']:.1f}, {lv['solo_outlive_probability']:.0%} chance alive then")
    (RUNS / 'personas-live.json').write_text(json.dumps(out, indent=1))


def main():
    if '--personas' in sys.argv:
        return personas()
    a = sys.argv[1:] or ['PHL', 'female', '1966', '62', '3000', '30']
    iso, sex, born, start, lump, monthly = a[0], a[1], int(a[2]), float(a[3]), float(a[4]), float(a[5])
    r = quote(deployment()['actuary'], iso, sex, born, start, lump, monthly)
    r['summary'] = (f"a {sex} born {born} in {iso} with ${lump:,.0f} + ${monthly:,.0f}/month gets ${r['income_start'][1]:.2f}/month for life from {start:g} "
                    f"(P10 ${r['income_start'][0]:.2f}, P90 ${r['income_start'][2]:.2f}); drawn alone the same income runs out at "
                    f"{r['solo_runout_age']:.1f}, with a {r['solo_outlive_probability']:.0%} chance of still being alive ({r['seconds']:.1f} s, {r['gas']:,} gas)")
    (RUNS / 'mainnet-quote.json').write_text(json.dumps(r, indent=1))
    print(r['summary'])


if __name__ == '__main__':
    main()
