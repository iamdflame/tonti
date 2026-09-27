//! Gompertz–Makeham mortality with a calendar-time improvement trend (docs/protocol.md §2):
//!
//!   μ(x, y) = A + B·exp(θ·x − κ·(y − 2024))
//!
//! Age `x` and calendar year `y` both advance with time, so a person aged `x` in year `y`
//! survives `τ` more years with probability
//!
//!   S = exp( −A·τ − B·e^{θx − κ(y−2024)} · (e^{(θ−κ)τ} − 1)/(θ − κ) ).

use crate::fixed::{Fx, MathError, MathResult};

/// Parameters for one (country, sex), fitted by `actuarial/fit_mortality.py`. Units are years.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gm {
    pub a: Fx,
    pub b: Fx,
    pub theta: Fx,
    pub kappa: Fx,
}

const BASE_YEAR: i64 = 2024;
/// Nobody is modelled past 120.
pub const MAX_AGE: i64 = 120;

impl Gm {
    /// B·e^{θx − κ(y−2024)}: the Gompertz force at age `x` in year `y`, without Makeham's A.
    fn gompertz_now(&self, x: Fx, y: Fx) -> MathResult<Fx> {
        let t = y.sub(Fx::from_int(BASE_YEAR))?;
        self.b.mul(self.theta.mul(x)?.sub(self.kappa.mul(t)?)?.exp()?)
    }

    /// Integrated force over `tau` years for someone aged `x` in year `y`.
    pub fn cumulative_hazard(&self, x: Fx, y: Fx, tau: Fx) -> MathResult<Fx> {
        let g = self.gompertz_now(x, y)?;
        let d = self.theta.sub(self.kappa)?;
        // (e^{dτ} − 1)/d, which tends to τ as d → 0.
        let growth = if d.0.unsigned_abs() < (1u128 << 40) {
            tau
        } else {
            d.mul(tau)?.exp()?.sub(Fx::ONE)?.div(d)?
        };
        self.a.mul(tau)?.add(g.mul(growth)?)
    }

    /// Probability of surviving `tau` years from age `x` in year `y`.
    pub fn survival(&self, x: Fx, y: Fx, tau: Fx) -> MathResult<Fx> {
        if tau.is_negative() {
            return Err(MathError::Domain);
        }
        let h = self.cumulative_hazard(x, y, tau)?;
        h.neg()?.exp()
    }

    /// Probability of dying within one month, from age `x` in year `y`.
    pub fn q_month(&self, x: Fx, y: Fx) -> MathResult<Fx> {
        Ok(Fx::ONE.sub(self.survival(x, y, month())?)?)
    }

    /// Monthly annuity-due factor ä = Σ_{m≥0} S(x, y, m/12)·(1+r)^{−m/12}, summed to age 120.
    /// Its reciprocal is the natural-tontine payout fraction (§4).
    pub fn annuity_factor(&self, x: Fx, y: Fx, annual_rate: Fx) -> MathResult<Fx> {
        let months = (MAX_AGE * 12).saturating_sub(x.mul_int(12)?.floor_int() as i64).max(1);
        let v = Fx::ONE.add(annual_rate)?.pow(month())?; // (1+r)^{1/12}
        // Survival from one month to the next only needs e^{(θ−κ)/12} and one exp per step:
        // S(τ + 1/12) = S(τ)·exp(−A/12 − g·e^{dτ}·(e^{d/12} − 1)/d), with g the force today.
        let g = self.gompertz_now(x, y)?;
        let d = self.theta.sub(self.kappa)?;
        let step = if d.0.unsigned_abs() < (1u128 << 40) { month() } else { d.mul(month())?.exp()?.sub(Fx::ONE)?.div(d)? };
        let grow = d.mul(month())?.exp()?;
        let a_step = self.a.mul(month())?;
        let mut surv = Fx::ONE;
        let mut edt = Fx::ONE; // e^{dτ}
        let mut disc = Fx::ONE; // (1+r)^{-τ}
        let mut total = Fx::ZERO;
        for _ in 0..months {
            total = total.add(surv.mul(disc)?)?;
            let hazard = a_step.add(g.mul(edt)?.mul(step)?)?;
            surv = surv.mul(hazard.neg()?.exp()?)?;
            if surv.0 == 0 {
                break;
            }
            edt = edt.mul(grow)?;
            disc = disc.div(v)?;
        }
        Ok(total)
    }

    /// Fraction of a paying cohort's value paid out this month: 1/ä.
    pub fn payout_fraction(&self, x: Fx, y: Fx, annual_rate: Fx) -> MathResult<Fx> {
        Fx::ONE.div(self.annuity_factor(x, y, annual_rate)?)
    }
}

/// One month in years (1/12, truncated).
pub fn month() -> Fx {
    Fx(Fx::ONE.0 / 12)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn fx(v: f64) -> Fx {
        Fx((v * (1u128 << 64) as f64) as i128)
    }
    fn f(x: Fx) -> f64 {
        x.0 as f64 / (1u128 << 64) as f64
    }

    // Round numbers in the range fits produce; the fitted values get tested against WPP in
    // actuarial/fit_mortality.py and the float reference in tests/reference.py.
    fn sample() -> Gm {
        Gm { a: fx(0.0005), b: fx(0.00003), theta: fx(0.1), kappa: fx(0.015) }
    }

    fn survival_f64(p: (f64, f64, f64, f64), x: f64, y: f64, tau: f64) -> f64 {
        let (a, b, th, k) = p;
        let g = b * (th * x - k * (y - 2024.0)).exp();
        let d = th - k;
        (-(a * tau) - g * ((d * tau).exp() - 1.0) / d).exp()
    }

    #[test]
    fn survival_matches_closed_form_in_f64() {
        let m = sample();
        for &(x, y, tau) in &[(40.0, 2026.0, 1.0), (65.0, 2026.0, 20.0), (80.0, 2040.0, 5.5), (30.0, 2026.0, 70.0)] {
            let got = f(m.survival(fx(x), fx(y), fx(tau)).unwrap());
            let want = survival_f64((0.0005, 0.00003, 0.1, 0.015), x, y, tau);
            assert!((got - want).abs() < 1e-12, "S({x},{y},{tau}) {got} vs {want}");
        }
    }

    #[test]
    fn survival_is_monotone_and_bounded() {
        let m = sample();
        let mut prev = Fx::ONE;
        for t in 0..=80 {
            let s = m.survival(fx(40.0), fx(2026.0), Fx::from_int(t)).unwrap();
            assert!(s <= prev && s.0 >= 0);
            prev = s;
        }
        assert_eq!(m.survival(fx(40.0), fx(2026.0), Fx::ZERO).unwrap(), Fx::ONE);
    }

    #[test]
    fn annuity_factor_matches_direct_sum() {
        let m = sample();
        let (x, y, r) = (65.0, 2026.0, 0.035);
        let got = f(m.annuity_factor(fx(x), fx(y), fx(r)).unwrap());
        let p = (0.0005, 0.00003, 0.1, 0.015);
        let want: f64 = (0..(12 * 55)).map(|mm| {
            let tau = mm as f64 / 12.0;
            survival_f64(p, x, y, tau) * (1.0 + r).powf(-tau)
        }).sum();
        assert!((got - want).abs() / want < 1e-11, "ä {got} vs {want}");
        // Older people get a larger share of their balance each month.
        let young = m.payout_fraction(fx(60.0), fx(2026.0), fx(r)).unwrap();
        let old = m.payout_fraction(fx(80.0), fx(2026.0), fx(r)).unwrap();
        assert!(old > young);
    }
}
