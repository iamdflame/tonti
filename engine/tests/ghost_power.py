#!/usr/bin/env python3
"""Measures the ghost-member detector (docs/protocol.md §6) with the real SPRT code, over many seeds:
false alarms on honest groups, how fast concealment is caught, and how both move when the group's
true mortality or its reporting delays differ from what the books assume.

  python3 engine/tests/ghost_power.py        (writes <runs>/ghost-detector.json)

A group is 3,300 Filipino women born 1956–1966 (300 per birth year) on the fitted mortality. By
default honest deaths become final 4–6 months after they happen (a report, then the 120-day
challenge window); hidden deaths never do within the horizon. The contract tests α = 0.1%, lag 5
months, H0 λ = 0.85 against H1 λ = 0.55 (recalibrated after skeptic review 2, which showed the
old H0 1.0 / H1 0.7 flagged honest groups healthier than the tables).
"""
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'actuarial'))
from paths import RUNS, CLI  # noqa: E402

SEEDS = range(1, 11)
RUNS_PER_SEED = 300
HORIZON = 36
HIDDEN = (0.0, 0.1, 0.2, 0.3, 0.4, 0.5)
# Robustness: honest groups (nothing hidden) that are healthier than the tables, or whose deaths
# take longer to become final; 5 seeds each.
HEALTH = (1.0, 0.9, 0.85, 0.8, 0.75, 0.7)
DELAYS = ((4, 3), (5, 4), (6, 4), (4, 9))  # final after 4–6, 5–8, 6–9 and 4–12 months


def spec():
    params = json.loads((ROOT / 'config' / 'mortality.json').read_text())['params']
    rows = []
    for b in range(1956, 1967):
        p = params[str((608 * 2 + 0) * 10_000 + b)]
        rows.append(f"{p['A']},{p['B']},{p['theta']},{p['kappa']},{b},300")
    return ';'.join(rows)


def cli(s, hidden, seed, runs=RUNS_PER_SEED, health=1.0, delay=(4, 3)):
    r = subprocess.run([str(CLI), 'ghosts', str(hidden), str(HORIZON), str(runs), str(seed), s, '0.001', '5', str(health), str(delay[0]), str(delay[1])],
                       capture_output=True, text=True, check=True)
    return json.loads(r.stdout)


def summarise(per_seed):
    mean = lambda k: sum(x[k] for x in per_seed) / len(per_seed)
    meds = [x['median_months'] for x in per_seed if x['median_months'] is not None]
    return {'flagged_12m': mean('flagged_12m'), 'flagged_24m': mean('flagged_24m'), 'flagged_36m': mean('flagged_36m'),
            'flagged_24m_by_seed': [x['flagged_24m'] for x in per_seed],
            'median_months': sorted(meds)[len(meds) // 2] if len(meds) > len(per_seed) // 2 else None}


def main():
    s = spec()
    path = RUNS / 'ghost-detector.json'
    before = json.loads(path.read_text()) if path.exists() else {}
    out = {'pool': 'PHL women born 1956-1966, 300 per birth year (3,300 members)', 'horizon_months': HORIZON,
           'runs_per_seed': RUNS_PER_SEED, 'seeds': len(SEEDS), 'honest_report_lag_months': '4-6',
           'contract': {'alpha': 0.001, 'lag_months': 5, 'h0': 0.85, 'h1': 0.55}, 'power': [], 'robustness': [], 'healthy_fraud': []}
    for hidden in HIDDEN:
        row = {'hidden': hidden, **summarise([cli(s, hidden, seed) for seed in SEEDS])}
        out['power'].append(row)
        print(f"hidden {hidden:.0%}: flagged in 12m {row['flagged_12m']:.2%}, 24m {row['flagged_24m']:.2%}, 36m {row['flagged_36m']:.2%}, median {row['median_months']}", flush=True)
    for health in HEALTH:
        for delay in DELAYS:
            row = {'health': health, 'final_after_months': [delay[0], delay[0] + delay[1] - 1], **summarise([cli(s, 0.0, seed, health=health, delay=delay) for seed in SEEDS[:5]])}
            out['robustness'].append(row)
            print(f"honest, mortality {health:.0%} of tables, final after {delay[0]}-{delay[0] + delay[1] - 1} months: 24m {row['flagged_24m']:.2%}, 36m {row['flagged_36m']:.2%}", flush=True)
    for hidden in (0.3, 0.5):
        row = {'health': 0.85, 'hidden': hidden, **summarise([cli(s, hidden, seed, health=0.85) for seed in SEEDS[:5]])}
        out['healthy_fraud'].append(row)
        print(f"mortality 85% of tables, {hidden:.0%} hidden: 24m {row['flagged_24m']:.2%}, 36m {row['flagged_36m']:.2%}", flush=True)
    # The designs this one replaced, as measured at the time.
    out['history'] = before.get('history', []) + [{**x, 'hypotheses': 'H0 1.0 / H1 0.7'} for x in before.get('settings', [])]
    path.write_text(json.dumps(out, indent=1))


if __name__ == '__main__':
    main()
