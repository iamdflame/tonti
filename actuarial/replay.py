#!/usr/bin/env python3
"""Historical replay: what the pool would have paid real cohorts through real history.

Mortality: each cohort's actual historical death rates, read along its diagonal of the UN WPP 2024
estimates (1950–2023). Returns: Fama-French (CRSP) monthly market total return and 1-month T-bill.
Inflation: Shiller's monthly CPI-U. The pool's money flows run through the production ledger via
`actuary-cli replay`, which also runs three solo benchmarks on the same pot, mix and returns.

Payouts are priced two ways:
  period  — from that calendar year's period life table at 3.5%, what an actuary would have had then
            (no hindsight). Mortality kept improving, so this under-prices longevity.
  cohort  — from the cohort's realised death rates (estimates to 2023, UN projections after): the
            price with perfect mortality foresight. The gap between the two is the cost of longevity
            forecast error, which members bear.

  python3 actuarial/replay.py
"""
import json
import subprocess
import zipfile
from pathlib import Path

import pandas as pd

from paths import CLI, DATA, RUNS
RATE = 0.035
MEMBERS, POT, SEED = 2000, 10_000, 7
RETIREMENTS = (1965, 1973, 2000)
# Escalating plan (docs/protocol.md §4.3): priced 2 points lower, so income starts lower and, when
# returns match the level plan's assumption, rises about 2% a year — CPF LIFE's Escalating Plan.
ESCALATING_RATE = 0.015


def returns():
    z = zipfile.ZipFile(DATA / 'markets' / 'FF3.zip')
    rows = {}
    for line in z.read(z.namelist()[0]).decode('latin-1').splitlines():
        parts = [p.strip() for p in line.split(',')]
        if len(parts) == 5 and len(parts[0]) == 6 and parts[0].isdigit():
            mkt_rf, rf = float(parts[1]) / 100, float(parts[4]) / 100
            rows[int(parts[0])] = (mkt_rf + rf, rf)
        elif rows and not (parts[0][:6].isdigit() and len(parts[0]) == 6):
            break  # the monthly block ends where the annual block begins
    return rows


def inflation():
    """Monthly CPI change keyed yyyymm. Shiller writes dates as floats: 1965.1 is October."""
    x = pd.read_excel(DATA / 'markets' / 'ie_data.xls', sheet_name='Data', header=7).dropna(subset=['Date', 'CPI'])
    cpi = {}
    for d, c in zip(x['Date'], x['CPI']):
        try:
            d = float(d)
        except (TypeError, ValueError):
            continue
        cpi[int(d) * 100 + round((d - int(d)) * 100)] = float(c)
    keys = sorted(cpi)
    return {k: cpi[k] / cpi[p] - 1 for p, k in zip(keys, keys[1:])}


def life_table(iso, sex):
    q = {}
    for span in ('1950-2023', '2024-2100'):
        f = DATA / 'wpp2024' / f'WPP2024_Life_Table_Complete_Medium_{sex}_{span}.csv.gz'
        t = pd.read_csv(f, usecols=['ISO3_code', 'Time', 'AgeGrpStart', 'qx'], dtype={'ISO3_code': str})
        t = t[t.ISO3_code == iso]
        q.update({(int(r.Time), int(r.AgeGrpStart)): r.qx for r in t.itertuples()})
    return q


def annuity(q_at, age, rate=RATE):
    """ä from `age` in monthly steps, q_at(k, age) giving the annual q for step k. The UN tables end
    in an open 100+ group, so past 100 its rate applies until survival is negligible."""
    v = (1 + rate) ** (-1 / 12)
    s, a, k = 1.0, 0.0, 0
    while s > 1e-9 and k < 1200:
        a += s * v ** k
        s *= (1 - q_at(k, age)) ** (1 / 12)
        age += 1 / 12
        k += 1
    return a


def build(q, rets, cpi, born, retire_year, pricing):
    lines = []
    for y in range(retire_year, 2024):
        for m in range(12):
            ym = y * 100 + m + 1
            age = y - born
            if ym not in rets or ym not in cpi or (y, min(age, 100)) not in q:
                return lines
            q_month = 1 - (1 - q[(y, min(age, 100))]) ** (1 / 12)
            if pricing in ('period', 'escalating'):  # this year's table, frozen
                ann = annuity(lambda k, a: q.get((y, min(int(a), 100)), 1.0), age + m / 12,
                              ESCALATING_RATE if pricing == 'escalating' else RATE)
            else:  # the cohort's own realised/projected rates, year by year
                ann = annuity(lambda k, a: q.get((born + int(a), min(int(a), 100)), q.get((2100, min(int(a), 100)), 1.0)), age + m / 12)
            spy, tb = rets[ym]
            lines.append(f'{spy} {tb} {q_month} {1 / ann} {cpi[ym]}')
    return lines


def summarise(r, retire, months):
    cols = r['columns']
    years = [dict(zip(cols, y)) for y in r['years']]
    alive = lambda month: years[month // 12]['alive'] / MEMBERS if month // 12 < len(years) else 0.0
    real = [y['income_real'] for y in years]
    ratio = [y['income_real'] / y['self_income_real'] for y in years]
    out = {
        'months': months,
        'first_month_income': r['first_month_income'],
        'income_nominal': [y['income'] for y in years],
        'income_real': real,
        'pool_over_self_annuitised': ratio,
        'alive': [y['alive'] for y in years],
        'min_real_income': min(real) if real else None,
        # Below ~100 survivors a one-cohort pool's credits get lumpy (the production pool shares
        # credits across every cohort, which this replay doesn't model), so report both.
        'min_real_income_100_alive': min(v for v, y in zip(real, years) if y['alive'] >= 100),
        'shadow_runout_year': retire + r['shadow_runout_month'] // 12 if r['shadow_runout_month'] is not None else None,
        'shadow_alive_share': alive(r['shadow_runout_month']) if r['shadow_runout_month'] is not None else None,
        'rule4_runout_year': retire + r['rule4_runout_month'] // 12 if r['rule4_runout_month'] is not None else None,
        'rule4_alive_share': alive(r['rule4_runout_month']) if r['rule4_runout_month'] is not None else None,
    }
    return out


def main():
    rets, cpi = returns(), inflation()
    q = life_table('PHL', 'Female')
    report = {'setup': {'country': 'PHL', 'sex': 'female', 'members': MEMBERS, 'pot': POT, 'seed': SEED,
                        'valuation_rate': RATE, 'fee_bps_year': 30, 'mix': '30% SPY / 49% T-bill / 21% cash',
                        'trading_cost_bps': {'SGOV': 4.75, 'SPY': 6.25}}}
    for retire in RETIREMENTS:
        born = retire - 65
        report[retire] = {}
        for pricing in ('period', 'cohort', 'escalating'):
            lines = build(q, rets, cpi, born, retire, pricing)
            out = subprocess.run([CLI, 'replay', str(MEMBERS), str(POT), str(SEED)], input='\n'.join(lines),
                                 capture_output=True, text=True, check=True).stdout
            report[retire][pricing] = summarise(json.loads(out), retire, len(lines))
        # The same history under 20 other draws of who dies when (level plan, period pricing).
        lines = build(q, rets, cpi, born, retire, 'period')
        runs = [summarise(json.loads(subprocess.run([CLI, 'replay', str(MEMBERS), str(POT), str(seed)], input='\n'.join(lines),
                                                    capture_output=True, text=True, check=True).stdout), retire, len(lines))
                for seed in range(100, 120)]
        report[retire]['seeds'] = {
            'n': len(runs),
            'shadow_alive_share': [r['shadow_alive_share'] for r in runs],
            'min_real_income': [r['min_real_income'] for r in runs],
            'min_real_income_100_alive': [r['min_real_income_100_alive'] for r in runs],
            'income_real_yr10': [r['income_real'][9] for r in runs],
        }
        p, c, e = report[retire]['period'], report[retire]['cohort'], report[retire]['escalating']
        pick = lambda xs, i: f'${xs[i]:,.0f}' if i < len(xs) else '—'
        x = lambda xs, i: f'{xs[i]:.2f}×' if i < len(xs) else '—'
        print(f"\nRetired {retire} at 65 (born {born}): {MEMBERS:,} Philippine women, ${POT:,} each, {p['months'] // 12} years of data")
        for name, s in (('level, period table        ', p), ('level, cohort (hindsight)  ', c), ('escalating, period table   ', e)):
            print(f"  {name}: income/yr in {retire} $  yr1 {pick(s['income_real'], 0)}  yr10 {pick(s['income_real'], 9)}"
                  f"  yr20 {pick(s['income_real'], 19)}  yr30 {pick(s['income_real'], 29)}   lowest {s['min_real_income']:,.0f}")
        print(f"  vs the same rule without pooling: yr10 {x(p['pool_over_self_annuitised'], 9)}  yr20 {x(p['pool_over_self_annuitised'], 19)}"
              f"  yr30 {x(p['pool_over_self_annuitised'], 29)}")
        if p['shadow_runout_year']:
            print(f"  the pool's exact income stream, drawn alone, ran out in {p['shadow_runout_year']} with {p['shadow_alive_share']:.0%} still alive"
                  f" (escalating: {e['shadow_runout_year'] or 'not by 2023'}, {e['shadow_alive_share'] or 0:.0%} alive)")
        else:
            print(f"  the pool's exact income stream, drawn alone, had not run out by 2023")
        sd = report[retire]['seeds']
        rng = lambda xs, f: f'{f(min(xs))}–{f(max(xs))}'
        print(f"  over {sd['n']} other death draws: alive when the pool's income ran out alone {rng(sd['shadow_alive_share'], lambda v: f'{v:.0%}')},"
              f" lowest real income ${rng(sd['min_real_income'], lambda v: f'{v:,.0f}')} (with 100+ alive ${rng(sd['min_real_income_100_alive'], lambda v: f'{v:,.0f}')}), yr10 ${rng(sd['income_real_yr10'], lambda v: f'{v:,.0f}')}")
        r4 = f"ran out in {p['rule4_runout_year']} with {p['rule4_alive_share']:.1%} still alive" if p['rule4_runout_year'] else 'lasted'
        print(f"  4% rule: ${POT * 0.04:,.0f}/yr real, {r4}")
    (RUNS / 'replay.json').write_text(json.dumps(report, indent=1))
    print(f"\nreport -> {RUNS / 'replay.json'}")


if __name__ == '__main__':
    main()
