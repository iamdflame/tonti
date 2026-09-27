//! Glide-path rebalancing across cohorts (docs/protocol.md §8), with exact share conservation.
//!
//! Each cohort drifts from its age's target mix (SPY share by the glide path; the rest 70/30
//! SGOV/cash). Cohorts wanting opposite trades in the same sleeve cross internally at the oracle
//! price, and only the net goes to market. Market fills (slippage) are shared by the trading
//! cohorts in proportion to their trade value; cohorts that don't trade are untouched. Integer
//! shares are conserved exactly: whatever can't be assigned carries to the next month's credits.
//!
//! Two phases, because the market trades happen in between (in the Treasury):
//!   `plan` → shares to sell and USDG to spend per sleeve → Treasury trades → `apply`.

use alloc::vec::Vec;

use crate::fixed::{Fx, MathError, MathResult};
use crate::ledger::{value, Cohort, Shares, SLEEVES};
use crate::quote::spy_share;
use crate::wide::mul_div;

const WAD: u128 = 1_000_000_000_000_000_000;

/// Target weights [cash, SGOV, SPY] for a cohort aged `age`.
pub fn target_weights(age: Fx) -> MathResult<[Fx; SLEEVES]> {
    let spy = spy_share(age)?;
    let rest = Fx::ONE.sub(spy)?;
    let sgov = rest.mul(Fx::from_ratio(7, 10)?)?;
    Ok([rest.sub(sgov)?, sgov, spy])
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Shares each cohort gives up, per sleeve.
    pub give: Vec<Shares>,
    /// USDG value each cohort wants to add, per sleeve.
    pub want: Vec<[Fx; SLEEVES]>,
    /// Shares of each sleeve to sell to market (net sellers' excess after crossing).
    pub sell: Shares,
    /// Shares handed from sellers to buyers inside the pool, per sleeve.
    pub crossed: Shares,
    /// USDG value to buy in market, per sleeve (net buyers' excess after crossing).
    pub buy_value: [Fx; SLEEVES],
}

fn shares_for(v: Fx, price: Fx) -> MathResult<u128> {
    if v.0 <= 0 || price.0 <= 0 {
        return Ok(0);
    }
    mul_div(v.0 as u128, WAD, price.0 as u128).ok_or(MathError::Overflow)
}

/// Rebalancing one cohort at a time, for the pool's paged rebalance. `plan` and `apply` run these
/// steps over every cohort at once. Each step reads other cohorts only through the per-sleeve
/// totals it's handed, so both orders give the same result to the last share.
pub mod step {
    use super::*;

    /// One cohort's trade toward its age's target mix: the shares it gives up and the USDG value
    /// it wants to add, per sleeve. Nothing if it's within `threshold` (a fraction of its value)
    /// in every sleeve, or empty.
    pub fn plan(c: &Cohort, age: Fx, prices: &[Fx; SLEEVES], threshold: Fx) -> MathResult<(Shares, [Fx; SLEEVES])> {
        let (mut give, mut want) = ([0u128; SLEEVES], [Fx::ZERO; SLEEVES]);
        if c.units == 0 {
            return Ok((give, want));
        }
        let v = value(&c.shares, prices)?;
        if v.0 <= 0 {
            return Ok((give, want));
        }
        let w = target_weights(age)?;
        let mut drift = [Fx::ZERO; SLEEVES];
        let mut worst = Fx::ZERO;
        for a in 0..SLEEVES {
            let cur = value(&single(a, c.shares[a]), prices)?;
            drift[a] = v.mul(w[a])?.sub(cur)?;
            let d = if drift[a].is_negative() { drift[a].neg()? } else { drift[a] };
            if d > worst {
                worst = d;
            }
        }
        if worst <= v.mul(threshold)? {
            return Ok((give, want));
        }
        for a in 0..SLEEVES {
            if drift[a].is_negative() {
                give[a] = shares_for(drift[a].neg()?, prices[a])?.min(c.shares[a]);
            } else if drift[a].0 > 0 {
                want[a] = drift[a];
            }
        }
        Ok((give, want))
    }

    /// From the totals given and wanted per sleeve: shares crossed inside the pool, shares to sell
    /// to market, and USDG value to buy in market.
    pub fn net(gave: &Shares, want_total: &[Fx; SLEEVES], prices: &[Fx; SLEEVES]) -> MathResult<(Shares, Shares, [Fx; SLEEVES])> {
        let (mut crossed, mut sell, mut buy_value) = ([0u128; SLEEVES], [0u128; SLEEVES], [Fx::ZERO; SLEEVES]);
        for a in 0..SLEEVES {
            let wanted = shares_for(want_total[a], prices[a])?;
            crossed[a] = gave[a].min(wanted);
            sell[a] = gave[a] - crossed[a];
            let crossed_value = value(&single(a, crossed[a]), prices)?;
            buy_value[a] = if want_total[a] > crossed_value { want_total[a].sub(crossed_value)? } else { Fx::ZERO };
        }
        Ok((crossed, sell, buy_value))
    }

    /// Removes the cohort's `give` and adds its share of each sleeve's `pot` (crossed plus bought
    /// shares), pro rata to its wanted value. Adds what it received to `given`.
    pub fn apply(c: &mut Cohort, give: &Shares, want: &[Fx; SLEEVES], pot: &Shares, want_total: &[Fx; SLEEVES], given: &mut Shares) -> MathResult<()> {
        for a in 0..SLEEVES {
            c.shares[a] = c.shares[a].checked_sub(give[a]).ok_or(MathError::Domain)?;
            let (w, total) = (want[a].0.max(0) as u128, want_total[a].0.max(0) as u128);
            if w == 0 || total == 0 {
                continue;
            }
            let s = mul_div(pot[a], w, total).ok_or(MathError::Overflow)?;
            c.shares[a] += s;
            given[a] += s;
        }
        Ok(())
    }
}

/// `ages[k]` is cohort k's age (only cohorts with units are considered). A cohort trades only if
/// some sleeve is more than `threshold` (a fraction of its value) away from target.
pub fn plan(cohorts: &[Cohort], ages: &[Fx], prices: &[Fx; SLEEVES], threshold: Fx) -> MathResult<Plan> {
    if ages.len() != cohorts.len() {
        return Err(MathError::Domain);
    }
    let mut p = Plan::default();
    let (mut gave, mut want_total) = ([0u128; SLEEVES], [Fx::ZERO; SLEEVES]);
    for (k, c) in cohorts.iter().enumerate() {
        let (give, want) = step::plan(c, ages[k], prices, threshold)?;
        for a in 0..SLEEVES {
            gave[a] += give[a];
            want_total[a] = want_total[a].add(want[a])?;
        }
        p.give.push(give);
        p.want.push(want);
    }
    // Cross inside each sleeve at the oracle price; only the imbalance goes to market.
    (p.crossed, p.sell, p.buy_value) = step::net(&gave, &want_total, prices)?;
    Ok(p)
}

fn single(a: usize, s: u128) -> Shares {
    let mut x = [0u128; SLEEVES];
    x[a] = s;
    x
}

/// Applies a plan after the Treasury sold `plan.sell` and bought `bought[a]` shares of each sleeve
/// by spending USDG. Removes each cohort's `give`, then hands each buyer its share of the crossed
/// shares plus the market-bought shares, pro rata to its wanted value. Returns the dust per sleeve.
pub fn apply(cohorts: &mut [Cohort], p: &Plan, bought: &Shares) -> MathResult<Shares> {
    let mut want_total = [Fx::ZERO; SLEEVES];
    for w in &p.want {
        for a in 0..SLEEVES {
            want_total[a] = want_total[a].add(w[a])?;
        }
    }
    let pot: Shares = core::array::from_fn(|a| p.crossed[a] + bought[a]);
    let mut given = [0u128; SLEEVES];
    for (k, c) in cohorts.iter_mut().enumerate() {
        let give = p.give.get(k).copied().unwrap_or([0; SLEEVES]);
        let want = p.want.get(k).copied().unwrap_or([Fx::ZERO; SLEEVES]);
        step::apply(c, &give, &want, &pot, &want_total, &mut given)?;
    }
    Ok(core::array::from_fn(|a| pot[a] - given[a]))
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::ledger::Class;

    fn prices() -> [Fx; SLEEVES] {
        [Fx::from_ratio(1008, 1000).unwrap(), Fx::from_ratio(10117, 100).unwrap(), Fx::from_ratio(77233, 100).unwrap()]
    }

    fn cash_only(usdg: u128) -> Cohort {
        Cohort { class: Class::Tontine, shares: [usdg * WAD * 1000 / 1008, 0, 0], units: usdg * WAD, income_per_unit: Fx::ZERO }
    }

    #[test]
    fn young_and_old_cross_before_touching_the_market() {
        // A young cohort all in SPY-less cash wants 80% SPY; an old cohort holding SPY wants only 30%.
        let young = cash_only(1_000);
        let mut old = cash_only(0);
        old.units = 1_000 * WAD;
        old.shares = [0, 0, 1_000 * WAD * 100 / 77233]; // ~$1,000 all in SPY
        let mut cohorts = [young, old];
        let ages = [Fx::from_int(30), Fx::from_int(80)];
        let pr = prices();
        let p = plan(&cohorts, &ages, &pr, Fx::from_ratio(5, 100).unwrap()).unwrap();
        // The old cohort's excess SPY is crossed to the young one instead of being sold.
        assert!(p.crossed[2] > 0, "SPY crossed internally");
        assert_eq!(p.sell[2], 0, "no SPY hits the market");
        let before: Shares = [0, 1, 2].map(|a| cohorts.iter().map(|c| c.shares[a]).sum::<u128>());
        // Simulate the market: cash sold becomes SGOV/SPY bought at oracle price.
        let bought = [0, shares_for(p.buy_value[1], pr[1]).unwrap(), shares_for(p.buy_value[2], pr[2]).unwrap()];
        let dust = apply(&mut cohorts, &p, &bought).unwrap();
        for a in 0..SLEEVES {
            let after: u128 = cohorts.iter().map(|c| c.shares[a]).sum();
            assert_eq!(before[a] - p.sell[a] + bought[a], after + dust[a], "sleeve {a} conserved exactly");
        }
        // Both end close to their targets.
        for (k, c) in cohorts.iter().enumerate() {
            let v = value(&c.shares, &pr).unwrap();
            let w = target_weights(ages[k]).unwrap();
            let spy_w = value(&single(2, c.shares[2]), &pr).unwrap().div(v).unwrap();
            let diff = (spy_w.0 - w[2].0).abs() as f64 / (1u128 << 64) as f64;
            assert!(diff < 0.02, "cohort {k} SPY weight off by {diff}");
        }
    }

    #[test]
    fn within_threshold_nothing_trades() {
        let pr = prices();
        let w = target_weights(Fx::from_int(30)).unwrap();
        let v = 1_000u128;
        let shares = [0, 1, 2].map(|a| shares_for(Fx::from_int(v as i64).mul(w[a]).unwrap(), pr[a]).unwrap());
        let cohorts = [Cohort { class: Class::Tontine, shares, units: v * WAD, income_per_unit: Fx::ZERO }];
        let p = plan(&cohorts, &[Fx::from_int(30)], &pr, Fx::from_ratio(5, 100).unwrap()).unwrap();
        assert_eq!(p.sell, [0, 0, 0]);
        assert!(p.buy_value.iter().all(|b| b.0 == 0));
    }

    #[test]
    fn random_rebalances_conserve_every_share() {
        let mut x: u64 = 20260926;
        let mut next = move || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let pr = prices();
        for _ in 0..10_000 {
            let n = 1 + (next() % 12) as usize;
            let mut cohorts = Vec::new();
            let mut ages = Vec::new();
            for _ in 0..n {
                let shares = [(next() % (1 << 50)) as u128 * 1_000_000, (next() % (1 << 40)) as u128 * 1_000_000, (next() % (1 << 38)) as u128 * 1_000_000];
                cohorts.push(Cohort { class: Class::Tontine, shares, units: 1 + (next() % (1 << 50)) as u128, income_per_unit: Fx::ZERO });
                ages.push(Fx::from_int(20 + (next() % 70) as i64));
            }
            let before: Shares = [0, 1, 2].map(|a| cohorts.iter().map(|c| c.shares[a]).sum::<u128>());
            let p = plan(&cohorts, &ages, &pr, Fx::from_ratio(5, 100).unwrap()).unwrap();
            // Market fills between 97% and 100.3% of oracle.
            let fill = |v: Fx, a: usize, r: u64| -> u128 {
                let f = 0.97 + (r % 3300) as f64 / 100_000.0;
                (shares_for(v, pr[a]).unwrap() as f64 * f) as u128
            };
            let bought = [fill(p.buy_value[0], 0, next()), fill(p.buy_value[1], 1, next()), fill(p.buy_value[2], 2, next())];
            let dust = apply(&mut cohorts, &p, &bought).unwrap();
            for a in 0..SLEEVES {
                let after: u128 = cohorts.iter().map(|c| c.shares[a]).sum();
                assert_eq!(before[a] - p.sell[a] + bought[a], after + dust[a], "sleeve {a}");
                assert!(p.crossed[a] + p.sell[a] == p.give.iter().map(|g| g[a]).sum::<u128>());
            }
        }
    }
}

