//! Power of the ghost-member detector (docs/protocol.md §6), measured with the real SPRT code.
//!
//! Monte Carlo: a group of members (one country, one birth-year band) lives month by month on the
//! fitted UN WPP mortality. A fraction `hidden` of real deaths is concealed by heirs, who keep the
//! member "alive" to collect income. The books still count ghosts as living, so the expected deaths
//! E include them. Reported deaths become final, and leave the books, 4–6 months after they happen
//! (a report within a month, then the 120-day challenge window). The contract therefore tests the
//! deaths made final this month against the deaths it expected `lag` months ago; so does this.
//! Reported: the share of runs flagged within 12, 24 and 36 months, the median months to a flag,
//! and (with hidden = 0) the false-alarm rate.

use actuary_core::fraud::{Signal, Sprt};
use actuary_core::mortality::Gm;
use actuary_core::Fx;

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

pub struct Outcome {
    pub hidden: f64,
    pub runs: usize,
    pub flagged_12m: f64,
    pub flagged_24m: f64,
    pub flagged_36m: f64,
    pub median_months: Option<usize>,
}

fn to_f(x: Fx) -> f64 {
    x.0 as f64 / 18_446_744_073_709_551_616.0
}

/// `cohorts`: (Gm params, birth year, members) groups that make up the pool. `lag` is how many
/// months back the expected deaths are taken from (the contract's report lag); 0 tests each month
/// against itself, as if deaths were final the moment they happen. `health` scales the true death
/// rates against the tables the books expect (0.8: a group 20% healthier); honest deaths become
/// final `delay.0` to `delay.0 + delay.1 − 1` months after they happen.
#[allow(clippy::too_many_arguments)]
pub fn run(cohorts: &[(Gm, u32, usize)], hidden: f64, horizon: usize, runs: usize, seed: u64, thresholds: (Fx, Fx), lag: usize, health: f64, delay: (usize, usize)) -> Outcome {
    let mut rng = Rng(seed);
    let mut months_to_flag = Vec::new();
    let (mut f12, mut f24, mut f36) = (0usize, 0usize, 0usize);
    for _ in 0..runs {
        // alive = truly alive; ghosts = dead but still on the books; pending[m] = reported deaths
        // that become final (and leave the books) in month m.
        let mut alive: Vec<(usize, usize)> = cohorts.iter().map(|&(_, _, n)| (n, 0)).collect();
        let mut pending = vec![vec![0usize; cohorts.len()]; horizon + delay.0 + delay.1 + 1];
        let mut on_books_pending = vec![0usize; cohorts.len()];
        let mut expected_hist = Vec::with_capacity(horizon);
        let mut sprt = Sprt::default();
        let mut flagged = None;
        for m in 0..horizon {
            let year = 2026.75 + m as f64 / 12.0;
            let mut expected = 0f64;
            let mut final_now = 0u64;
            for (k, &(gm, born, _)) in cohorts.iter().enumerate() {
                // Reported deaths due this month become final and leave the books.
                final_now += pending[m][k] as u64;
                on_books_pending[k] -= pending[m][k];
                let age = year - (born as f64 + 0.5);
                let q = to_f(gm.q_month(Fx((age * 18_446_744_073_709_551_616.0) as i128), Fx((year * 18_446_744_073_709_551_616.0) as i128)).unwrap());
                let (a, g) = alive[k];
                // The books count ghosts, and the dead whose reports aren't final yet, as alive.
                expected += q * (a + g + on_books_pending[k]) as f64;
                let mut died = 0;
                for _ in 0..a {
                    if rng.unit() < q * health {
                        died += 1;
                    }
                }
                let mut hid = 0;
                for _ in 0..died {
                    if rng.unit() < hidden {
                        hid += 1;
                    } else {
                        // Reported within a month, final after the 120-day window: 4 to 6 months
                        // by default.
                        let d = delay.0 + (rng.next() % delay.1.max(1) as u64) as usize;
                        pending[m + d][k] += 1;
                        on_books_pending[k] += 1;
                    }
                }
                alive[k] = (a - died, g + hid);
            }
            expected_hist.push(expected);
            if m < lag {
                continue; // warm-up: nothing to compare yet
            }
            let e = Fx((expected_hist[m - lag] * 18_446_744_073_709_551_616.0) as i128);
            if sprt.update_with(final_now, e, thresholds.0, thresholds.1).unwrap() == Signal::HiddenDeaths {
                flagged = Some(m + 1);
                break;
            }
        }
        if let Some(m) = flagged {
            months_to_flag.push(m);
            f12 += (m <= 12) as usize;
            f24 += (m <= 24) as usize;
            f36 += (m <= 36) as usize;
        }
    }
    months_to_flag.sort_unstable();
    // Median over all runs, counting unflagged runs as never: defined only if most runs flagged.
    let median = if months_to_flag.len() > runs / 2 { Some(months_to_flag[runs / 2]) } else { None };
    Outcome { hidden, runs, flagged_12m: f12 as f64 / runs as f64, flagged_24m: f24 as f64 / runs as f64, flagged_36m: f36 as f64 / runs as f64, median_months: median }
}
