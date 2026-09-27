#!/usr/bin/env python3
"""Validates the mortality fit where it sets payouts: the annuity factor from the ages income can
start (50, 55, …, 80, and the cohort's own age if it is older), against UN WPP 2024 read along the
cohort's diagonal. The skeptic review (2026-09-27) showed that measuring from each cohort's age in
2026 (age 30 for the young) dilutes the error with near-certain early years: 0.66% there, up to
3.1% from the ages that matter.

  python3 actuarial/validate_fit.py [config/mortality.json]

Prints the worst errors by start age and writes <runs>/mortality-validation.json.
"""
import collections
import json
import sys
from pathlib import Path

import fit_mortality as F
from paths import RUNS

START_AGES = (50, 55, 60, 65, 70, 75, 80)
# Income is paid at every age after the start, each month at 1/ä from the age reached, so the fit
# must hold from later ages too (skeptic review 2: 2.2% from 85, 4.7% from 90, 7.3% from 95).
PAID_AGES = (85, 90, 95)


def annuity_error(p, q, b, x0):
    """Relative error of the model's annuity factor ä (at 3.5%) from age x0 to 100 against WPP's,
    and the worst survival error on the way. None if WPP has no data along the path."""
    s_wpp = s_mod = 1.0
    a_wpp = a_mod = 0.0
    worst = 0.0
    for k in range(0, 100 - x0):
        disc = (1 + F.RATE) ** -k
        a_wpp += s_wpp * disc
        a_mod += s_mod * disc
        qx = q.get((b + x0 + k, x0 + k))
        if qx is None:
            return None
        s_wpp *= 1 - qx
        s_mod = F.model_survival(p, x0, k + 1)
        worst = max(worst, abs(s_mod - s_wpp))
    return a_mod / a_wpp - 1, worst


def start_ages(b):
    now = F.NOW - b
    return sorted({x for x in START_AGES if x >= now} | ({now} if 50 <= now <= 99 else set()))


def validate(params, data):
    rows = []
    for iso in F.COUNTRIES:
        for sex, sid in F.SEXES.items():
            d = data[(data.ISO3_code == iso) & (data.Sex == sex)]
            loc = int(d.LocID.iloc[0])
            q = {(int(r.Time), int(r.AgeGrpStart)): r.qx for r in d.itertuples()}
            for b in F.BIRTH_YEARS:
                p = params[str((loc * 2 + sid) * 10_000 + b)]
                for x0 in start_ages(b) + [x for x in PAID_AGES if x >= F.NOW - b]:
                    e = annuity_error(p, q, b, x0)
                    if e is not None:
                        rows.append({'iso3': iso, 'sex': sex.lower(), 'born': b, 'from': x0, 'annuity_error': e[0], 'survival_error': e[1]})
    return rows


def summarise(rows):
    later = [r for r in rows if r['from'] in PAID_AGES]
    rows = [r for r in rows if r['from'] not in PAID_AGES]
    paid = {}
    for x in PAID_AGES:
        e = sorted(abs(r['annuity_error']) for r in later if r['from'] == x)
        if e:
            paid[str(x)] = {'pairs': len(e), 'worst': e[-1], 'p95': e[int(0.95 * (len(e) - 1))], 'median': e[len(e) // 2]}
    by_age = collections.defaultdict(float)
    for r in rows:
        by_age[r['from'] if r['from'] in START_AGES else 'own age'] = max(by_age[r['from'] if r['from'] in START_AGES else 'own age'], abs(r['annuity_error']))
    worst = sorted(rows, key=lambda r: -abs(r['annuity_error']))[:10]
    errs = sorted(abs(r['annuity_error']) for r in rows)
    return {
        'pairs': len(rows),
        'worst_abs_annuity_error': errs[-1],
        'p95_abs_annuity_error': errs[int(0.95 * (len(errs) - 1))],
        'median_abs_annuity_error': errs[len(errs) // 2],
        'worst_by_start_age': {str(k): v for k, v in sorted(by_age.items(), key=lambda kv: str(kv[0]))},
        'worst_cases': worst,
        'from_later_ages': paid,
    }


def main():
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else F.REPO / 'config' / 'mortality.json'
    params = json.loads(path.read_text())['params']
    data = F.load(2000)
    s = summarise(validate(params, data))
    (RUNS / 'mortality-validation.json').write_text(json.dumps({'source': str(path.name), **s}, indent=1))
    print(f"{s['pairs']} cohort × start-age pairs: worst |Δä| {s['worst_abs_annuity_error']:.2%}, "
          f"95th percentile {s['p95_abs_annuity_error']:.2%}, median {s['median_abs_annuity_error']:.3%}")
    print('worst by start age:', {k: f'{v:.2%}' for k, v in s['worst_by_start_age'].items()})
    print('from later ages (income keeps being paid at 1/ä):', {k: f"worst {v['worst']:.2%}, p95 {v['p95']:.2%}, median {v['median']:.2%}" for k, v in s['from_later_ages'].items()})
    for r in s['worst_cases'][:5]:
        print(f"  {r['iso3']} {r['sex']} born {r['born']} from {r['from']}: {r['annuity_error']:+.2%}")


if __name__ == '__main__':
    main()
