//! Cohort ledger and monthly settlement (docs/protocol.md §3–§5).
//!
//! All asset movement is integer share accounting, so conservation is exact: every share that
//! leaves a cohort lands in another cohort, the fee bucket, a bequest claim, a sale order, or
//! the carry. Leftover rounding dust carries into the next release, where it becomes credits
//! for survivors, so no member ever gains from rounding.

use alloc::vec::Vec;

use crate::fixed::{Fx, MathError, MathResult};
use crate::wide::{div_wide, mul_div, mul_wide};

/// Asset sleeves: USDG in Morpho steakUSDG, SGOV, SPY. Share amounts are 18-decimal integers.
pub const SLEEVES: usize = 3;
pub type Shares = [u128; SLEEVES];
const WAD: u128 = 1_000_000_000_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// At risk: earns mortality credits, released to survivors at death.
    Tontine,
    /// Protected: earns no credits, goes to beneficiaries at death.
    Bequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cohort {
    pub class: Class,
    pub shares: Shares,
    /// Units held by living members.
    pub units: u128,
    /// Cumulative USDG paid per whole (1e18) unit. A member claims
    /// `units·(income_per_unit − last)/1e18` since their last claim.
    pub income_per_unit: Fx,
}

/// Per-cohort inputs for one month, from the mortality model.
#[derive(Clone, Copy, Debug)]
pub struct CohortMonth {
    /// One-month death probability.
    pub q: Fx,
    /// Fraction of value paid out this month (zero while accumulating).
    pub payout_fraction: Fx,
}

/// A member whose death became final this epoch: `units` of cohort `cohort` are released.
#[derive(Clone, Copy, Debug)]
pub struct Release {
    pub cohort: usize,
    pub units: u128,
}

/// Shares a paying cohort sells for this month's income.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sale {
    pub cohort: usize,
    pub shares: Shares,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settlement {
    pub fees: Shares,
    pub bequest_claims: Shares,
    /// Bequest shares released by each entry of `releases`, in order (zero for tontine
    /// releases), so the pool can pay each beneficiary exactly what their member left.
    pub bequest_by_release: Vec<Shares>,
    /// Released tontine shares handed to survivors, per sleeve.
    pub credited: Shares,
    /// Dust carried into next month's release.
    pub carry: Shares,
    pub sales: Vec<Sale>,
    /// Sum of `sales`, per sleeve: what the treasury must sell.
    pub to_sell: Shares,
}

pub struct Epoch<'a> {
    pub cohorts: &'a mut [Cohort],
    pub months: &'a [CohortMonth],
    /// USDG per whole share of each sleeve (Chainlink price, or the vault's share price).
    pub prices: [Fx; SLEEVES],
    /// Monthly fee fraction (0.30%/yr ⇒ 0.003/12).
    pub fee_fraction: Fx,
}

fn share_of(amount: u128, part: u128, whole: u128) -> MathResult<u128> {
    if whole == 0 {
        return Ok(0);
    }
    mul_div(amount, part, whole).ok_or(MathError::Overflow)
}

fn fraction_of(amount: u128, f: Fx) -> MathResult<u128> {
    if f.is_negative() || f > Fx::ONE {
        return Err(MathError::Domain);
    }
    mul_div(amount, f.0 as u128, Fx::ONE.0 as u128).ok_or(MathError::Overflow)
}

/// USDG value of 18-decimal `shares` at `price` per whole share.
fn price_times(shares: u128, price: Fx) -> MathResult<Fx> {
    if price.is_negative() {
        return Err(MathError::Domain);
    }
    let raw = mul_div(shares, price.0 as u128, WAD).ok_or(MathError::Overflow)?;
    i128::try_from(raw).map(Fx).map_err(|_| MathError::Overflow)
}

pub fn value(shares: &Shares, prices: &[Fx; SLEEVES]) -> MathResult<Fx> {
    let mut v = Fx::ZERO;
    for a in 0..SLEEVES {
        v = v.add(price_times(shares[a], prices[a])?)?;
    }
    Ok(v)
}

/// Σw² over the epoch's credit weights, in 256 bits (a weight can exceed 2⁶⁴).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SumSq {
    pub hi: u128,
    pub lo: u128,
}

impl SumSq {
    pub fn add(&mut self, w: u128) -> MathResult<()> {
        let (h, l) = mul_wide(w, w);
        let (lo, carry) = self.lo.overflowing_add(l);
        self.lo = lo;
        self.hi = self.hi.checked_add(h).and_then(|x| x.checked_add(carry as u128)).ok_or(MathError::Overflow)?;
        Ok(())
    }

    /// Σs² = Σw²/W², the weights' concentration, as Q64.64, rounded up: that is what guarantees
    /// the corrected weights never sum past W, so credits never exceed what was released.
    pub fn share(&self, total: u128) -> MathResult<Fx> {
        if total == 0 {
            return Ok(Fx::ZERO);
        }
        // ceil(Q/W) = floor((Q + W − 1)/W); Q < W² keeps the quotient within 128 bits.
        let (lo, carry) = self.lo.overflowing_add(total - 1);
        let hi = self.hi.checked_add(carry as u128).ok_or(MathError::Overflow)?;
        let per = div_wide(hi, lo, total).ok_or(MathError::Overflow)?;
        // ceil(per·2⁶⁴/W).
        let (lo, carry) = (per << 64).overflowing_add(total - 1);
        let s = div_wide((per >> 64) + carry as u128, lo, total).ok_or(MathError::Overflow)?;
        Ok(Fx(s.min(Fx::ONE.0 as u128) as i128))
    }
}

/// Settlement one cohort at a time (docs/protocol.md §5.1). `Epoch::settle` and `book_income`
/// run these steps over every cohort in one call. The pool runs them in pages, so no transaction's
/// gas grows with the number of cohorts, deposits or deaths. Each step reads other cohorts only
/// through the running totals it's handed, so both orders give the same result to the last share.
pub mod step {
    use super::*;

    /// Charges one cohort the epoch's fee.
    pub fn fee(c: &mut Cohort, fee_fraction: Fx, fees: &mut Shares) -> MathResult<()> {
        for a in 0..SLEEVES {
            let f = fraction_of(c.shares[a], fee_fraction)?;
            c.shares[a] -= f;
            fees[a] += f;
        }
        Ok(())
    }

    /// Releases a dead member's `units` of `c`. Tontine shares join the credit `pool`; bequest
    /// shares are added to `bequest_claims` and returned, so the beneficiary can be paid exactly
    /// what the member left.
    pub fn release(c: &mut Cohort, units: u128, pool: &mut Shares, bequest_claims: &mut Shares) -> MathResult<Shares> {
        if units > c.units {
            return Err(MathError::Domain);
        }
        let mut left = [0u128; SLEEVES];
        for a in 0..SLEEVES {
            let s = share_of(c.shares[a], units, c.units)?;
            c.shares[a] -= s;
            match c.class {
                Class::Tontine => pool[a] += s,
                Class::Bequest => {
                    bequest_claims[a] += s;
                    left[a] = s;
                }
            }
        }
        c.units -= units;
        Ok(left)
    }

    /// Credit weight h·value with h = q/(1−q); zero for bequest and empty cohorts.
    pub fn weight(c: &Cohort, q: Fx, prices: &[Fx; SLEEVES]) -> MathResult<u128> {
        if c.class != Class::Tontine || c.units == 0 {
            return Ok(0);
        }
        if q.is_negative() || q >= Fx::ONE {
            return Err(MathError::Domain);
        }
        let odds = q.div(Fx::ONE.sub(q)?)?;
        Ok(odds.mul(value(&c.shares, prices)?)?.0 as u128)
    }

    /// The finite-pool correction (docs/protocol.md §3): y = w·(1 + s − Σs²) with s = w/W.
    ///
    /// With plain `h·value` weights, a surviving member's expected credit falls short by about
    /// their own weight share s (the dead leave the denominator; a big member dilutes their own
    /// credit) and gains Σs² from the variance of deaths. That bias is O(1/N): 2.5% RMS at 100
    /// members. y cancels it to first order, leaving about −s²; Σy = W, so it moves credit
    /// between members without creating any. Rounded so that Σy ≤ W.
    pub fn corrected_weight(w: u128, total: u128, sum_sq: Fx) -> MathResult<u128> {
        if total == 0 || w == 0 {
            return Ok(0);
        }
        let up = mul_div(w, w, total).ok_or(MathError::Overflow)?;
        let (hi, lo) = mul_wide(w, sum_sq.0.max(0) as u128);
        let (lo, carry) = lo.overflowing_add((1u128 << 64) - 1);
        let down = div_wide(hi + carry as u128, lo, 1u128 << 64).ok_or(MathError::Overflow)?;
        Ok((w + up).saturating_sub(down))
    }

    /// Hands `c` its `y / total` of the credit pool, in kind (y the corrected weight of `w`), then
    /// takes its income sale. Returns the sale, or `None` if the cohort isn't paying this epoch.
    #[allow(clippy::too_many_arguments)]
    pub fn credit_and_sell(
        c: &mut Cohort,
        w: u128,
        pool: &Shares,
        total: u128,
        sum_sq: Fx,
        payout_fraction: Fx,
        credited: &mut Shares,
        to_sell: &mut Shares,
    ) -> MathResult<Option<Shares>> {
        let y = corrected_weight(w, total, sum_sq)?;
        for a in 0..SLEEVES {
            let g = share_of(pool[a], y, total)?;
            c.shares[a] += g;
            credited[a] += g;
        }
        if payout_fraction.0 == 0 || c.units == 0 {
            return Ok(None);
        }
        let mut s = [0u128; SLEEVES];
        for a in 0..SLEEVES {
            s[a] = fraction_of(c.shares[a], payout_fraction)?;
            c.shares[a] -= s[a];
            to_sell[a] += s[a];
        }
        Ok(Some(s))
    }

    /// Books `c`'s income: its `sale_value / total_sale_value` of the USDG `received`, per whole
    /// (1e18) unit, rounded down. Returns what that figure pays out.
    pub fn book(c: &mut Cohort, sale_value: Fx, total_sale_value: Fx, received: Fx) -> MathResult<Fx> {
        if received.is_negative() || c.units == 0 {
            return Err(MathError::Domain);
        }
        let share = mul_div(received.0 as u128, sale_value.0 as u128, total_sale_value.0 as u128).ok_or(MathError::Overflow)?;
        let per_unit = mul_div(share, WAD, c.units).ok_or(MathError::Overflow)?;
        c.income_per_unit = c.income_per_unit.add(Fx(per_unit as i128))?;
        Ok(Fx(mul_div(per_unit, c.units, WAD).ok_or(MathError::Overflow)? as i128))
    }
}

impl Epoch<'_> {
    /// Fees → releases → mortality credits → income sale orders. `carry_in` is last month's dust.
    pub fn settle(&mut self, releases: &[Release], carry_in: Shares) -> MathResult<Settlement> {
        if self.months.len() != self.cohorts.len() {
            return Err(MathError::Domain);
        }
        let mut out = Settlement::default();

        // 1. Fee.
        for c in self.cohorts.iter_mut() {
            step::fee(c, self.fee_fraction, &mut out.fees)?;
        }

        // 2. Releases: tontine units feed the credit pool, bequest units become claims.
        let mut pool = carry_in;
        for r in releases {
            let c = self.cohorts.get_mut(r.cohort).ok_or(MathError::Domain)?;
            let left = step::release(c, r.units, &mut pool, &mut out.bequest_claims)?;
            out.bequest_by_release.push(left);
        }

        // 3. Credits: weight h·value over living tontine cohorts, paid in kind. With no weight
        // at all, nobody is credited and the whole pool carries to next month.
        let mut weights = Vec::with_capacity(self.cohorts.len());
        let mut total = 0u128;
        let mut sq = SumSq::default();
        for (i, c) in self.cohorts.iter().enumerate() {
            let w = step::weight(c, self.months[i].q, &self.prices)?;
            weights.push(w);
            total = total.checked_add(w).ok_or(MathError::Overflow)?;
            sq.add(w)?;
        }
        let sum_sq = sq.share(total)?;

        // 4. Income: paying cohorts sell their payout fraction (after their credit).
        for (i, c) in self.cohorts.iter_mut().enumerate() {
            if let Some(s) = step::credit_and_sell(c, weights[i], &pool, total, sum_sq, self.months[i].payout_fraction, &mut out.credited, &mut out.to_sell)? {
                out.sales.push(Sale { cohort: i, shares: s });
            }
        }
        for a in 0..SLEEVES {
            out.carry[a] = pool[a] - out.credited[a];
        }
        Ok(out)
    }

    /// After the treasury sold `to_sell` for `usdg_received`, raise each paying cohort's income
    /// per unit in proportion to the oracle value of what it sold. Slippage is shared pro rata
    /// and nobody is paid value the pool didn't realize. Returns the rounding remainder, which
    /// stays with the pool.
    pub fn book_income(&mut self, sales: &[Sale], usdg_received: Fx) -> MathResult<Fx> {
        if usdg_received.is_negative() {
            return Err(MathError::Domain);
        }
        let mut total_value = Fx::ZERO;
        for s in sales {
            total_value = total_value.add(value(&s.shares, &self.prices)?)?;
        }
        if total_value.0 == 0 {
            return Ok(usdg_received);
        }
        let mut booked = Fx::ZERO;
        for s in sales {
            let c = self.cohorts.get_mut(s.cohort).ok_or(MathError::Domain)?;
            booked = booked.add(step::book(c, value(&s.shares, &self.prices)?, total_value, usdg_received)?)?;
        }
        usdg_received.sub(booked)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    /// SplitMix64: deterministic scenarios without a dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        fn frac(&mut self, lo: f64, hi: f64) -> Fx {
            let u = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
            Fx(((lo + (hi - lo) * u) * (1u128 << 64) as f64) as i128)
        }
    }

    fn prices() -> [Fx; SLEEVES] {
        [Fx::from_ratio(1008, 1000).unwrap(), Fx::from_ratio(10117, 100).unwrap(), Fx::from_ratio(77233, 100).unwrap()]
    }

    fn scenario(rng: &mut Rng) -> (Vec<Cohort>, Vec<CohortMonth>, Vec<Release>, Shares) {
        let n = 2 + rng.below(20) as usize;
        let mut cohorts = Vec::new();
        let mut months = Vec::new();
        for _ in 0..n {
            let class = if rng.below(4) == 0 { Class::Bequest } else { Class::Tontine };
            let shares = [rng.below(1 << 40) as u128 * 1_000_000_000, rng.below(1 << 30) as u128 * 1_000_000_000, rng.below(1 << 28) as u128 * 1_000_000_000];
            let units = 1 + rng.below(1 << 40) as u128 * 1_000_000;
            cohorts.push(Cohort { class, shares, units, income_per_unit: Fx::ZERO });
            let paying = rng.below(2) == 0;
            months.push(CohortMonth { q: rng.frac(0.00005, 0.05), payout_fraction: if paying { rng.frac(0.001, 0.02) } else { Fx::ZERO } });
        }
        let mut releases = Vec::new();
        for i in 0..n {
            if rng.below(3) == 0 {
                let u = cohorts[i].units;
                releases.push(Release { cohort: i, units: if rng.below(4) == 0 { u } else { u / (1 + rng.below(50) as u128) } });
            }
        }
        let carry = [rng.below(1000) as u128, rng.below(1000) as u128, rng.below(1000) as u128];
        (cohorts, months, releases, carry)
    }

    fn sum(cs: &[Cohort]) -> Shares {
        let mut t = [0u128; SLEEVES];
        for c in cs {
            for a in 0..SLEEVES {
                t[a] += c.shares[a];
            }
        }
        t
    }

    #[test]
    fn settlement_conserves_every_share_exactly() {
        let mut rng = Rng(20260926);
        for _ in 0..10_000 {
            let (mut cohorts, months, releases, carry) = scenario(&mut rng);
            let before = sum(&cohorts);
            let bequest_before: Vec<Shares> = cohorts.iter().map(|c| c.shares).collect();
            let mut e = Epoch { cohorts: &mut cohorts, months: &months, prices: prices(), fee_fraction: Fx::from_ratio(3, 12_000).unwrap() };
            let s = e.settle(&releases, carry).unwrap();
            let after = sum(&cohorts);
            for a in 0..SLEEVES {
                assert_eq!(before[a] + carry[a], after[a] + s.fees[a] + s.bequest_claims[a] + s.to_sell[a] + s.carry[a]);
                assert_eq!(s.to_sell[a], s.sales.iter().map(|x| x.shares[a]).sum::<u128>());
                // Per-release bequest attribution sums exactly to the aggregate.
                assert_eq!(s.bequest_claims[a], s.bequest_by_release.iter().map(|x| x[a]).sum::<u128>());
            }
            assert_eq!(s.bequest_by_release.len(), releases.len());
            // Bequest cohorts never receive credits: they only ever shrink.
            for (c, b) in cohorts.iter().zip(&bequest_before) {
                if c.class == Class::Bequest {
                    assert!((0..SLEEVES).all(|a| c.shares[a] <= b[a]));
                }
            }
        }
    }

    /// The pool settles in pages: each cohort pays its fee when first touched (by a release or by
    /// the weighing pass), then weigh, credit-and-sell and book run as separate passes. That order
    /// must reproduce the one-shot ledger exactly, share for share and unit for unit.
    #[test]
    fn paged_settlement_equals_the_one_shot_ledger_exactly() {
        let mut rng = Rng(4663);
        let pr = prices();
        let fee_fraction = Fx::from_ratio(3, 12_000).unwrap();
        for _ in 0..10_000 {
            let (cohorts, months, releases, carry) = scenario(&mut rng);
            let factor = rng.frac(0.97, 1.005);

            // One shot.
            let mut once = cohorts.clone();
            let mut e = Epoch { cohorts: &mut once, months: &months, prices: pr, fee_fraction };
            let s = e.settle(&releases, carry).unwrap();
            let received = value(&s.to_sell, &pr).unwrap().mul(factor).unwrap();
            let remainder = e.book_income(&s.sales, received).unwrap();

            // Paged.
            let mut paged = cohorts.clone();
            let mut charged = alloc::vec![false; paged.len()];
            let (mut fees, mut claims, mut pool) = ([0u128; SLEEVES], [0u128; SLEEVES], carry);
            let mut touch = |k: usize, c: &mut Cohort, fees: &mut Shares| {
                if !charged[k] {
                    step::fee(c, fee_fraction, fees).unwrap();
                    charged[k] = true;
                }
            };
            let mut lefts = Vec::new();
            for r in &releases {
                touch(r.cohort, &mut paged[r.cohort], &mut fees);
                lefts.push(step::release(&mut paged[r.cohort], r.units, &mut pool, &mut claims).unwrap());
            }
            let mut weights = Vec::new();
            let mut total = 0u128;
            let mut sq = SumSq::default();
            for k in 0..paged.len() {
                touch(k, &mut paged[k], &mut fees);
                let w = step::weight(&paged[k], months[k].q, &pr).unwrap();
                weights.push(w);
                total += w;
                sq.add(w).unwrap();
            }
            let sum_sq = sq.share(total).unwrap();
            let (mut credited, mut to_sell, mut sale_values, mut total_sale) = ([0u128; SLEEVES], [0u128; SLEEVES], Vec::new(), Fx::ZERO);
            for k in 0..paged.len() {
                let sale = step::credit_and_sell(&mut paged[k], weights[k], &pool, total, sum_sq, months[k].payout_fraction, &mut credited, &mut to_sell).unwrap();
                let v = match sale {
                    Some(sh) => value(&sh, &pr).unwrap(),
                    None => Fx::ZERO,
                };
                total_sale = total_sale.add(v).unwrap();
                sale_values.push(sale.map(|_| v));
            }
            let mut booked = Fx::ZERO;
            for k in 0..paged.len() {
                if let (Some(v), true) = (sale_values[k], total_sale.0 > 0) {
                    booked = booked.add(step::book(&mut paged[k], v, total_sale, received).unwrap()).unwrap();
                }
            }

            assert_eq!(paged, once, "every cohort identical");
            assert_eq!((fees, claims, credited, to_sell), (s.fees, s.bequest_claims, s.credited, s.to_sell));
            assert_eq!(lefts, s.bequest_by_release);
            let paged_carry: Shares = core::array::from_fn(|a| pool[a] - credited[a]);
            assert_eq!(paged_carry, s.carry);
            let paged_remainder = if total_sale.0 > 0 { received.sub(booked).unwrap() } else { received };
            assert_eq!(paged_remainder, remainder);
        }
    }

    /// The correction only moves credit between members: over random weight sets, corrected weights
    /// never sum past W (so credits never exceed the release) and fall short by rounding only.
    #[test]
    fn corrected_weights_never_sum_past_the_total() {
        let mut rng = Rng(77);
        for _ in 0..20_000 {
            let n = 1 + rng.below(40) as usize;
            // Up to 2¹⁰⁰: h·value for a pool of about $1B.
            let ws: Vec<u128> = (0..n).map(|_| (rng.next() as u128) << rng.below(37)).collect();
            let total: u128 = ws.iter().sum();
            let mut sq = SumSq::default();
            ws.iter().for_each(|&w| sq.add(w).unwrap());
            let s = sq.share(total).unwrap();
            let ys: u128 = ws.iter().map(|&w| step::corrected_weight(w, total, s).unwrap()).sum();
            assert!(ys <= total, "Σy {ys} > W {total}");
            // Rounding Σs² up by an ulp costs about W·2⁻⁶³ in all; it carries to next month's credits.
            assert!(total - ys <= (total >> 62) + 3 * n as u128 + 2, "rounding shortfall {} of {total}", total - ys);
        }
    }

    #[test]
    fn income_booking_never_pays_more_than_received() {
        let mut rng = Rng(7);
        for _ in 0..10_000 {
            let (mut cohorts, months, releases, carry) = scenario(&mut rng);
            let mut e = Epoch { cohorts: &mut cohorts, months: &months, prices: prices(), fee_fraction: Fx::ZERO };
            let s = e.settle(&releases, carry).unwrap();
            let sale_value = value(&s.to_sell, &prices()).unwrap();
            // The treasury realizes somewhere between 97% and 100.5% of oracle value.
            let received = sale_value.mul(rng.frac(0.97, 1.005)).unwrap();
            let units_before: Vec<(u128, Fx)> = e.cohorts.iter().map(|c| (c.units, c.income_per_unit)).collect();
            let remainder = e.book_income(&s.sales, received).unwrap();
            assert!(!remainder.is_negative());
            let mut paid = 0u128;
            for (c, (u, inc0)) in e.cohorts.iter().zip(units_before) {
                paid += mul_div(u, (c.income_per_unit.0 - inc0.0) as u128, WAD).unwrap();
            }
            assert!(paid <= received.0 as u128, "paid {paid} > received {}", received.0);
            // Rounding loss per paying cohort is below 2 + units/1e18 raw units (one from splitting the
            // proceeds, up to units/1e18 + 1 from the per-unit floor): about 1e-19 USDG.
            let bound: u128 = s.sales.iter().map(|x| 2 + e.cohorts[x.cohort].units / WAD + 1).sum::<u128>() + 1;
            assert!(received.0 as u128 - paid <= bound, "remainder {} > bound {bound}", received.0 as u128 - paid);
        }
    }

    #[test]
    fn credits_follow_the_corrected_odds_times_value_weights() {
        // Two identical-value tontine cohorts, one with twice the odds. Credits follow
        // y = w·(1 + s − Σs²), w = h·value: the bigger weight gets a little more than twice.
        let base = Cohort { class: Class::Tontine, shares: [1_000 * WAD, 0, 0], units: 1_000 * WAD, income_per_unit: Fx::ZERO };
        let mut cohorts = [base.clone(), base.clone(), base];
        let q1 = Fx::from_ratio(1, 100).unwrap();
        let q2 = q1.div(Fx::ONE.add(q1).unwrap()).unwrap().mul_int(2).unwrap(); // odds exactly 2×(1/99)… close enough
        let months = [
            CohortMonth { q: q1, payout_fraction: Fx::ZERO },
            CohortMonth { q: q2, payout_fraction: Fx::ZERO },
            CohortMonth { q: q1, payout_fraction: Fx::ZERO },
        ];
        let mut e = Epoch { cohorts: &mut cohorts, months: &months, prices: prices(), fee_fraction: Fx::ZERO };
        let s = e.settle(&[Release { cohort: 2, units: 1_000 * WAD }], [0; SLEEVES]).unwrap();
        let g0 = cohorts[0].shares[0] - 1_000 * WAD;
        let g1 = cohorts[1].shares[0] - 1_000 * WAD;
        assert_eq!(g0 + g1 + s.carry[0], 1_000 * WAD);
        let ratio = g1 as f64 / g0 as f64;
        let h = |q: f64| q / (1.0 - q);
        let (w0, w1) = (h(0.01), h(2.0 * 0.01 / 1.01));
        let (s0, s1) = (w0 / (w0 + w1), w1 / (w0 + w1));
        let ss = s0 * s0 + s1 * s1;
        let want = (w1 * (1.0 + s1 - ss)) / (w0 * (1.0 + s0 - ss));
        assert!((ratio - want).abs() < 1e-9, "ratio {ratio} want {want}");
        assert_eq!(cohorts[2].units, 0);
    }
}
