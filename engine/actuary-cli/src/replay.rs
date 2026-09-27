//! Historical replay (docs/protocol.md §10): what the pool would have paid a real cohort through
//! real history, run through the production ledger (`Epoch::settle` and `book_income`).
//!
//! Input: one line per month, `spy_return tbill_return q_month payout_fraction`. Deaths are drawn
//! from the cohort's historical monthly death probability; the payout fraction comes from the life
//! table an actuary would have used that year (no hindsight). Sleeves follow the 65+ glide path
//! (30% SPY, 49% SGOV, 21% cash; SGOV and cash earn the T-bill return), rebalanced monthly.
//! Alongside, three solo benchmarks hold the same pot in the same mix: the pool's exact income
//! stream drawn alone, the pool's payout rule without pooling, and the 4% rule.

use actuary_core::ledger::{Class, Cohort, CohortMonth, Epoch, Release, SLEEVES};
use actuary_core::Fx;

const WAD: u128 = 1_000_000_000_000_000_000;
const Q: f64 = 18_446_744_073_709_551_616.0;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn fx(v: f64) -> Fx {
    Fx((v * Q) as i128)
}
fn fl(v: Fx) -> f64 {
    v.0 as f64 / Q
}

pub struct Month {
    pub spy: f64,
    pub tbill: f64,
    pub q: f64,
    pub payout: f64,
    /// Consumer-price change this month (Shiller CPI-U).
    pub inflation: f64,
}

/// One row per completed year. Incomes are totals for the year, per surviving member; `_real`
/// values are in the dollars of the first month.
pub struct Year {
    pub alive: usize,
    pub income: f64,
    pub income_real: f64,
    /// The pool's own payout rule applied to one member's money with no pooling.
    pub self_income_real: f64,
    /// Balance left if the pool's exact income stream had been drawn alone.
    pub shadow_balance: f64,
    /// Balance left under the 4% rule (4% of the pot a year, raised with inflation).
    pub rule4_balance: f64,
}

pub struct Report {
    pub years: Vec<Year>,
    /// Month the pool's own income stream, drawn alone, runs out.
    pub shadow_runout: Option<usize>,
    /// Month the 4% rule runs out.
    pub rule4_runout: Option<usize>,
    pub first_month_income: f64,
}

/// One-way trading cost against the oracle price, from the mainnet fork test (half of the measured
/// round trip): cash 0, SGOV 4.75 bps, SPY 6.25 bps. Charged to the pool only; the solo benchmarks
/// trade for free and pay no fee, so every comparison leans toward the benchmark.
const COST: [f64; SLEEVES] = [0.0, 0.000_475, 0.000_625];

/// `pot` USDG per member, `members` in one cohort.
pub fn run(months: &[Month], members: usize, pot: f64, seed: u64) -> Report {
    let weights = [0.21, 0.49, 0.30];
    let mut prices = [1.0f64; SLEEVES];
    let total = pot * members as f64;
    let mut shares = [0u128; SLEEVES];
    for a in 0..SLEEVES {
        shares[a] = (total * weights[a] / prices[a] * WAD as f64) as u128;
    }
    let unit_each = (pot * WAD as f64) as u128;
    let mut cohort = [Cohort { class: Class::Tontine, shares, units: unit_each * members as u128, income_per_unit: Fx::ZERO }];
    let mut alive = members;
    let mut rng = Rng(seed);
    let mut carry = [0u128; SLEEVES];
    let mut years = Vec::new();
    let (mut y_inc, mut y_real, mut y_self) = (0f64, 0f64, 0f64);
    let (mut shadow, mut self_bal, mut rule4) = (pot, pot, pot);
    let (mut shadow_runout, mut rule4_runout) = (None, None);
    let mut cpi = 1.0f64;
    let mut first = None::<f64>;

    for (m, mo) in months.iter().enumerate() {
        prices[0] *= 1.0 + mo.tbill;
        prices[1] *= 1.0 + mo.tbill;
        prices[2] *= 1.0 + mo.spy;
        cpi *= 1.0 + mo.inflation;
        let fxp = [fx(prices[0]), fx(prices[1]), fx(prices[2])];
        // Deaths this month.
        let mut died = 0usize;
        for _ in 0..alive {
            if rng.unit() < mo.q {
                died += 1;
            }
        }
        let releases: Vec<Release> = if died > 0 { alloc_release(died, unit_each) } else { Vec::new() };
        let months_in = [CohortMonth { q: fx(mo.q), payout_fraction: fx(mo.payout) }];
        let mut ep = Epoch { cohorts: &mut cohort, months: &months_in, prices: fxp, fee_fraction: fx(0.003 / 12.0) };
        let s = ep.settle(&releases, carry).expect("settle");
        carry = s.carry;
        alive -= died;
        // The Treasury sells the income shares; each sleeve pays its trading cost.
        let received: f64 = (0..SLEEVES).map(|a| s.to_sell[a] as f64 / WAD as f64 * prices[a] * (1.0 - COST[a])).sum();
        let received = fx(received);
        let before = cohort[0].income_per_unit;
        let mut ep = Epoch { cohorts: &mut cohort, months: &months_in, prices: fxp, fee_fraction: Fx::ZERO };
        ep.book_income(&s.sales, received).expect("book");
        let per_unit = fl(cohort[0].income_per_unit) - fl(before);
        let income = per_unit * unit_each as f64 / WAD as f64;
        first.get_or_insert(income);
        y_inc += income;
        y_real += income / cpi;
        // Rebalance the cohort back to the glide-path mix, paying the cost of the turnover.
        let held: Vec<f64> = (0..SLEEVES).map(|a| cohort[0].shares[a] as f64 / WAD as f64 * prices[a]).collect();
        let v: f64 = held.iter().sum();
        let turnover_cost: f64 = (0..SLEEVES).map(|a| (v * weights[a] - held[a]).abs() * COST[a]).sum();
        let v = v - turnover_cost;
        for a in 0..SLEEVES {
            cohort[0].shares[a] = (v * weights[a] / prices[a] * WAD as f64) as u128;
        }
        // Benchmarks: one member alone, same mix and market returns, no fee, no trading cost.
        let gross = weights[0] * (1.0 + mo.tbill) + weights[1] * (1.0 + mo.tbill) + weights[2] * (1.0 + mo.spy);
        if shadow_runout.is_none() {
            shadow = shadow * gross - income;
            if shadow <= 0.0 {
                shadow_runout = Some(m);
                shadow = 0.0;
            }
        }
        self_bal *= gross;
        let self_income = self_bal * mo.payout;
        self_bal -= self_income;
        y_self += self_income / cpi;
        if rule4_runout.is_none() {
            rule4 = rule4 * gross - pot * 0.04 / 12.0 * cpi;
            if rule4 <= 0.0 {
                rule4_runout = Some(m);
                rule4 = 0.0;
            }
        }
        if m % 12 == 11 {
            years.push(Year { alive, income: y_inc, income_real: y_real, self_income_real: y_self, shadow_balance: shadow, rule4_balance: rule4 });
            (y_inc, y_real, y_self) = (0.0, 0.0, 0.0);
        }
        if alive == 0 {
            break;
        }
    }
    Report { years, shadow_runout, rule4_runout, first_month_income: first.unwrap_or(0.0) }
}

fn alloc_release(died: usize, unit_each: u128) -> Vec<Release> {
    // One cohort: every death releases one member's units from cohort 0.
    (0..died).map(|_| Release { cohort: 0, units: unit_each }).collect()
}
