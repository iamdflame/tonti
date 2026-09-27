#!/usr/bin/env python3
"""Independent float64 re-implementation of the quote (actuary-core/src/quote.rs), for a
differential test against the real engine.

It uses the same SplitMix64 stream, the same Acklam inverse-normal and the same 256-node return
table, so both see identical market paths. The only differences are float64 versus Q64.64 and a separate code path, so any
disagreement beyond rounding noise is a bug in one of them.

  python3 tests/reference_quote.py [n_cases]
"""
import json
import math
import random
import subprocess
import sys
from pathlib import Path

import sys as _sys
_sys.path.insert(0, str(__import__('pathlib').Path(__file__).resolve().parents[2] / 'actuarial'))
from paths import CLI  # noqa: E402
M64 = (1 << 64) - 1
A = [-3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02, 1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00]
B = [-5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02, 6.680131188771972e+01, -1.328068155288572e+01]
C = [-7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00, -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00]
D = [7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00, 3.754408661907416e+00]
P_LOW = 0.02425


class Rng:
    def __init__(self, seed):
        self.s = seed & M64

    def next(self):
        self.s = (self.s + 0x9E3779B97F4A7C15) & M64
        z = self.s
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M64
        return z ^ (z >> 31)

    def uniform(self):
        return (((self.next() >> 11) * 2 + 1) << 10) / 2 ** 64


def horner(coef, x):
    acc = 0.0
    for c in coef:
        acc = acc * x + c
    return acc


def inv_norm(u):
    if u < P_LOW:
        q = math.sqrt(-2 * math.log(u))
        return horner(C, q) / (horner(D, q) * q + 1)
    if u <= 1 - P_LOW:
        q = u - 0.5
        r = q * q
        return horner(A, r) * q / (horner(B, r) * r + 1)
    q = math.sqrt(-2 * math.log(1 - u))
    return -(horner(C, q) / (horner(D, q) * q + 1))


def survival(p, x, y, tau):
    a, b, th, k = p
    g = b * math.exp(th * x - k * (y - 2024))
    d = th - k
    return math.exp(-a * tau - g * math.expm1(d * tau) / d)


def spy_share(age):
    if age <= 40:
        return 0.8
    if age >= 65:
        return 0.3
    return 0.8 - 0.5 * (age - 40) / 25


K = 256  # return-table nodes (quote.rs RETURN_NODES)


def return_table(mu, sigma):
    z = [inv_norm((2 * i + 1) / (2 * K)) for i in range(K)]
    sd = math.sqrt(sum(x * x for x in z) / K)
    g = [math.exp(mu + sigma * x / sd) for x in z]
    scale = math.exp(mu + sigma * sigma / 2) / (sum(g) / K)
    return [x * scale for x in g]


def quote(p, age, year, start_age, lump, monthly, spy_ret, spy_vol, safe, rate, paths, seed, fee=0.003):
    mo = 1 / 12
    keep = 1 - fee / 12  # the pool's fee, charged monthly on the member's pot (not on money drawn alone)
    acc_months = int((start_age - age) * 12)
    start_year = year + (start_age - age)
    n = max(1, 120 * 12 - int(start_age * 12))
    surv, a_, y_ = [], start_age, start_year
    for _ in range(n):
        surv.append(survival(p, a_, y_, mo))
        a_ += mo
        y_ += mo
    v = 1 / (1 + rate) ** mo
    frac, nxt = [0.0] * n, 1.0
    for m in range(n - 1, -1, -1):
        a = 1 + v * surv[m] * nxt
        frac[m] = 1 / a
        nxt = a
    at85 = int((85 - start_age) * 12)
    sigma_m = spy_vol / math.sqrt(12)
    mu_m = (spy_ret - spy_vol ** 2 / 2) / 12
    safe_g = (1 + safe) ** mo
    table = return_table(mu_m, sigma_m)
    w, rest, a_ = [], [], age
    for _ in range(acc_months + n):
        s = spy_share(a_)
        w.append(s)
        rest.append((1 - s) * safe_g)
        a_ += mo
    acc_credit, a_, y_ = [], age, year
    for _ in range(acc_months):
        q = 1 - survival(p, a_, y_, mo)
        acc_credit.append((1 + q / (1 - q)) * keep)
        a_ += mo
        y_ += mo
    k = [(1 - frac[m]) / surv[m] * keep for m in range(n)]
    alive = [1.0]
    for m in range(n):
        alive.append(alive[-1] * surv[m])
    rng = Rng(seed)
    inc0s, inc85s, runouts, outlive = [], [], [], 0.0

    def gross(j):
        return w[j] * table[rng.next() >> 56] + rest[j]

    for _ in range(paths):
        j = 0
        pot = lump
        for credit in acc_credit:
            pot = (pot + monthly) * gross(j) * credit
            j += 1
        inc0 = pot * frac[0]
        pooled, solo, runout, inc85 = pot, pot, None, 0.0
        for m in range(n):
            if m == at85:
                inc85 = pooled * frac[m]
            g = gross(j)
            pooled = pooled * k[m] * g
            if runout is None:
                solo -= inc0
                if solo <= 0:
                    runout = m
                else:
                    solo *= g
            j += 1
        inc0s.append(inc0)
        if at85 >= 0:
            inc85s.append(inc85)
        ro = runout if runout is not None else n
        runouts.append(ro)
        if runout is not None:
            outlive += alive[ro]

    def pct(xs):
        xs = sorted(xs)
        return [xs[(len(xs) - 1) * k // 100] for k in (10, 50, 90)]

    return {'income_start': pct(inc0s), 'income_at_85': pct(inc85s) if inc85s else [0, 0, 0],
            'solo_runout_age_p50': start_age + pct(runouts)[1] / 12, 'solo_outlive_probability': outlive / paths}


def main(cases):
    rng = random.Random(20260926)
    worst, mismatches = 0.0, 0
    for i in range(cases):
        p = (rng.uniform(1e-4, 1e-3), rng.uniform(1e-5, 8e-5), rng.uniform(0.085, 0.115), rng.uniform(0.0, 0.025))
        age = rng.uniform(25, 70)
        start = age + rng.uniform(0, 25) if age < 65 else age
        start = min(start, 95)
        # The market, the valuation rate, the fee and the path count vary too (skeptic: a fixed
        # market proves only precision on one scenario).
        safe = rng.uniform(0.0, 0.06)
        args = [*p, age, 2026.75, start, rng.choice([0, 1000, 20000]), rng.choice([0, 25, 50, 200]),
                rng.uniform(-0.02, 0.12), rng.uniform(0.05, 0.35), safe, rng.uniform(0.0, 0.06),
                rng.choice([16, 64, 128]), rng.randrange(1, 2 ** 63)]
        fee = rng.choice([0.0, 0.003, 0.01])
        if args[7] == 0 and args[8] == 0:
            args[7] = 5000
        got = json.loads(subprocess.run([CLI, 'quote', *map(str, args), str(fee)], capture_output=True, text=True, check=True).stdout)
        want = quote(tuple(args[:4]), *args[4:], fee=fee)
        for k in ('income_start', 'income_at_85'):
            for g, w in zip(got[k], want[k]):
                if w > 1e-9:
                    worst = max(worst, abs(g / w - 1))
        worst_ro = abs(got['solo_runout_age_p50'] - want['solo_runout_age_p50'])
        worst_p = abs(got['solo_outlive_probability'] - want['solo_outlive_probability'])
        if worst_ro > 1e-9 or worst_p > 1e-9:
            mismatches += 1
            print(f'case {i}: runout {got["solo_runout_age_p50"]} vs {want["solo_runout_age_p50"]}, '
                  f'outlive {got["solo_outlive_probability"]} vs {want["solo_outlive_probability"]}')
    print(f'{cases} random quotes (random markets, rates, fees, paths): worst relative income difference '
          f'engine vs float reference = {worst:.2e}; run-out mismatches: {mismatches}')
    sys.exit(0 if worst < 1e-9 and mismatches == 0 else 1)


if __name__ == '__main__':
    main(int(sys.argv[1]) if len(sys.argv) > 1 else 40)
