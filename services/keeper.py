#!/usr/bin/env python3
"""Tonti keeper: runs the pool's monthly duties on Robinhood Chain.

  python3 services/keeper.py <private-key-file> [--once]

Every 15 minutes it:
  1. queues every member whose death the LifeRegistry has made final (`markDead`);
  2. if the epoch has elapsed (or a settlement is part-way), pushes `settleSteps(PAGE)` until the
     epoch is settled, then `rebalanceSteps(PAGE)` the same way. Each page is at most PAGE deaths,
     cohorts or deposits, so no pool size can outgrow a block. Every send is dry-run with eth_call
     first; the Treasury refuses stale prices, so a new epoch only starts in US market hours;
  3. pushes income to every member who can receive it (`claim` pays to the member's own payout
     address), so income arrives like a pension without the retiree doing anything;
  4. queues every exit whose 12-month notice has run (`exit`), so a member who gave notice a year
     ago doesn't have to come back to finish leaving. The pool still checks they're alive and
     identified, and that income hasn't started;
  5. settles death reports whose 120-day window has closed (`resolveReport`: rejected if the member
     checked in since, final otherwise), and returns reporters' bonds after their one-year hold
     (`releaseBond`);
  6. queues the repayment of every member whose death was undone (`restore`, after `revive`), and
     applies account recoveries once their 14-day delay has passed (`finishRecovery`);
  7. abandons a rebalance stuck for a day (`abortRebalance`), so a paused token or a dry pool can't
     stop anyone's income.

All actions are permissionless: anyone can run a keeper. It needs only gas money and never holds
member funds. Uses `cast` only (no extra dependencies). State: <runs>/keeper-state.json.
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
import sys as _sys
_sys.path.insert(0, str(__import__('pathlib').Path(__file__).resolve().parents[1] / 'actuarial'))
from paths import RUNS  # noqa: E402
STATE = RUNS / 'keeper-state.json'
# The public endpoint is rate-limited; production should set ROBINHOOD_RPC (e.g. an Alchemy URL).
RPC = os.environ.get('ROBINHOOD_RPC', 'https://rpc.mainnet.chain.robinhood.com')
EXIT_REQUESTED, QUEUED_EXIT, RELEASED, EXITED, QUEUED_RESTORE, RESTORED, OWED = 8, 16, 4, 32, 256, 512, 1024  # pool member flags
ESTATE_HOLD = 365 * 86_400  # a reported death's estate waits this long (TontiPool.pay_estate)
PAGE = 100         # items per transaction: ink-meter measured the dearest page of 50 at 4.9M gas,
                   # so 100 stays near 10M, a third of the 32M per-transaction cap
MAX_PAGES = 500    # per tick, so a stuck run can't loop forever


def cast(*args, check=True):
    r = subprocess.run(['cast', *args, '--rpc-url', RPC], capture_output=True, text=True)
    if check and r.returncode != 0:
        raise RuntimeError(r.stderr.strip()[:300])
    return r


def call(to, sig, *args):
    return cast('call', to, sig, *args).stdout.split()


def values(to, sig, *args):
    """One value per returned field: cast prints each on its own line, big ones annotated
    ("24801587301587301587 [2.48e19]"), so take the first token of each line."""
    return [line.split()[0] for line in cast('call', to, sig, *args).stdout.splitlines() if line.strip()]


def log(msg):
    print(time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), msg, flush=True)


def send(pk, to, sig, *args):
    """Dry-run with eth_call first; send only if it succeeds. Returns the tx hash or None."""
    sim = cast('call', to, sig, *args, '--from', cast('wallet', 'address', '--private-key', pk).stdout.strip(), check=False)
    if sim.returncode != 0:
        return None
    r = cast('send', to, sig, *args, '--private-key', pk, '--json', check=False)
    if r.returncode != 0:
        log(f'send {sig} failed: {r.stderr.strip()[:200]}')
        return None
    return json.loads(r.stdout)['transactionHash']


def tick(pk, dep, state):
    pool, registry = dep['pool'], dep['lifeRegistry']
    head = int(cast('block-number').stdout)
    # 1. Deaths made final since the last scan.
    since = state.get('death_scan_from', head - 50_000)
    topic = cast('keccak', 'Deceased(uint256,uint64,bool)').stdout.strip()
    logs = json.loads(cast('logs', '--from-block', str(since), '--to-block', str(head), '--address', registry, topic, '--json').stdout or '[]')
    for lg in logs:
        member = int(lg['topics'][1], 16)
        if send(pk, pool, 'markDead(uint256)', str(member)):
            log(f'queued death of member {member}')
        state.setdefault('estates', [])
        if member not in state['estates']:
            state['estates'].append(member)
    # Death reports and held bonds: remembered until settled.
    for event, key in (('DeathReported(uint256,address,uint64,bytes32)', 'reports'), ('Revived(uint256,address)', 'revived'),
                       ('RecoveryRequested(uint256,address,uint64,address)', 'recoveries')):
        t = cast('keccak', event).stdout.strip()
        found = json.loads(cast('logs', '--from-block', str(since), '--to-block', str(head), '--address', registry, t, '--json').stdout or '[]')
        state.setdefault(key, [])
        state[key] = sorted(set(state[key]) | {int(lg['topics'][1], 16) for lg in found})
    state['death_scan_from'] = head + 1
    for m in list(state['reports']):
        if send(pk, registry, 'resolveReport(uint256)', str(m)):
            log(f'settled the death report on member {m}')
            state.setdefault('bonds', []).append(m)
        if values(registry, 'reports(uint256)(address,uint64,uint64,bytes32,uint256)', str(m))[0].lower() == '0x' + '0' * 40:
            state['reports'].remove(m)
    for m in list(state.get('bonds', [])):
        if send(pk, registry, 'releaseBond(uint256)', str(m)):
            log(f'returned the bond for member {m}')
        if int(values(registry, 'heldBonds(uint256)(address,uint64,uint256)', str(m))[2]) == 0:
            state['bonds'].remove(m)
    for m in list(state['revived']):
        flags = int(values(pool, 'member(uint256)(uint256,uint256,uint256,uint256,uint256,uint256,uint256)', str(m))[6])
        # First repayment, or a top-up as the reserve refills (the rest of a release stays owed).
        first = flags & RELEASED and not flags & (EXITED | RESTORED)
        if (first or flags & OWED) and not flags & QUEUED_RESTORE and send(pk, pool, 'restore(uint256)', str(m)):
            log(f'queued the repayment of revived member {m}')
            flags |= QUEUED_RESTORE
        # Once back in the pool, a reported death's held estate goes back to them as well.
        held = int(values(pool, 'estateOf(uint256)(uint256,uint256)', str(m))[0])
        if held and not flags & (RELEASED | QUEUED_RESTORE) and send(pk, pool, 'payEstate(uint256)', str(m)):
            log(f'returned the held estate of revived member {m}')
            held = 0
        # Done when nothing more is owed or held; or if a later death cleared the revival (the
        # pool refuses to repay a member the registry says is dead).
        revived = int(values(registry, 'revivedAt(uint256)(uint64)', str(m))[0])
        if revived == 0 or (not flags & (RELEASED | OWED | QUEUED_RESTORE) and held == 0):
            state['revived'].remove(m)
    # A reported death's estate goes to the beneficiary after a year (if nobody revived).
    for m in list(state.get('estates', [])):
        held, since = (int(x) for x in values(pool, 'estateOf(uint256)(uint256,uint256)', str(m))[:2])
        if held == 0 and since > 0:
            state['estates'].remove(m)  # paid, or returned to a revived member
        elif held and time.time() >= since + ESTATE_HOLD and send(pk, pool, 'payEstate(uint256)', str(m)):
            log(f"paid member {m}'s estate to the beneficiary")
            state['estates'].remove(m)
        elif held == 0 and since == 0 and int(values(pool, 'member(uint256)(uint256,uint256,uint256,uint256,uint256,uint256,uint256)', str(m))[6]) & RELEASED:
            state['estates'].remove(m)  # a presumed death: paid at once
    # Recoveries apply after their 14-day delay unless the member cancelled with a check-in.
    for m in list(state.get('recoveries', [])):
        ready = int(values(registry, 'pendingRecovery(uint256)(address,bytes32,bytes32,uint64,address)', str(m))[3])
        if ready == 0:
            state['recoveries'].remove(m)
        elif time.time() >= ready and send(pk, registry, 'finishRecovery(uint256)', str(m)):
            log(f'applied the recovery of member {m}')
            state['recoveries'].remove(m)

    # 2. Settlement, then rebalance, page by page. A run left part-way (by an RPC hiccup or
    #    another keeper) is simply continued: every page picks up at the stored cursor.
    epoch, last, length = _epoch(pool)
    phase = _phase(pool)
    if (phase == 0 and time.time() >= last + length) or 0 < phase < 10:
        if drive(pk, pool, 'settleSteps(uint32)'):
            log(f'settled epoch {epoch + 1}')
        else:
            log('settlement due; waiting for fresh prices (market hours)')
    if _phase(pool) in (0, 10, 11, 12):
        if drive(pk, pool, 'rebalanceSteps(uint32)'):
            log('rebalanced')
    if _phase(pool) in (10, 11) and send(pk, pool, 'abortRebalance()'):
        log('abandoned a rebalance stuck for a day; settlement goes on')

    # 3. Push income to everyone who can receive it; 4. queue exits whose notice has run.
    members = int(call(pool, 'counts()(uint256,uint256)')[0])
    for m in range(members):
        owed = int(call(pool, 'owed(uint256)(uint256)', str(m))[0])
        if owed > 0 and send(pk, pool, 'claim(uint256)', str(m)):
            log(f'paid member {m}: {owed / 1e6:.6f} USDG')
        flags = int(values(pool, 'member(uint256)(uint256,uint256,uint256,uint256,uint256,uint256,uint256)', str(m))[6])
        if flags & EXIT_REQUESTED and not flags & QUEUED_EXIT and send(pk, pool, 'exit(uint256)', str(m)):
            log(f'queued the exit of member {m}')


def drive(pk, pool, sig):
    """Sends pages until the pool is idle again. True if at least one page went through."""
    sent = False
    for _ in range(MAX_PAGES):
        if not send(pk, pool, sig, str(PAGE)):
            return sent
        sent = True
        if _phase(pool) == 0:
            return True
    log(f'{sig}: still running after {MAX_PAGES} pages; continuing next tick')
    return sent


def _phase(pool):
    return int(call(pool, 'runState()(uint256,uint256,uint256,uint256,uint256,uint256,uint256)')[0])


def _epoch(pool):
    out = call(pool, 'epochInfo()(uint256,uint256,uint256)')
    vals = [int(x) for x in out if x.isdigit()]
    return vals[0], vals[1], vals[2]


def main():
    # A mode-600 file with one hex key or a `PRIVATE_KEY=` line (a .env); never printed.
    sys.path.insert(0, str(ROOT / 'deploy'))
    from deploy import load_key
    pk = load_key(sys.argv[1])
    dep = json.loads((ROOT / 'config' / 'deployment.json').read_text())
    state = json.loads(STATE.read_text()) if STATE.exists() else {}
    while True:
        try:
            tick(pk, dep, state)
            STATE.write_text(json.dumps(state))
        except Exception as e:  # keep running through RPC hiccups; every action is idempotent
            log(f'tick failed: {e}')
        if '--once' in sys.argv:
            return
        time.sleep(900)


if __name__ == '__main__':
    main()
