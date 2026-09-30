#!/usr/bin/env python3
"""Deploys Tonti to Robinhood Chain, resumably.

  python3 deploy/deploy.py <key-file> <phase> [--countries PHL,IDN,...]

Phases, in order (each one skips whatever config/deployment.json says is already done, so a run
that stops for lack of gas simply resumes when the wallet is topped up):
  actuary     deploy + init the Actuary, set the market assumptions (a deployment of an older
              Actuary version is retired: moved to `retired` in deployment.json)
  mortality   load mortality cohorts, country by country (--countries, in priority order)
  seal        freeze the mortality tables for good (after this no owner can change them)
  core        deploy the TontiPool, Treasury, LifeRegistry, AttestedIdentity and wire them (a core
              built on an older Actuary is retired with it)
  handover    deploy the 48-hour TimelockController (the deployer proposes, anyone executes),
              make the deployer guardian (pause only), and hand every contract to the timelock.
  status      print what's deployed and the wallet's balance

The Stylus contracts are built with TONTI_DEPLOYER set to the deployer's address: only that address
can call their `init`, so nobody can front-run the set-up. Every `init` is checked right after.

The key file must be mode 600: one hex key, or a line `PRIVATE_KEY=0x...` (a .env). The key is
never printed; cargo-stylus reads it from a RAM file that is removed on exit. Foundry takes it as
an argument, visible to this user's other processes while a command runs.
"""
import atexit
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'actuarial'))
from paths import RUNS, target_dir  # noqa: E402

RPC = os.environ.get('ROBINHOOD_RPC', os.environ.get('RPC', 'https://rpc.mainnet.chain.robinhood.com'))
CFG = json.loads((ROOT / 'config' / 'robinhood-mainnet.json').read_text())
STATE = Path(os.environ.get('DEPLOYMENT_OUT', ROOT / 'config' / 'deployment.json'))
LOG = RUNS / 'artifacts' / 'deploy-log.jsonl'
PRIORITY = ['PHL', 'IDN', 'IND', 'BGD', 'MMR', 'LKA', 'NPL', 'VNM', 'THA', 'MYS', 'CHN', 'SGP', 'GHA']
ENV = {**os.environ, 'CARGO_TARGET_DIR': str(target_dir()), 'CARGO_INCREMENTAL': '0',
       'FOUNDRY_OUT': '/dev/shm/arbit-target/forge-out', 'FOUNDRY_CACHE_PATH': '/dev/shm/arbit-target/forge-cache'}
ZERO = '0x0000000000000000000000000000000000000000'
ACTUARY_VERSION = 3  # 3: Ghana added (13 countries); 2: sealed, fee, bounds
CORE_KEYS = ('pool', 'poolActuary', 'treasury', 'lifeRegistry', 'attestedIdentity', 'timelock', 'governance', 'deployBlock')


def load_key(path):
    p = Path(path)
    if stat.S_IMODE(p.stat().st_mode) & 0o077:
        sys.exit(f'{p} must be chmod 600')
    text = p.read_text().strip()
    m = re.search(r'PRIVATE_KEY\s*=\s*"?(0x)?([0-9a-fA-F]{64})', text) or re.fullmatch(r'(0x)?([0-9a-fA-F]{64})', text)
    if not m:
        sys.exit(f'{p}: no 32-byte hex private key found')
    return '0x' + m.group(2)


class Deployer:
    def __init__(self, key):
        self.key = key
        self.state = json.loads(STATE.read_text()) if STATE.exists() else {}
        self.address = self.cast('wallet', 'address', '--private-key', key, rpc=False).strip()
        chain = int(self.cast('chain-id').strip())
        if chain not in (4663, 46630):
            sys.exit(f'not Robinhood Chain: {chain}')
        self.state.update({'chainId': chain, 'deployer': self.address, 'usdg': CFG['tokens']['USDG']['address']})
        # Compiled into the Stylus contracts: only this address may initialise them.
        ENV['TONTI_DEPLOYER'] = self.address
        self.spent = 0
        LOG.parent.mkdir(parents=True, exist_ok=True)

    # ---------------------------------------------------------------- plumbing

    def redact(self, s):
        return s.replace(self.key, '<key>').replace(self.key[2:], '<key>')

    def run(self, args, cwd=None):
        r = subprocess.run(args, cwd=cwd, env=ENV, capture_output=True, text=True)
        if r.returncode != 0:
            raise RuntimeError(self.redact(f'{" ".join(args[:3])}… failed:\n{r.stderr.strip()[-1500:]}\n{r.stdout.strip()[-800:]}'))
        return r.stdout

    def cast(self, *args, rpc=True):
        return self.run(['cast', *args, *(['--rpc-url', RPC] if rpc else [])])

    def balance(self):
        return int(self.cast('balance', self.address).strip())

    def save(self):
        STATE.write_text(json.dumps(self.state, indent=1) + '\n')

    def record(self, what, receipt):
        gas = int(receipt['gasUsed'], 16)
        price = int(receipt.get('effectiveGasPrice', '0x0'), 16)
        l1 = int(receipt.get('gasUsedForL1', '0x0'), 16)
        cost = gas * price
        self.spent += cost
        entry = {'what': what, 'tx': receipt['transactionHash'], 'gas': gas, 'l1_gas': l1, 'eth': cost / 1e18, 'at': int(time.time())}
        with LOG.open('a') as f:
            f.write(json.dumps(entry) + '\n')
        print(f'  {what}: {gas:,} gas, {cost / 1e18:.8f} ETH  {receipt["transactionHash"]}', flush=True)

    def send(self, what, to, sig, *args):
        out = self.cast('send', to, sig, *args, '--private-key', self.key, '--json')
        r = json.loads(out)
        if int(r['status'], 16) != 1:
            raise RuntimeError(f'{what} reverted: {r["transactionHash"]}')
        self.record(what, r)
        return r

    def call(self, to, sig, *args):
        return [line.split()[0] for line in self.cast('call', to, sig, *args).splitlines() if line.strip()]

    def need(self, eth, what):
        have = self.balance() / 1e18
        if have < eth:
            self.save()
            sys.exit(f'stopping before {what}: {have:.6f} ETH left, needs about {eth:.6f}. Top up and rerun; finished steps are skipped.')

    def forge_create(self, what, contract, *ctor):
        out = self.run(['forge', 'create', contract, '--rpc-url', RPC, '--private-key', self.key, '--broadcast', '--json',
                        *(['--constructor-args', *ctor] if ctor else [])], cwd=ROOT / 'contracts')
        d = json.loads(out)
        receipt = json.loads(self.cast('receipt', d['transactionHash'], '--json'))
        self.record(what, receipt)
        return d['deployedTo']

    def stylus_deploy(self, what, crate):
        fd, path = tempfile.mkstemp(dir='/dev/shm', prefix='tonti-key-')
        os.write(fd, self.key.encode())
        os.close(fd)
        os.chmod(path, 0o600)
        atexit.register(lambda: os.path.exists(path) and os.remove(path))
        try:
            before = self.balance()
            # A fee cap, not a price: EIP-1559 charges base fee + tip. Without it cargo-stylus caps at
            # the base fee it just read, and a tick up in the base fee rejects the transaction.
            out = self.run(['cargo', 'stylus', 'deploy', '--endpoint', RPC, '--private-key-path', path, '--no-verify',
                            '--max-fee-per-gas-gwei', '0.05'], cwd=ROOT / 'engine' / crate)
        finally:
            os.remove(path)
        clean = re.sub(r'\x1b\[[0-9;]*m', '', out)
        with (LOG.parent / f'{crate}-deploy.log').open('a') as f:
            f.write(clean)
        m = re.findall(r'deployed code at address:?\s*(0x[0-9a-fA-F]{40})', clean, re.I)
        if not m:
            raise RuntimeError(f'no address in cargo stylus output:\n{clean[-1500:]}')
        cost = before - self.balance()
        self.spent += cost
        with LOG.open('a') as f:
            f.write(json.dumps({'what': what, 'address': m[-1], 'eth': cost / 1e18, 'at': int(time.time())}) + '\n')
        print(f'  {what}: {m[-1]}  (deploy + activation {cost / 1e18:.8f} ETH)', flush=True)
        return m[-1]

    # ---------------------------------------------------------------- phases

    def owned(self, what, address):
        """Stops unless `address` is owned by the deployer: nothing is wired to a contract someone
        else initialised."""
        owner = self.call(address, 'owner()(address)')[0].lower()
        if owner != self.address.lower():
            self.save()
            sys.exit(f'{what} at {address} is owned by {owner}, not the deployer: refusing to continue')

    def actuary(self):
        s = self.state
        if 'actuary' in s and s.get('actuaryVersion', 1) < ACTUARY_VERSION:
            # An older Actuary stays on-chain, retired: nothing new is wired to it.
            old = {k: s.pop(k) for k in ('actuary', 'market', 'countries', 'batches', 'sealed') if k in s}
            old['version'] = s.pop('actuaryVersion', 1)
            s.setdefault('retired', []).append(old)
            self.save()
        if 'actuary' not in s:
            self.need(0.0006, 'the Actuary deployment')
            s['actuary'] = self.stylus_deploy('Actuary', 'actuary-stylus')
            s['actuaryVersion'] = ACTUARY_VERSION
            self.save()
        if self.call(s['actuary'], 'owner()(address)')[0].lower() == ZERO:
            self.send('Actuary.init', s['actuary'], 'init()')
        self.owned('Actuary', s['actuary'])
        if not s.get('market'):
            q = 2 ** 64
            market = {'spy_return': 0.06, 'spy_vol': 0.16, 'safe_rate': 0.035, 'valuation_rate': 0.035, 'pool_fee': 0.003}
            m = [int(round(v * q)) for v in market.values()]
            self.send('Actuary.setMarket', s['actuary'], 'setMarket(int256,int256,int256,int256,int256)', *map(str, m))
            s['market'] = market
            self.save()

    def mortality(self, countries):
        s = self.state
        params = json.loads((ROOT / 'config' / 'mortality.json').read_text())['params']
        done = set(s.setdefault('countries', []))
        for iso in countries:
            if iso in done:
                continue
            for sex in ('female', 'male'):
                tag = f'{iso}-{sex}'
                if tag in s.setdefault('batches', []):
                    continue
                rows = sorted((int(k), p['q64']) for k, p in params.items() if p['iso3'] == iso and p['sex'] == sex)
                if not rows:
                    sys.exit(f'no fitted mortality for {tag}')
                self.need(0.0002, f'mortality {tag}')
                arr = lambda xs: '[' + ','.join(str(x) for x in xs) + ']'
                self.send(f'mortality {tag} ({len(rows)} cohorts)', s['actuary'], 'setMortalityBatch(uint256[],int256[],int256[],int256[],int256[])',
                          arr(k for k, _ in rows), arr(q['A'] for _, q in rows), arr(q['B'] for _, q in rows),
                          arr(q['theta'] for _, q in rows), arr(q['kappa'] for _, q in rows))
                s['batches'].append(tag)
                self.save()
            s['countries'].append(iso)
            self.save()

    def seal(self):
        s = self.state
        if self.call(s['actuary'], 'isSealed()(bool)')[0] != 'true':
            missing = [c for c in PRIORITY if c not in s.get('countries', [])]
            if missing:
                sys.exit(f'refusing to seal: mortality not loaded for {missing}')
            self.send('Actuary.seal', s['actuary'], 'seal()')
        s['sealed'] = True
        self.save()

    def core(self):
        s, c = self.state, CFG
        usdg = c['tokens']['USDG']['address']
        if not s.get('sealed'):
            sys.exit('seal the Actuary\'s mortality first (phase: seal)')
        # A pool's Actuary is fixed at init, so a new Actuary needs a new core. The old one stays
        # on-chain with the Actuary it was built on (a record from before `poolActuary` was kept
        # belongs to the Actuary retired last).
        if 'pool' in s and s.get('poolActuary') != s['actuary'] and self.call(s['pool'], 'owner()(address)')[0].lower() != ZERO:
            home = next((r for r in s.get('retired', []) if r.get('actuary') == s.get('poolActuary')), None) or s['retired'][-1]
            home.update({k: s.pop(k) for k in CORE_KEYS if k in s})
            self.save()
        if 'pool' not in s:
            self.need(0.0008, 'the TontiPool deployment')
            s['deployBlock'] = int(self.cast('block-number').strip())  # event scans start here
            s['pool'] = self.stylus_deploy('TontiPool', 'pool-stylus')
            self.save()
        if 'treasury' not in s:
            pm = c['uniswapV4']['poolManager']
            s['treasury'] = self.forge_create(
                'Treasury', 'src/Treasury.sol:Treasury', usdg, c['morpho']['steakhouseUSDG']['address'], c['tokens']['SGOV']['address'],
                c['tokens']['SPY']['address'], c['chainlink']['SGOV_USD']['address'], c['chainlink']['SPY_USD']['address'], pm,
                f"({usdg},{c['tokens']['SGOV']['address']},375,4,{ZERO})", f"({c['tokens']['SPY']['address']},{usdg},500,5,{ZERO})")
            self.save()
        if 'lifeRegistry' not in s:
            s['lifeRegistry'] = self.forge_create('LifeRegistry', 'src/LifeRegistry.sol:LifeRegistry', usdg)
            self.save()
        if 'attestedIdentity' not in s:
            s['attester'] = s.get('attester', self.address)
            s['attestedIdentity'] = self.forge_create('AttestedIdentity', 'src/AttestedIdentity.sol:AttestedIdentity', s['attester'], s['lifeRegistry'])
            self.save()
        if self.call(s['pool'], 'owner()(address)')[0].lower() == ZERO:
            s['poolActuary'] = s['actuary']
            self.save()
            self.send('TontiPool.init', s['pool'], 'init(address,address,address,address)', s['treasury'], s['lifeRegistry'], s['actuary'], usdg)
        for name in ('pool', 'treasury', 'lifeRegistry'):
            self.owned(name, s[name])
        # The verifier before the pool: until a pool is connected nobody is enrolled, so the
        # registry lets a verifier act at once; afterwards a new one waits 30 days.
        if self.call(s['lifeRegistry'], 'strongVerifiers(address)(bool)', s['attestedIdentity'])[0] != 'true':
            self.send('LifeRegistry.setVerifier', s['lifeRegistry'], 'setVerifier(address,bool)', s['attestedIdentity'], 'true')
        if self.call(s['treasury'], 'pool()(address)')[0].lower() == ZERO:
            self.send('Treasury.setPool', s['treasury'], 'setPool(address)', s['pool'])
        if self.call(s['lifeRegistry'], 'pool()(address)')[0].lower() == ZERO:
            self.send('LifeRegistry.setPool', s['lifeRegistry'], 'setPool(address)', s['pool'])
        self.save()

    def handover(self):
        s = self.state
        if 'timelock' not in s:
            # The deployer proposes (and may cancel); anyone may execute once the 48 hours have run.
            s['timelock'] = self.forge_create('TimelockController', 'lib/openzeppelin-contracts/contracts/governance/TimelockController.sol:TimelockController',
                                              '172800', f'[{self.address}]', f'[{ZERO}]', ZERO)
            self.save()
        if self.call(s['pool'], 'guardian()(address)')[0].lower() == ZERO:
            self.send('TontiPool.setGuardian', s['pool'], 'setGuardian(address)', self.address)
        for name in ('actuary', 'pool', 'treasury', 'lifeRegistry'):
            if self.call(s[name], 'owner()(address)')[0].lower() != s['timelock'].lower():
                self.send(f'{name}.transferOwnership', s[name], 'transferOwnership(address)', s['timelock'])
        s['governance'] = 'timelock-48h'
        self.save()

    def status(self):
        print(json.dumps(self.state, indent=1))
        print(f'wallet {self.address}: {self.balance() / 1e18:.8f} ETH')


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    key, phase = load_key(sys.argv[1]), sys.argv[2]
    countries = PRIORITY
    if '--countries' in sys.argv:
        countries = [c for c in sys.argv[sys.argv.index('--countries') + 1].split(',') if c]
    d = Deployer(key)
    print(f'deployer {d.address} on chain {d.state["chainId"]}: {d.balance() / 1e18:.8f} ETH', flush=True)
    try:
        {'actuary': d.actuary, 'mortality': lambda: d.mortality(countries), 'seal': d.seal, 'core': d.core,
         'handover': d.handover, 'status': d.status}[phase]()
    finally:
        d.save()
        if d.spent:
            print(f'spent this run: {d.spent / 1e18:.8f} ETH; left {d.balance() / 1e18:.8f} ETH')


if __name__ == '__main__':
    main()
