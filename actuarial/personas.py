#!/usr/bin/env python3
"""The README's worked examples, reproducible: Maria and her mother, quoted by the same engine code
the Actuary contract runs (`actuary-cli quote`), on both plans.

  python3 actuarial/personas.py

Market assumptions are the deployed ones (deploy/actuary_batches.py): SPY 6% log-return, 16% vol,
safe sleeve 3.5%; level plan priced at 3.5%, escalating plan 2 points lower. 512 paths, seed 7.
"""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
from paths import CLI, RUNS
PARAMS = json.loads((ROOT / 'config' / 'mortality.json').read_text())['params']
YEAR = 2026.75
PLANS = {'level': 0.035, 'escalating': 0.015}


def gm(iso_num, sex, birth_year):
    p = PARAMS[str((iso_num * 2 + sex) * 10000 + birth_year)]
    return [p['A'], p['B'], p['theta'], p['kappa']]


def quote(params, age, start, lump, monthly, rate, seed=7):
    args = [*params, age, YEAR, start, lump, monthly, 0.06, 0.16, 0.035, rate, 512, seed]
    return json.loads(subprocess.run([CLI, 'quote', *map(str, args)], capture_output=True, text=True, check=True).stdout)


PERSONAS = [
    # Maria: Filipino domestic helper in Singapore, born 1990 (36), $50 a month until 60.
    ('Maria (PHL F, 36; $50/mo to 60)', gm(608, 0, 1990), 36, 60, 0, 50),
    # Her mother: born 1966 (60); Maria puts in $3,000 once and $30 a month; income from 62.
    ('Her mother (PHL F, 60; $3,000 + $30/mo, from 62)', gm(608, 0, 1966), 60, 62, 3000, 30),
]


def main():
    report = {}
    for who, params, age, start, lump, monthly in PERSONAS:
        report[who] = {}
        print(who)
        for plan, rate in PLANS.items():
            q = quote(params, age, start, lump, monthly, rate)
            report[who][plan] = q
            s, e = q['income_start'], q['income_at_85']
            print(f"  {plan:<10}  income at start  P10 ${s[0]:.2f}  P50 ${s[1]:.2f}  P90 ${s[2]:.2f}\n"
                  f"              income at 85     P10 ${e[0]:.2f}  P50 ${e[1]:.2f}  P90 ${e[2]:.2f}\n"
                  f"              same starting income drawn alone runs out at {q['solo_runout_age_p50']:.1f} (median);"
                  f" {q['solo_outlive_probability']:.0%} chance she is still alive then")
    (RUNS / 'personas.json').write_text(json.dumps(report, indent=1))
    print(f"report -> {RUNS / 'personas.json'}")


if __name__ == '__main__':
    main()
