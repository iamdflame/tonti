#!/usr/bin/env python3
"""Every number the demo video shows, read from Robinhood Chain (and the repo's measured runs) at
build time. The video's overlays read only facts.json: nothing on screen is typed in by hand.

  python3 video/data/pull-facts.py            (writes video/data/facts.json)
"""
import json
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RPC = 'https://rpc.mainnet.chain.robinhood.com'
DEP = json.loads((ROOT / 'config' / 'deployment.json').read_text())
USDG = DEP['usdg']
MEMBER = '0x1CD2B147EfE092c3BdE0B474bCE3Bd33ae3dbB37'

# Member #0's life on mainnet, in order. Each is checked against its receipt below.
TXS = {
    'join': '0xb5ddd1965943c8dea83c8069c7853d3c8ae66dd21b520f6f84f7cb073a2e5a40',
    'identity': '0x8258fe18b6a0327720f56b678db8b61328bac20ee7f9a90b1977e77da1152595',
    'deposit': '0x26861ca62fe1c8a901905628f5bb915a0f30480496a4668586bea1c482be7b86',
    'settle': '0xb57edd8c01fe2007abb3a9688ae9609fc3e5061bd1a79b519f89d1efff7392a6',
    'rebalance': '0x97a8a6c10b18895bdffa1399070b100ed8ca9aa7af21072ca878118a42621bf2',
}


def cast(*args):
    for tries in range(5):
        r = subprocess.run(['cast', *args, '--rpc-url', RPC], capture_output=True, text=True)
        if r.returncode == 0:
            return r.stdout.strip()
        if '429' in r.stderr or 'Too Many' in r.stderr:
            time.sleep(2 ** tries)
            continue
        raise RuntimeError(f"cast {' '.join(args[:2])}: {r.stderr.strip()[:200]}")
    raise RuntimeError('rate limited')


def first(out):
    return [line.split()[0] for line in out.splitlines() if line.strip()]


def receipt(h):
    r = json.loads(cast('receipt', h, '--json'))
    block = int(r['blockNumber'], 16)
    ts = int(json.loads(cast('block', str(block), '--json'))['timestamp'], 16)
    return {'hash': h, 'block': block, 'gas': int(r['gasUsed'], 16), 'status': int(r['status'], 16), 'from': r['from'], 'time': ts,
            'logs': [{'address': l['address'], 'topic0': l['topics'][0]} for l in r['logs']]}


def checkins():
    """The CheckedIn events for member 0, from the registry's own logs."""
    topic = subprocess.run(['cast', 'keccak', 'CheckedIn(uint256,uint32)'], capture_output=True, text=True).stdout.strip()
    logs = json.loads(cast('logs', '--from-block', str(DEP['deployBlock']), '--address', DEP['lifeRegistry'], topic,
                           '0x' + '0' * 64, '--json'))
    return [receipt(l['transactionHash']) for l in logs]


def main():
    out = {'pulledAt': int(time.time()), 'rpc': RPC, 'chainId': DEP['chainId'], 'contracts': {
        k: DEP[k] for k in ('actuary', 'pool', 'treasury', 'lifeRegistry', 'attestedIdentity', 'timelock', 'usdg')}}
    out['deployBlock'] = DEP['deployBlock']
    out['block'] = int(cast('block-number'))
    pool, reg, tre = DEP['pool'], DEP['lifeRegistry'], DEP['treasury']

    out['txs'] = {k: receipt(h) for k, h in TXS.items()}
    for k, r in out['txs'].items():
        assert r['status'] == 1, f'{k} reverted'
    assert out['txs']['join']['from'].lower() == MEMBER.lower()
    out['checkins'] = checkins()

    members, cohorts = map(int, first(cast('call', pool, 'counts()(uint256,uint256)')))
    epoch, last_settle, epoch_len = map(int, first(cast('call', pool, 'epochInfo()(uint256,uint256,uint256)')))
    life = first(cast('call', reg, 'life(uint256)(uint256,bool,bytes32,bytes32,address,address,uint64,uint64,uint64,uint32,bool,bool,address[3])', '0'))
    period = int(first(cast('call', reg, 'checkInPeriod()(uint64)'))[0])
    out['pool'] = {'members': members, 'cohorts': cohorts, 'epoch': epoch, 'lastSettle': last_settle, 'epochLength': epoch_len,
                   'nextSettle': last_settle + epoch_len}
    out['member0'] = {'key': int(life[0]), 'identified': life[1] == 'true', 'lastProof': int(life[6]), 'nextCheckIn': int(life[6]) + period,
                      'country': 'GHA', 'born': int(life[0]) % 10000, 'sex': 'female' if (int(life[0]) // 10000) % 2 == 0 else 'male'}

    plain = lambda s: re.sub(r'\s*\[[0-9.e+-]+\]', '', s)  # drop cast's "[3.011e18]" annotations
    holdings = [int(x) for x in plain(cast('call', tre, 'holdings()(uint256[3])')).strip('[]').split(', ')]
    prices_raw = plain(cast('call', tre, 'lastPrices()(uint256[3],uint256)')).splitlines()
    prices = [int(x) for x in prices_raw[0].strip('[]').split(', ')]
    oldest = int(prices_raw[1].strip())
    sleeves = ['cash', 'tbills', 'sp500']
    out['holdings'] = {s: {'units': h / 1e18, 'price': p / 1e18, 'usd': round(h * p / 1e36, 4)} for s, h, p in zip(sleeves, holdings, prices)}
    out['holdings']['pricedAt'] = oldest
    value = int(first(cast('call', pool, 'memberValue(uint256,uint256,uint256,uint256)(uint256)', '0', *map(str, prices)))[0])
    out['member0']['valueUsd'] = round(value / 1e18, 4)
    total = sum(v['usd'] for k, v in out['holdings'].items() if isinstance(v, dict))
    out['holdings']['totalUsd'] = round(total, 4)
    out['holdings']['costBps'] = round((25 - total) / 25 * 1e4, 1)

    # Measured elsewhere in the repo (runs/ on the build machine): the quote's gas, tests, fit.
    runs = Path('/media/dflame/UNIQ/arbit/runs')
    def load(name):
        p = runs / name
        return json.loads(p.read_text()) if p.exists() else {}
    q = load('mainnet-quote.json')
    out['quote'] = {'example': q.get('input'), 'gas': q.get('gas'), 'seconds': q.get('seconds'), 'paths': q.get('paths'), 'incomeStart': q.get('income_start'),
                    'soloRunoutAge': q.get('solo_runout_age'), 'soloOutliveProbability': q.get('solo_outlive_probability'), 'actuary': q.get('actuary')}
    mp, mr = load('mutations-pool.json'), load('mutations-registry.json')
    out['tests'] = {'plantedBugs': (mp.get('killed', 0) + mr.get('killed', 0)), 'plantedTotal': (mp.get('total', 0) + mr.get('total', 0))}
    fit = load('mortality-validation.json')
    out['mortality'] = {'cohorts': len(json.loads((ROOT / 'config' / 'mortality.json').read_text())['params']), 'countries': len(DEP.get('countries', [])),
                        'worstAnnuityError': fit.get('worst_abs_annuity_error')}
    replay = json.loads((ROOT / 'web' / 'public' / 'data' / 'replay.json').read_text())
    c65 = replay.get('cohorts', {}).get('1965', {})
    out['replay1965'] = {k: c65.get(k) for k in ('shadow_runout_year', 'shadow_alive_share', 'rule4_runout_year')} | {'members': (replay.get('setup') or {}).get('members')}

    dest = Path(__file__).with_name('facts.json')
    dest.write_text(json.dumps(out, indent=1) + '\n')
    print(json.dumps({'member0': out['member0'], 'holdings': out['holdings'], 'pool': out['pool'], 'checkins': len(out['checkins']),
                      'quote_gas': out['quote']['gas'], 'tests': out['tests'], 'replay1965': out['replay1965']}, indent=1))


if __name__ == '__main__':
    sys.exit(main())
