//! Fairness of the credit allocation in finite pools (docs/protocol.md §3), measured through
//! the real ledger code.
//!
//! Fair means every member's expected net transfer is zero:
//!   E[net_i] = (1 − q_i)·E[credit_i | i survives] − q_i·T_i = 0.
//! q_i is known exactly, so only E[credit_i | i survives] is estimated. Every trial contributes
//! to it: if i survived, their credit is read from the ledger; if i died, the credit they would
//! have received alive is computed counterfactually as (R − T_i)·w_i/(W + w_i). Other members'
//! deaths are independent of i's, so this is unbiased. It also avoids the rare-event failure of
//! naive estimation, where a young member who never dies in the sample looks +100% biased.
//!
//! For every surviving member, every trial asserts that the formula reproduces the ledger's
//! in-kind credit exactly. The measurement can't drift from the code it measures.

use actuary_core::ledger::{step, value, Class, Cohort, CohortMonth, Epoch, Release, SumSq, SLEEVES};
use actuary_core::mortality::Gm;
use actuary_core::wide::mul_div;
use actuary_core::Fx;

const WAD: u128 = 1_000_000_000_000_000_000;

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

fn to_f(x: Fx) -> f64 {
    x.0 as f64 / 18_446_744_073_709_551_616.0
}

pub struct Report {
    pub members: usize,
    pub trials: usize,
    /// Worst |E[net]| / (q·T) over members, with its standard error.
    pub worst_bias: f64,
    pub worst_bias_se: f64,
    pub rms_bias: f64,
    /// Mean |bias/SE|: about 0.8 if all remaining bias is noise.
    pub mean_abs_z: f64,
    pub ledger_checks: u64,
    /// The same deaths under plain h·value weights (before the §3 correction), for comparison.
    pub uncorrected_worst_bias: f64,
    pub uncorrected_rms_bias: f64,
}

pub fn run(members: usize, trials: usize, seed: u64, gm: &Gm, year: Fx) -> Report {
    let mut rng = Rng(seed);
    let prices = [Fx::ONE; SLEEVES];
    let mut base = Vec::with_capacity(members);
    let mut months = Vec::with_capacity(members);
    for _ in 0..members {
        let age = Fx::from_int(40).add(Fx(((rng.unit() * 50.0) * 18_446_744_073_709_551_616.0) as i128)).unwrap();
        let bal = (1_000.0 * (rng.unit() * 3.4).exp()) as u128; // $1k to ~$30k
        base.push(Cohort { class: Class::Tontine, shares: [bal * WAD, 0, 0], units: bal * WAD, income_per_unit: Fx::ZERO });
        months.push(CohortMonth { q: gm.q_month(age, year).unwrap(), payout_fraction: Fx::ZERO });
    }
    // Weights exactly as the ledger computes them.
    let w: Vec<u128> = (0..members)
        .map(|i| {
            let q = months[i].q;
            let odds = q.div(Fx::ONE.sub(q).unwrap()).unwrap();
            odds.mul(value(&base[i].shares, &prices).unwrap()).unwrap().0 as u128
        })
        .collect();
    let q: Vec<f64> = months.iter().map(|m| to_f(m.q)).collect();
    let t: Vec<u128> = base.iter().map(|c| c.shares[0]).collect();
    let mut sum = vec![0f64; members];
    let mut sumsq = vec![0f64; members];
    let mut sum_raw = vec![0f64; members];
    let mut checks = 0u64;
    let mut dead = vec![false; members];

    for _ in 0..trials {
        let mut releases = Vec::new();
        for i in 0..members {
            dead[i] = rng.unit() < q[i];
            if dead[i] {
                releases.push(Release { cohort: i, units: base[i].units });
            }
        }
        let mut cohorts = base.clone();
        let mut e = Epoch { cohorts: &mut cohorts, months: &months, prices, fee_fraction: Fx::ZERO };
        e.settle(&releases, [0; SLEEVES]).expect("settle");

        let r: u128 = (0..members).filter(|&i| dead[i]).map(|i| t[i]).sum();
        let w_alive: u128 = (0..members).filter(|&i| !dead[i]).map(|i| w[i]).sum();
        let mut sq_alive = SumSq::default();
        (0..members).filter(|&i| !dead[i]).for_each(|i| sq_alive.add(w[i]).unwrap());
        let s_alive = sq_alive.share(w_alive).unwrap();
        for i in 0..members {
            // Corrected weight y (§3), over the survivors plus i: exactly what i would get alive.
            let (credit, raw) = if dead[i] {
                let total = w_alive + w[i];
                let mut sq = sq_alive;
                sq.add(w[i]).unwrap();
                let y = step::corrected_weight(w[i], total, sq.share(total).unwrap()).unwrap();
                (mul_div(r - t[i], y, total).unwrap(), mul_div(r - t[i], w[i], total).unwrap())
            } else {
                let y = step::corrected_weight(w[i], w_alive, s_alive).unwrap();
                let formula = if w_alive == 0 { 0 } else { mul_div(r, y, w_alive).unwrap() };
                let ledger = cohorts[i].shares[0] - t[i];
                assert_eq!(formula, ledger, "formula must reproduce the ledger's credit");
                checks += 1;
                (formula, if w_alive == 0 { 0 } else { mul_div(r, w[i], w_alive).unwrap() })
            };
            let x = credit as f64 / t[i] as f64; // credit as a fraction of balance
            sum[i] += x;
            sumsq[i] += x * x;
            sum_raw[i] += raw as f64 / t[i] as f64;
        }
    }

    let n = trials as f64;
    let (mut worst, mut worst_se, mut rms, mut abs_z) = (0f64, 0f64, 0f64, 0f64);
    let (mut worst_raw, mut rms_raw) = (0f64, 0f64);
    for i in 0..members {
        let raw = (1.0 - q[i]) * (sum_raw[i] / n) / q[i] - 1.0;
        rms_raw += raw * raw;
        if raw.abs() > worst_raw.abs() {
            worst_raw = raw;
        }
        let mean = sum[i] / n;
        let se = ((sumsq[i] / n - mean * mean).max(0.0) / n).sqrt();
        // Relative bias: E[net]/(qT) = (1−q)·E[credit/T | survive]/q − 1.
        let bias = (1.0 - q[i]) * mean / q[i] - 1.0;
        let bias_se = (1.0 - q[i]) * se / q[i];
        rms += bias * bias;
        if bias_se > 0.0 {
            abs_z += (bias / bias_se).abs();
        }
        if bias.abs() > worst.abs() {
            worst = bias;
            worst_se = bias_se;
        }
    }
    Report {
        members,
        trials,
        worst_bias: worst,
        worst_bias_se: worst_se,
        rms_bias: (rms / members as f64).sqrt(),
        mean_abs_z: abs_z / members as f64,
        ledger_checks: checks,
        uncorrected_worst_bias: worst_raw,
        uncorrected_rms_bias: (rms_raw / members as f64).sqrt(),
    }
}
