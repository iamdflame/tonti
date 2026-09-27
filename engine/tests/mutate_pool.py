#!/usr/bin/env python3
"""Mutation check for the pool's tests: re-introduces each bug the skeptic found (and a few that the
old lenient TestVM could not see), one at a time, and requires the named test to fail. A mutation
that survives means the suite can't catch that bug.

  python3 engine/tests/mutate_pool.py        (from the repo root; writes runs/mutations-pool.json)
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'actuarial'))
from paths import RUNS  # noqa: E402

LIB = ROOT / 'engine' / 'pool-stylus' / 'src' / 'lib.rs'

# (name, what the bug is, old text, mutated text, test that must fail)
MUTATIONS = [
    ('topup-old-snapshot', 'a top-up keeps the old income snapshot (the CRITICAL finding)',
     'let snap = topped_up_snap(u128_of(held)?, u128_of(snapped)?, units, fx_u(c.income_per_unit).to::<u128>())?;',
     'let snap = if held == U256::ZERO { fx_u(c.income_per_unit).to::<u128>() } else { u128_of(snapped)? };',
     'a_top_up_keeps_income_already_owed_and_earns_only_from_then'),
    ('exit-after-income-starts', 'exit() does not re-check that income has not started',
     'if now > at + EXIT_WINDOW || U256::from(age_wad_at(self.c_key.get(t), now)?) >= start {',
     'if now > at + EXIT_WINDOW {',
     'an_exit_notice_lapses_after_90_days_and_never_outlives_the_start_of_income'),
    ('exit-notice-never-lapses', 'a matured notice never expires',
     'if now > at + EXIT_WINDOW || U256::from(age_wad_at(self.c_key.get(t), now)?) >= start {',
     'if U256::from(age_wad_at(self.c_key.get(t), now)?) >= start {',
     'an_exit_notice_lapses_after_90_days_and_never_outlives_the_start_of_income'),
    ('init-front-run', 'anyone may call init first',
     'if self.owner.get() != Address::ZERO || sender != DEPLOYER {',
     'if self.owner.get() != Address::ZERO {',
     'only_the_deployer_can_initialise'),
    ('claim-pays-caller', 'claim pays the caller instead of the payout address',
     'let to = self.payout_of(member_id)?;\n            let treasury = ITreasury::new(self.treasury.get());',
     'let to = self.vm().msg_sender();\n            let treasury = ITreasury::new(self.treasury.get());',
     'paying_cohort_sells_its_payout_fraction_and_only_the_living_are_paid'),
    ('enroll-wrong-key', 'join enrolls the wrong cohort key in the registry',
     'ext(registry.enroll(self.vm(), ctx, id, key, qx, qy, payout, beneficiary, guardians))?;',
     'ext(registry.enroll(self.vm(), ctx, id, key + U256::from(1u8), qx, qy, payout, beneficiary, guardians))?;',
     'contributions_are_forward_priced_into_units_split_by_bequest_share'),
    ('estate-to-payout', 'a presumed death pays the estate to the payout address, not the beneficiary',
     'let heir = self.beneficiary_of(m)?;',
     'let heir = self.payout_of(m)?;',
     'a_presumed_death_funds_the_revival_reserve_and_a_revived_member_is_repaid'),
    ('held-estate-to-payout', "a reported death's held estate is paid to the payout address, not the beneficiary",
     'let heir = self.beneficiary_of(member_id)?;',
     'let heir = self.payout_of(member_id)?;',
     'a_death_credits_survivors_pays_the_heir_and_nothing_more_to_the_dead'),
    ('no-reserve-for-reports', 'reported deaths fund no revival reserve (so they could never be undone)',
     'let x = mul_div(released, RESERVE_BPS, 10_000).unwrap_or(0);\n            run.pool[a] -= x;\n            reserve[a] += x;\n            self.m_owed',
     'let x = if presumed { mul_div(released, RESERVE_BPS, 10_000).unwrap_or(0) } else { 0 };\n            run.pool[a] -= x;\n            reserve[a] += x;\n            self.m_owed',
     'a_reported_death_pays_no_bounty_funds_the_reserve_and_can_be_undone'),
    ('restore-presumed-only', 'only presumed deaths can be restored',
     'let first = flags & FLAG_RELEASED != 0 && flags & (FLAG_EXITED | FLAG_RESTORED) == 0;',
     'let first = flags & FLAG_PRESUMED != 0 && flags & (FLAG_EXITED | FLAG_RESTORED) == 0;',
     'a_reported_death_pays_no_bounty_funds_the_reserve_and_can_be_undone'),
    ('hold-without-grace', 'a flag holds income at once',
     'if flagged == U256::ZERO || U256::from(self.now()) < flagged + U256::from(HOLD_GRACE) {',
     'if flagged == U256::ZERO {',
     'the_ghost_detector_flags_a_group_with_no_deaths_and_holds_it_after_120_days_until_strong_proofs'),
    ('abort-anytime', 'a rebalance can be aborted without waiting a day',
     'if self.now() < u128_of(self.run_ts.get())? + REBALANCE_TIMEOUT {',
     'if false {',
     'a_stuck_rebalance_can_be_abandoned_after_a_day'),
    ('ghost-no-lag', 'the ghost detector tests each epoch against itself (flags every honest group at launch)',
     'let then = self.g_exp_ring.get(ring(epoch - lag));',
     'let then = self.g_exp_ring.get(ring(epoch));',
     'the_ghost_detector_flags_a_group_with_no_deaths_and_holds_it_after_120_days_until_strong_proofs'),
    ('exit-fee-zero', 'the 1% exit fee is dropped',
     'let keep = released - mul_div(released, EXIT_FEE_BPS, 10_000).unwrap_or(0);',
     'let keep = released;',
     'an_exit_after_notice_pays_the_member_and_leaves_one_percent_with_those_who_stay'),
    # Skeptic review 2 (2026-09-27): the first survived the whole suite; the others are its fixes.
    ('markdead-accepts-living', 'mark_dead queues a member the registry has not declared dead',
     'if s != STATUS_DECEASED && s != STATUS_PRESUMED {\n            return Err(PoolError::NotEligible(NotEligible {}));\n        }\n        self.m_flags.insert(member_id, U256::from(flags | FLAG_QUEUED_DEAD));',
     'if false {\n            return Err(PoolError::NotEligible(NotEligible {}));\n        }\n        self.m_flags.insert(member_id, U256::from(flags | FLAG_QUEUED_DEAD));',
     'only_a_death_the_registry_made_final_can_be_queued'),
    ('restore-stale-revival', 'restore trusts any revival on record, even while the registry says the member is dead',
     'if s == 0 || s == STATUS_DECEASED || s == STATUS_PRESUMED || ext(registry.revived_at(',
     'if ext(registry.revived_at(',
     'a_reported_death_pays_no_bounty_funds_the_reserve_and_can_be_undone'),
    ('restore-pays-cash', 'an undone death is repaid in cash to the payout address (an exit after income started)',
     'let mut f = flags & !(FLAG_RESTORED | FLAG_RELEASED | FLAG_QUEUED_DEAD | FLAG_PRESUMED);\n            if amount > 0 {',
     'let mut f = flags & !(FLAG_RESTORED | FLAG_RELEASED | FLAG_QUEUED_DEAD | FLAG_PRESUMED);\n            if amount > 0 {\n                let to = self.payout_of(m)?;\n                self.claimable.insert(to, self.claimable.get(to) + U256::from(amount));\n            }\n            if false {',
     'a_reported_death_pays_no_bounty_funds_the_reserve_and_can_be_undone'),
    ('pause-forever', 'a pause holds settlements for as long as it lasts',
     'if self.paused.get() && self.now() < u128_of(self.paused_at.get())? + PAUSE_MAX {',
     'if self.paused.get() {',
     'the_guardian_can_stop_new_business_but_not_money_already_owed'),
    ('bequest-pays-income', 'the bequest part is paid out at a life-contingent rate (and dwindles)',
     'if c.class == Class::Tontine && c.units > 0 && age >= U256::from(meta >> 8) * U256::from(WAD) {',
     'if c.units > 0 && age >= U256::from(meta >> 8) * U256::from(WAD) {',
     'settlement_one_item_per_transaction_equals_settlement_in_one'),
    # Skeptic review 3.
    ('release-ignores-revival', 'a member revived before the settlement is released anyway',
     '        if st != STATUS_DECEASED && st != STATUS_PRESUMED {\n            self.m_flags.insert(m, U256::from(flags & !FLAG_QUEUED_DEAD));\n            return Ok(());\n        }\n',
     '',
     'a_member_revived_before_the_settlement_is_not_released'),
    ('restore-erases-remainder', "what the reserve can't cover is written off",
     'self.m_owed.insert(key, U256::from(owed - pay));', 'self.m_owed.insert(key, U256::ZERO);',
     'a_revival_the_reserve_cannot_cover_yet_stays_owed_and_is_repaid_later'),
    ('estate-paid-at-once', "a reported death's estate is paid at once (a false report pays the family)",
     '        if presumed {\n            let heir = self.beneficiary_of(m)?;', '        if true {\n            let heir = self.beneficiary_of(m)?;',
     'a_death_credits_survivors_pays_the_heir_and_nothing_more_to_the_dead'),
    ('revived-estate-to-heir', "a revived member's held estate still goes to the beneficiary",
     '        if s == STATUS_DECEASED || s == STATUS_PRESUMED {\n            if self.now() < u128_of(self.m_estate_at.get(member_id))? + ESTATE_HOLD {',
     '        if true {\n            if self.now() < u128_of(self.m_estate_at.get(member_id))? + ESTATE_HOLD {',
     'a_reported_death_pays_no_bounty_funds_the_reserve_and_can_be_undone'),
    ('pause-refresh', 'pausing again while paused restarts the 7-day hold',
     '        if self.paused.get() {\n            return Ok(()); // already paused', '        if false {\n            return Ok(()); // already paused',
     'nobody_can_stretch_a_pause_past_seven_days'),
    ('owner-pause-anytime', 'the owner may pause again at any time',
     '        if last != 0 && self.now() < last + PAUSE_COOLDOWN {', '        if self.vm().msg_sender() != self.owner.get() && last != 0 && self.now() < last + PAUSE_COOLDOWN {',
     'nobody_can_stretch_a_pause_past_seven_days'),
    ('exit-ignores-pause', 'a matured exit goes through during a pause',
     '    pub fn exit(&mut self, member_id: U256) -> Result<(), PoolError> {\n        self.runs_allowed()?;\n',
     '    pub fn exit(&mut self, member_id: U256) -> Result<(), PoolError> {\n',
     'a_pause_holds_a_matured_exit_for_at_most_seven_days'),
]


def run_test(name):
    env = dict(os.environ)
    env.setdefault('CARGO_TARGET_DIR', '/dev/shm/arbit-target/engine')
    env['CARGO_INCREMENTAL'] = '0'
    args = ['cargo', 'test', '-p', 'pool-stylus', '--lib'] + (['--', '--exact', f'tests::{name}'] if name else [])
    r = subprocess.run(args,
                       cwd=ROOT / 'engine', env=env, capture_output=True, text=True)
    out = r.stdout + r.stderr
    compiled = 'error[' not in out and 'could not compile' not in out
    return compiled, r.returncode == 0, out


# The unmutated source, kept on disk while a mutation is applied, so a crash or reboot mid-run
# (it happened) can't leave a planted bug behind: the next run restores it first.
BACKUP = LIB.with_name('.lib.rs.unmutated')


def main():
    if BACKUP.exists():
        print(f'restoring {LIB} from an interrupted run', flush=True)
        LIB.write_text(BACKUP.read_text())
        BACKUP.unlink()
    original = LIB.read_text()
    BACKUP.write_text(original)
    # Restore the source even if the run is killed (the default SIGTERM skips `finally`).
    import signal
    def restore(*_):
        LIB.write_text(original)
        BACKUP.unlink(missing_ok=True)
        sys.exit(130)
    signal.signal(signal.SIGTERM, restore)
    signal.signal(signal.SIGINT, restore)
    results = []
    only = set(sys.argv[1:])
    try:
        for name, bug, old, new, test in MUTATIONS:
            if only and name not in only:
                continue
            if original.count(old) != 1:
                results.append({'mutation': name, 'bug': bug, 'test': test, 'outcome': 'pattern-missing'})
                print(f'{name}: pattern not found exactly once', flush=True)
                continue
            LIB.write_text(original.replace(old, new))
            t0 = time.time()
            compiled, passed, out = run_test(test)
            outcome = 'compile-error' if not compiled else ('SURVIVED' if passed else 'killed')
            results.append({'mutation': name, 'bug': bug, 'test': test, 'outcome': outcome, 'seconds': round(time.time() - t0, 1)})
            print(f'{name}: {outcome} ({test})', flush=True)
            if not compiled:
                print(out[-2000:])
    finally:
        LIB.write_text(original)
        BACKUP.unlink(missing_ok=True)
    # The unmutated suite must pass.
    compiled, passed, _ = run_test('')
    # Named mutations re-run after a fix replace their own rows in the last full run's results.
    path = RUNS / 'mutations-pool.json'
    if only and path.exists():
        rows = {r['mutation']: r for r in json.loads(path.read_text()).get('results', [])}
        rows.update({r['mutation']: r for r in results})
        results = [rows[n] for n, *_ in MUTATIONS if n in rows]
    summary = {'killed': sum(r['outcome'] == 'killed' for r in results), 'total': len(results),
               'baseline_passes': compiled and passed, 'results': results, 'at': int(time.time())}
    (RUNS / 'mutations-pool.json').write_text(json.dumps(summary, indent=1))
    print(json.dumps({k: summary[k] for k in ('killed', 'total', 'baseline_passes')}))
    sys.exit(0 if summary['killed'] == summary['total'] and summary['baseline_passes'] else 1)


if __name__ == '__main__':
    main()
