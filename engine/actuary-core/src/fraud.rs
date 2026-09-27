//! Ghost-member detector (docs/protocol.md §6): Wald's SPRT on monthly deaths per group.
//!
//! Deaths in a group are Poisson with mean λ·E, where E = Σq over its members at the Actuary's
//! (UN) rates. The low test looks for deaths being hidden (heirs keeping the dead "alive" to
//! collect): H0 λ = 0.85 against H1 λ = 0.55. Its drift turns positive only below λ ≈ 0.69, so an
//! honest group up to ~30% healthier than the UN tables (annuitant selection) isn't flagged, while
//! a group hiding ~40% or more of its deaths is. It was H0 1.0 / H1 0.7, which flagged every honest
//! group healthier than 0.84 of the tables sooner or later (skeptic review 2, 2026-09-27).
//! The high test looks for mass false death reports (λ = 1.5 against 1).

use crate::fixed::{Fx, MathResult};

/// ln(0.55/0.85) and ln 1.5 in Q64.64 (60-digit decimal).
const LN_LOW: Fx = Fx(-8_030_201_051_154_334_440);
const LN_1_5: Fx = Fx(7_479_511_080_090_283_979);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Continue,
    /// Too few deaths: every member of the group must give a strong proof of life.
    HiddenDeaths,
    /// Too many deaths: freeze new death reports in the group for review.
    ExcessDeaths,
}

/// Running log-likelihood ratios for one group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sprt {
    pub low: Fx,
    pub high: Fx,
}

/// Wald thresholds for α = 0.1%, β = 5%: accept H1 above ln((1−β)/α) = ln 950, accept H0 below
/// ln(β/(1−α)) = ln(0.05/0.999) (Q64.64, mpmath at 50 digits). α was 1%: repeated tests on an
/// honest group then flagged 1.0% of groups within 24 months (skeptic review, 2026-09-27); the
/// power and false alarms at this setting are measured by `actuary-cli ghosts`.
const UPPER: Fx = Fx(126_479_399_480_935_019_852);
const LOWER: Fx = Fx(-55_243_050_590_003_248_203);

pub fn thresholds() -> MathResult<(Fx, Fx)> {
    Ok((UPPER, LOWER))
}

/// Wald thresholds for another α and β, for research (the contract uses `thresholds`).
pub fn thresholds_for(alpha: Fx, beta: Fx) -> MathResult<(Fx, Fx)> {
    let upper = Fx::ONE.sub(beta)?.div(alpha)?.ln()?;
    let lower = beta.div(Fx::ONE.sub(alpha)?)?.ln()?;
    Ok((upper, lower))
}

impl Sprt {
    /// Adds one month: `deaths` observed, `expected` = λ̂·Σq for the group.
    pub fn update(&mut self, deaths: u64, expected: Fx) -> MathResult<Signal> {
        self.update_with(deaths, expected, UPPER, LOWER)
    }

    /// The same test with the given thresholds.
    pub fn update_with(&mut self, deaths: u64, expected: Fx, upper: Fx, lower: Fx) -> MathResult<Signal> {
        let d = Fx::from_int(deaths as i64);
        // LLR increment for Poisson rates λ1 against λ0: d·ln(λ1/λ0) − (λ1 − λ0)·E.
        self.low = self.low.add(d.mul(LN_LOW)?.add(Fx::from_ratio(3, 10)?.mul(expected)?)?)?;
        self.high = self.high.add(d.mul(LN_1_5)?.sub(Fx::HALF.mul(expected)?)?)?;
        let mut signal = Signal::Continue;
        if self.low >= upper {
            signal = Signal::HiddenDeaths;
            self.low = Fx::ZERO;
        } else if self.low <= lower {
            self.low = Fx::ZERO; // accept H0 and restart
        }
        if self.high >= upper {
            signal = Signal::ExcessDeaths;
            self.high = Fx::ZERO;
        } else if self.high <= lower {
            self.high = Fx::ZERO;
        }
        Ok(signal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(deaths_per_month: u64, expected: Fx, months: usize) -> (Signal, usize) {
        let mut s = Sprt::default();
        for m in 1..=months {
            let sig = s.update(deaths_per_month, expected).unwrap();
            if sig != Signal::Continue {
                return (sig, m);
            }
        }
        (Signal::Continue, months)
    }

    #[test]
    fn detects_hidden_deaths_and_stays_quiet_on_normal_months() {
        let e = Fx::from_int(20); // 20 expected deaths a month
        // Half the deaths hidden: flagged within a few months.
        let (sig, m) = run(10, e, 24);
        assert_eq!(sig, Signal::HiddenDeaths);
        assert!(m <= 6, "took {m} months");
        // Exactly as expected: never flags.
        assert_eq!(run(20, e, 120).0, Signal::Continue);
        // An honest group 25% healthier than the tables: never flags either (skeptic review 2).
        assert_eq!(run(15, e, 240).0, Signal::Continue);
        // Twice the deaths: the excess test fires.
        assert_eq!(run(40, e, 24).0, Signal::ExcessDeaths);
    }

    #[test]
    fn the_low_test_constant_is_ln_of_055_over_085() {
        let want = Fx::from_ratio(55, 85).unwrap().ln().unwrap();
        assert!((want.0 - LN_LOW.0).abs() < 1 << 12);
    }

    #[test]
    fn the_constant_thresholds_are_walds_for_one_in_a_thousand() {
        let (u, l) = thresholds_for(Fx::from_ratio(1, 1000).unwrap(), Fx::from_ratio(5, 100).unwrap()).unwrap();
        let (cu, cl) = thresholds().unwrap();
        assert!((u.0 - cu.0).abs() < 1 << 12 && (l.0 - cl.0).abs() < 1 << 12, "ln 950 and ln(0.05/0.999)");
    }
}
