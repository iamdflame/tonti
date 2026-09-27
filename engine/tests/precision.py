#!/usr/bin/env python3
"""Measure actuary-core's fixed-point error against mpmath at 50 digits.

Runs the real engine through `actuary-cli fx` on random inputs across each function's working
domain and reports the worst absolute and relative error. Exits non-zero if a bound is broken.

  python3 tests/precision.py [n_per_op]
"""
import json
import random
import subprocess
import sys
from pathlib import Path

import mpmath as mp

mp.mp.dps = 50
ONE = 2 ** 64
import sys as _sys
_sys.path.insert(0, str(__import__('pathlib').Path(__file__).resolve().parents[2] / 'actuarial'))
from paths import CLI as _CLI, RUNS  # noqa: E402
CLI = Path(_CLI)

# (op, input generator, reference, abs bound, rel bound for |want| > 1e-6)
def uniform(lo, hi):
    return lambda r: [int(mp.nint(mp.mpf(r.uniform(lo, hi)) * ONE))]

def log_uniform(lo_exp, hi_exp):
    return lambda r: [int(mp.nint(mp.power(10, r.uniform(lo_exp, hi_exp)) * ONE))]

CASES = [
    ('exp', uniform(-44, 43.6), lambda a: mp.exp(a), 1e-18, 2e-18),
    ('ln', log_uniform(-15, 18), lambda a: mp.log(a), 2e-18, None),
    ('sqrt', log_uniform(-12, 18), lambda a: mp.sqrt(a), 1e-18, 5e-18),
    ('pow', lambda r: [int(mp.nint(mp.mpf(r.uniform(0.5, 2.0)) * ONE)), int(mp.nint(mp.mpf(r.uniform(-3, 3)) * ONE))],
     lambda a, b: mp.power(a, b), 1e-17, 1e-17),
]


def main(n):
    rng = random.Random(20260926)
    lines, meta = [], []
    for op, gen, ref, abs_b, rel_b in CASES:
        for _ in range(n):
            raws = gen(rng)
            lines.append(' '.join([op, *map(str, raws)]))
            meta.append((op, raws, ref, abs_b, rel_b))
    out = subprocess.run([str(CLI), 'fx'], input='\n'.join(lines), capture_output=True, text=True, check=True).stdout.split('\n')
    report, failed = {}, False
    for (op, raws, ref, abs_b, rel_b), got in zip(meta, out):
        want = ref(*[mp.mpf(x) / ONE for x in raws])
        r = report.setdefault(op, {'n': 0, 'errors': 0, 'max_abs_below_1': 0.0, 'max_rel_from_1': 0.0})
        r['n'] += 1
        if got.startswith('err'):
            r['errors'] += 1
            continue
        err = abs(mp.mpf(int(got)) / ONE - want)
        # Q64.64 resolution is absolute (2^-64), so outputs below 1 are judged by absolute error
        # and outputs of 1 or more by relative error.
        if abs(want) < 1:
            r['max_abs_below_1'] = max(r['max_abs_below_1'], float(err))
            failed |= err > abs_b
        else:
            r['max_rel_from_1'] = max(r['max_rel_from_1'], float(err / abs(want)))
            failed |= err / abs(want) > (rel_b if rel_b is not None else abs_b)
    for op, r in report.items():
        print(f"{op:5} n={r['n']:<6} errors={r['errors']:<3} max_abs(|y|<1)={r['max_abs_below_1']:.2e}  max_rel(|y|>=1)={r['max_rel_from_1']:.2e}")
    path = RUNS / 'precision.json'
    path.write_text(json.dumps(report, indent=1))
    print(f'report -> {path}')
    print('PASS' if not failed else 'FAIL: a bound was exceeded')
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main(int(sys.argv[1]) if len(sys.argv) > 1 else 5000)
