import assert from 'node:assert/strict';
import { test } from 'node:test';
import { WAD, ageAt, cohortKey, fromUsdg, toUsdg, toWad, yearAt } from '../src/index.ts';

test('cohort keys match the Actuary and pool: (isoNumeric·2 + sex)·10000 + birthYear', () => {
  assert.equal(cohortKey('PHL', 'female', 1966), 12161966n);
  assert.equal(cohortKey('PHL', 'male', 1990), 12171990n);
  assert.equal(cohortKey('SGP', 'female', 2005), 14042005n);
  assert.throws(() => cohortKey('PHL', 'female', 1934), /1935–2005/);
  // @ts-expect-error: not a fitted country
  assert.throws(() => cohortKey('FRA', 'female', 1970), /no mortality/);
});

test('WAD and USDG conversions are exact where the contracts need them to be', () => {
  assert.equal(toWad(62), 62n * WAD);
  assert.equal(toWad(0.035), 35n * 10n ** 15n);
  assert.equal(toUsdg(3000), 3_000_000_000n);
  assert.equal(toUsdg(0.01), 10_000n);
  assert.equal(fromUsdg(25_000_000n), 25);
  assert.throws(() => toUsdg(-1));
});

test("ages use the pool's clock: 1970 + seconds/Julian year, born mid-year", () => {
  const t0 = new Date(1_790_460_000 * 1000); // the pool tests' 2026-09-26
  assert.ok(Math.abs(yearAt(t0) - (1970 + 1_790_460_000 / 31_557_600)) < 1e-9);
  assert.ok(Math.abs(ageAt(1966, t0) - (yearAt(t0) - 1966.5)) < 1e-12);
});
