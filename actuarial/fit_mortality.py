#!/usr/bin/env python3
"""Fit Tonti's mortality model to UN WPP 2024 life tables and measure the fit where it matters.

Model (docs/protocol.md §2): one Gompertz–Makeham curve per (country, sex, birth year), fitted
along that cohort's own diagonal of the WPP tables (estimates to 2023, medium projections after):

    μ_b(x) = A + B·exp(θ·x)        (κ = 0: the cohort's calendar-time improvement is in A, B, θ)

A single improvement rate shared by all ages (the first version) put annuity factors off by up to
9.8% (India, Bangladesh), because real improvement differs sharply by age. Fitting each birth
cohort along its own lifetime path removes that error at the source.

The fit targets what sets payouts. It starts from least squares on ln m_x over ages max(30, age in
2026)–99 on the cohort diagonal, then refines A, B, θ to match WPP's annuity factor ä (3.5%) from
every age income can start (50, 55, …, 80, and the cohort's own age if older), with the ln m_x
residuals kept as a light regulariser (weight 0.05) so the saving-phase death rates stay sensible.
A plain ln m_x fit weights ages 30–49 as much as 70–99; measured from the ages income starts, it
was off by up to 3.1% (skeptic review, 2026-09-27). The refined fit: see actuarial/validate_fit.py.

Validation is on what drives payouts, not on the fit residuals:
  * cohort survival: for every birth year the pool supports (1935–2005), from its age in 2026 to
    100, model S versus WPP S read along the cohort diagonal, as the maximum absolute error;
  * the annuity factor ä at 3.5%, which sets income, as the maximum relative error, from each age
    income can start (actuarial/validate_fit.py writes the full report).

Outputs:
  config/mortality.json          parameters (float and raw Q64.64), keyed by ISO numeric×2+sex
  <runs>/mortality-fit.json      the validation report

  python3 actuarial/fit_mortality.py [--years-from 2000]
"""
import argparse
import json
import math
from pathlib import Path

import numpy as np
import pandas as pd
from scipy.optimize import least_squares

from paths import DATA as _DATA, RUNS
DATA = _DATA / 'wpp2024'
REPO = Path(__file__).resolve().parent.parent
# Origin countries of Singapore's foreign workforce, plus Singapore.
COUNTRIES = ['PHL', 'IDN', 'IND', 'BGD', 'MMR', 'MYS', 'CHN', 'THA', 'LKA', 'VNM', 'NPL', 'SGP']
SEXES = {'Female': 0, 'Male': 1}
AGE_MIN, AGE_MAX = 30, 99
BIRTH_YEARS = range(1935, 2006)
NOW = 2026
RATE = 0.035
Q64 = 2 ** 64


def load(years_from):
    frames = []
    for sex in SEXES:
        for span in ('1950-2023', '2024-2100'):
            f = DATA / f'WPP2024_Life_Table_Complete_Medium_{sex}_{span}.csv.gz'
            if not f.exists():
                raise SystemExit(f'missing {f}; run actuarial/fetch_wpp.sh')
            for chunk in pd.read_csv(f, usecols=['ISO3_code', 'LocID', 'Time', 'Sex', 'AgeGrpStart', 'mx', 'qx'], chunksize=500_000, dtype={'ISO3_code': str}):
                c = chunk[chunk.ISO3_code.isin(COUNTRIES) & (chunk.Time >= 1990) & (chunk.AgeGrpStart >= 20)]
                frames.append(c)
    return pd.concat(frames, ignore_index=True)


def fit_one(m, b):
    """m: {(year, age): mx}. Least squares on ln m_x along the cohort born in `b`'s diagonal."""
    x0 = max(AGE_MIN, NOW - b)
    ages = [x for x in range(x0, AGE_MAX + 1) if (b + x, x) in m and m[(b + x, x)] > 0]
    x = np.array(ages, float) + 0.5
    lm = np.log(np.array([m[(b + a, a)] for a in ages]))

    def resid(p):
        la, lb, th = p
        return np.log(np.exp(la) + np.exp(lb + th * x)) - lm

    r = least_squares(resid, [math.log(3e-4), math.log(3e-5), 0.1], method='trf', x_scale='jac', max_nfev=20000)
    la, lb, th = r.x
    return {'A': math.exp(la), 'B': math.exp(lb), 'theta': th, 'kappa': 0.0, 'rms_log_resid': float(np.sqrt(np.mean(r.fun ** 2)))}


START_AGES = (50, 55, 60, 65, 70, 75, 80)
# Income keeps being paid at 1/ä from every later age, so those annuity factors are targets too, at
# a lower weight (skeptic review 2: fitted from 50-80 only, the error reached 7% from 95).
PAID_AGES = (85, 90, 95)
LATE_WEIGHT = 0.5
REGULARISER = 0.05


def start_ages(b):
    now = NOW - b
    return sorted({x for x in START_AGES if x >= now} | ({now} if 50 <= now <= 99 else set()))


def annuity(surv_at, x0):
    a, s = 0.0, 1.0
    for k in range(0, 100 - x0):
        a += s * (1 + RATE) ** -k
        s = surv_at(k + 1)
        if s is None:
            return None
    return a


def wpp_annuity(q, b, x0):
    s = [1.0]
    for k in range(0, 100 - x0):
        qx = q.get((b + x0 + k, x0 + k))
        if qx is None:
            return None
        s.append(s[-1] * (1 - qx))
    return annuity(lambda k: s[k], x0)


def fit_payouts(m, q, b):
    """Refines the ln m_x fit so the annuity factors from every age income can start match WPP's."""
    base = fit_one(m, b)
    targets = [(x0, t, 1.0) for x0 in start_ages(b) for t in [wpp_annuity(q, b, x0)] if t]
    targets += [(x0, t, LATE_WEIGHT) for x0 in PAID_AGES if x0 >= NOW - b for t in [wpp_annuity(q, b, x0)] if t]
    if not targets:
        return base  # born too late for WPP (to 2100) to reach 100 from 50: the ln m_x fit stands
    xa = max(AGE_MIN, NOW - b)
    ages = [x for x in range(xa, AGE_MAX + 1) if (b + x, x) in m and m[(b + x, x)] > 0]
    x = np.array(ages, float) + 0.5
    lm = np.log(np.array([m[(b + a, a)] for a in ages]))
    lo, hi = [math.log(1e-8), math.log(1e-9), 0.03], [math.log(0.05), math.log(1e-2), 0.2]
    clip = lambda v, i: min(max(v, lo[i] + 1e-6), hi[i] - 1e-6)
    start = [clip(math.log(max(base['A'], 1e-8)), 0), clip(math.log(base['B']), 1), clip(base['theta'], 2)]

    def resid(pv):
        la, lb, th = pv
        p = {'A': math.exp(la), 'B': math.exp(lb), 'theta': th}
        r1 = [w * (annuity(lambda k: model_survival(p, x0, k), x0) / t - 1) for x0, t, w in targets]
        r2 = REGULARISER * (np.log(np.exp(la) + np.exp(lb + th * x)) - lm) / math.sqrt(len(ages))
        return np.concatenate([np.array(r1), r2])

    r = least_squares(resid, start, bounds=(lo, hi), method='trf', x_scale='jac', max_nfev=4000)
    la, lb, th = r.x
    rms = float(np.sqrt(np.mean((np.log(np.exp(la) + np.exp(lb + th * x)) - lm) ** 2)))
    return {'A': math.exp(la), 'B': math.exp(lb), 'theta': th, 'kappa': 0.0, 'rms_log_resid': rms}


def model_survival(p, x, tau):
    g = p['B'] * math.exp(p['theta'] * x)
    return math.exp(-p['A'] * tau - g * math.expm1(p['theta'] * tau) / p['theta'])


def validate(p, q, b):
    """|S_model − S_wpp| along the cohort's diagonal from its age in 2026 to 100, and ä error."""
    x0 = max(NOW - b, AGE_MIN)
    s_wpp = s_mod = 1.0
    a_wpp = a_mod = 0.0
    worst = 0.0
    for k in range(0, 100 - x0):
        disc = (1 + RATE) ** -k
        a_wpp += s_wpp * disc
        a_mod += s_mod * disc
        qx = q.get((b + x0 + k, x0 + k))
        if qx is None:
            break
        s_wpp *= 1 - qx
        s_mod = model_survival(p, x0, k + 1)
        worst = max(worst, abs(s_mod - s_wpp))
    return worst, abs(a_mod / a_wpp - 1)


def main():
    global LATE_WEIGHT
    ap = argparse.ArgumentParser()
    ap.add_argument('--years-from', type=int, default=2000)
    ap.add_argument('--out', type=Path, default=REPO / 'config' / 'mortality.json')
    ap.add_argument('--late-weight', type=float, default=LATE_WEIGHT)
    args = ap.parse_args()
    LATE_WEIGHT = args.late_weight
    data = load(args.years_from)
    params, report = {}, {}
    for iso in COUNTRIES:
        for sex, sex_id in SEXES.items():
            d = data[(data.ISO3_code == iso) & (data.Sex == sex)]
            if d.empty:
                continue
            loc = int(d.LocID.iloc[0])
            m = {(int(r.Time), int(r.AgeGrpStart)): r.mx for r in d.itertuples()}
            q = {(int(r.Time), int(r.AgeGrpStart)): r.qx for r in d.itertuples()}
            worst_s = worst_a = worst_rms = 0.0
            for b in BIRTH_YEARS:
                p = fit_payouts(m, q, b)
                ds, da = validate(p, q, b)
                worst_s, worst_a, worst_rms = max(worst_s, ds), max(worst_a, da), max(worst_rms, p['rms_log_resid'])
                key = (loc * 2 + sex_id) * 10_000 + b
                params[str(key)] = {
                    'iso3': iso, 'isoNumeric': loc, 'sex': sex.lower(), 'birthYear': b,
                    **{k: p[k] for k in ('A', 'B', 'theta', 'kappa')},
                    'q64': {k: str(int(round(p[k] * Q64))) for k in ('A', 'B', 'theta', 'kappa')},
                    'maxSurvivalError': ds, 'annuityError': da,
                }
            report[f'{iso}-{sex}'] = {'cohorts': len(BIRTH_YEARS), 'max_abs_survival_error': worst_s,
                                      'max_rel_annuity_error': worst_a, 'max_rms_log_resid': worst_rms}
            print(f"{iso} {sex:6} cohorts {BIRTH_YEARS.start}-{BIRTH_YEARS.stop - 1}:  worst max|ΔS|={worst_s:.4f}  "
                  f"worst |Δä|={worst_a:.2%}  worst rms(ln m)={worst_rms:.3f}")
    out = args.out
    out.write_text(json.dumps({'source': 'UN WPP 2024 (CC BY 3.0 IGO), medium variant, single-age life tables',
                               'model': 'per (country, sex, birthYear) cohort: mu(x) = A + B*exp(theta*x), kappa = 0',
                               'key': '(isoNumeric*2 + sex)*10000 + birthYear; sex 0 = female, 1 = male',
                               'fitYearsFrom': args.years_from, 'ages': [AGE_MIN, AGE_MAX], 'params': params}, indent=1) + '\n')
    (RUNS / 'mortality-fit.json').write_text(json.dumps(report, indent=1))
    print(f'params -> {out}\nreport -> {RUNS / "mortality-fit.json"}')


if __name__ == '__main__':
    main()
