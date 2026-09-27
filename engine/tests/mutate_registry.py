#!/usr/bin/env python3
"""Mutation testing for the LifeRegistry: plants one bug at a time (each a flaw a review found, or
the fix for one) and requires the non-fork Solidity suites to fail. Skeptic review 3 showed five
of the second-round registry fixes could be undone without any test noticing.

  python3 engine/tests/mutate_registry.py [name ...]      (writes <runs>/mutations-registry.json)

The unmutated source is kept on disk while a mutation is applied, so a crash or reboot can't
leave a planted bug behind: the next run restores it first.
"""
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'actuarial'))
from paths import RUNS  # noqa: E402

SRC = ROOT / 'contracts' / 'src' / 'LifeRegistry.sol'
BACKUP = SRC.with_name('.LifeRegistry.sol.unmutated')

# (name, the bug, old text, mutated text)
MUTATIONS = [
    ('report-keeps-revival', 'a final reported death leaves an earlier revival on record',
     "        revivedAt[memberId] = 0; // a revival from an earlier death can't repay this one\n        delete pendingRecovery[memberId];\n        heldBonds",
     "        delete pendingRecovery[memberId];\n        heldBonds"),
    ('presume-keeps-revival', 'a presumption leaves an earlier revival on record',
     "        revivedAt[memberId] = 0; // a revival from an earlier death can't repay this one\n        reporterOf[memberId] = address(0); // a presumed death",
     "        reporterOf[memberId] = address(0); // a presumed death"),
    ('report-keeps-recovery', 'a final reported death leaves a pending recovery in place',
     "        revivedAt[memberId] = 0; // a revival from an earlier death can't repay this one\n        delete pendingRecovery[memberId];\n        heldBonds",
     "        revivedAt[memberId] = 0; // a revival from an earlier death can't repay this one\n        heldBonds"),
    ('recovery-delay-short', 'a recovery waits 14 days, less than a check-in period',
     'r.readyAt = uint64(block.timestamp) + challengeWindow;', 'r.readyAt = uint64(block.timestamp) + 14 days;'),
    ('checkin-no-cancel', "a check-in doesn't cancel a pending recovery",
     '        if (pendingRecovery[memberId].readyAt != 0) {', '        if (false) {'),
    ('recovery-request-is-proof', 'a recovery request counts as an identity proof, cancelled or not',
     '        lastRecoveryIssued[memberId] = r.issuedAt;\n',
     '        lastRecoveryIssued[memberId] = r.issuedAt;\n        lives[memberId].lastStrong = r.issuedAt;\n'),
    ('recovery-request-answers-reports', 'a recovery request (an attester statement alone) answers a death report',
     '        pendingRecovery[memberId] = r;\n', '        pendingRecovery[memberId] = r;\n        lastOwnProof[memberId] = uint64(block.timestamp);\n'),
    ('income-during-recovery', 'income keeps flowing while a recovery is pending',
     'block.timestamp <= uint256(l.lastStrong) + strongPeriod\n            && pendingRecovery[memberId].readyAt == 0;',
     'block.timestamp <= uint256(l.lastStrong) + strongPeriod\n            && true;'),
    ('statement-cannot-answer', "an identity statement doesn't answer a death report",
     '        l.lastProof = uint64(block.timestamp);\n        lastOwnProof[memberId] = uint64(block.timestamp);\n        l.lastStrong = issued;\n        emit StrongProof',
     '        l.lastProof = uint64(block.timestamp);\n        l.lastStrong = issued;\n        emit StrongProof'),
    ('guardians-answer-reports', 'guardian attestations answer a death report',
     '        if (lastOwnProof[memberId] > r.filedAt) {', '        if (l.lastProof > r.filedAt) {'),
    ('date-unclamped', 'a reporter may date the death before the last proof of life',
     '        if (dateOfDeath < lives[memberId].lastProof) dateOfDeath = lives[memberId].lastProof;\n', ''),
    ('duplicate-guardians', 'one address may fill several guardian slots',
     '                require(guardians[i] == address(0) || guardians[i] != guardians[j], "guardians");\n', ''),
    ('presume-keeps-report', 'a presumption leaves an open report (and its bond) in place',
     '        if (r.reporter != address(0)) {\n            delete reports[memberId];', '        if (false) {\n            delete reports[memberId];'),
    ('revive-from-stale-statement', 'a revival accepts a statement issued before the death became final',
     '        require(issued > deceasedAt[memberId], "stale");\n', ''),
    ('report-bond-to-reporter', "a rejected report's bond goes back to the reporter, not the member",
     '            require(usdg.transfer(l.payout, r.bond), "bond");\n            emit ReportRejected',
     '            require(usdg.transfer(r.reporter, r.bond), "bond");\n            emit ReportRejected'),
    # Skeptic review 4 (the first four survived its own mutation run).
    ('presume-keeps-recovery', 'a presumption leaves a pending recovery in place',
     "        reporterOf[memberId] = address(0); // a presumed death: nobody reported this one\n        delete pendingRecovery[memberId];\n",
     "        reporterOf[memberId] = address(0); // a presumed death: nobody reported this one\n"),
    ('guardians-neighbours-only', 'only neighbouring guardian slots must differ',
     '            for (uint256 j = i + 1; j < 3; j++) {', '            for (uint256 j = i + 1; j < 3 && j < i + 2; j++) {'),
    ('recovery-keeps-old-window', "a newer recovery request inherits the earlier request's window",
     '        r.readyAt = uint64(block.timestamp) + challengeWindow;',
     '        r.readyAt = pendingRecovery[memberId].readyAt != 0 ? pendingRecovery[memberId].readyAt : uint64(block.timestamp) + challengeWindow;'),
    ('applied-recovery-not-own-proof', "an applied recovery doesn't count as the member's own proof of life",
     '        lastOwnProof[memberId] = uint64(block.timestamp);\n        if (r.issuedAt > l.lastStrong)', '        if (r.issuedAt > l.lastStrong)'),
    ('presume-mid-recovery', 'a member whose recovery is pending is presumed dead for a lapsed identity',
     '            && pendingRecovery[memberId].readyAt == 0;\n        require(silent || unproven', ';\n        require(silent || unproven'),
    ('revive-keeps-reporter', 'a revival leaves the old reporter on record',
     '        revivedAt[memberId] = uint64(block.timestamp);\n        reporterOf[memberId] = address(0);\n', '        revivedAt[memberId] = uint64(block.timestamp);\n'),
    ('anyone-cancels-recovery', 'anyone may cancel a pending recovery',
     '        _owned(memberId); // only the payout address', '        _live(memberId);'),
    ('strong-period-shortenable', 'governance may shorten the identity period',
     'require(period >= 400 days && period <= 730 days, "strong");', 'require(period >= 180 days && period <= 730 days, "strong");'),
]


def run_suite():
    env = dict(os.environ)
    env.setdefault('FOUNDRY_OUT', '/dev/shm/arbit-target/forge-out')
    env.setdefault('FOUNDRY_CACHE_PATH', '/dev/shm/arbit-target/forge-cache')
    r = subprocess.run(['forge', 'test', '--no-match-path', 'test/*.fork.t.sol'], cwd=ROOT / 'contracts', env=env, capture_output=True, text=True)
    out = r.stdout + r.stderr
    compiled = 'Compiler run failed' not in out and 'Error: Compiler' not in out
    failing = [line.strip() for line in out.splitlines() if line.strip().startswith('[FAIL')]
    return compiled, r.returncode == 0, failing, out


def main():
    if BACKUP.exists():
        print(f'restoring {SRC} from an interrupted run', flush=True)
        SRC.write_text(BACKUP.read_text())
        BACKUP.unlink()
    original = SRC.read_text()
    BACKUP.write_text(original)

    def restore(*_):
        SRC.write_text(original)
        BACKUP.unlink(missing_ok=True)
        sys.exit(130)
    signal.signal(signal.SIGTERM, restore)
    signal.signal(signal.SIGINT, restore)
    only = set(sys.argv[1:])
    results = []
    try:
        for name, bug, old, new in MUTATIONS:
            if only and name not in only:
                continue
            if original.count(old) != 1:
                results.append({'mutation': name, 'bug': bug, 'outcome': 'pattern-missing'})
                print(f'{name}: pattern not found exactly once', flush=True)
                continue
            SRC.write_text(original.replace(old, new))
            t0 = time.time()
            compiled, passed, failing, out = run_suite()
            outcome = 'compile-error' if not compiled else ('SURVIVED' if passed else 'killed')
            results.append({'mutation': name, 'bug': bug, 'outcome': outcome, 'failing': failing[:3], 'seconds': round(time.time() - t0, 1)})
            print(f'{name}: {outcome} {failing[:2]}', flush=True)
            if not compiled:
                print(out[-1500:])
    finally:
        SRC.write_text(original)
        BACKUP.unlink(missing_ok=True)
    compiled, passed, _, _ = run_suite()
    summary = {'killed': sum(r['outcome'] == 'killed' for r in results), 'total': len(results),
               'baseline_passes': compiled and passed, 'results': results, 'at': int(time.time())}
    (RUNS / 'mutations-registry.json').write_text(json.dumps(summary, indent=1))
    print(json.dumps({k: summary[k] for k in ('killed', 'total', 'baseline_passes')}))
    sys.exit(0 if summary['killed'] == summary['total'] and summary['baseline_passes'] else 1)


if __name__ == '__main__':
    main()
