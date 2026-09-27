//! Lifelong-income quote (the gasp test): Monte Carlo over market returns, computed on-chain.
//!
//! For a member with a pot (lump sum and/or monthly contributions until the start age), it
//! reports monthly income at the start and at 85 (P10/P50/P90) from the pool. It also reports
//! what happens if the same starting income is drawn from the same pot alone: the median age the
//! money runs out, and the probability of being alive when it does. Both use identical market
//! paths, so the difference is purely the pooling of longevity.
//!
//! The quote assumes a large pool, so survivors' credits equal their expected value, 1/(1−q).
//! The ledger settles the real, finite-pool credits month by month.

use alloc::vec::Vec;

use crate::fixed::{Fx, MathError, MathResult};
use crate::mortality::{month, Gm, MAX_AGE};

// Acklam's inverse normal CDF coefficients in Q64.64 (mpmath). Relative error 1.15e-9.
const A: [Fx; 6] = [Fx(-732277268835384088363), Fx(4075736131721770832428), Fx(-5089982614855123968030), Fx(2552250039309321074870), Fx(-565665681998200242698), Fx(46239130322213998667)];
const B: [Fx; 5] = [Fx(-1004906652664955705667), Fx(2980732578456464406132), Fx(-2872139234012985971771), Fx(1232266704180817165196), Fx(-244985333730518420514)];
const C: [Fx; 6] = [Fx(-143605947303788039), Fx(-5947164951755284849), Fx(-44286173521644288303), Fx(-47034263609683431318), Fx(80698309826038582002), Fx(54199559035435628852)];
const D: [Fx; 4] = [Fx(143602289436392765), Fx(5948468602138889755), Fx(45104763653767479495), Fx(69256615734324433706)];
const P_LOW: Fx = Fx(447333543787456627);

/// Capital-market assumptions, disclosed with every quote.
#[derive(Clone, Copy, Debug)]
pub struct Market {
    /// SPY expected annual return (arithmetic) and volatility.
    pub spy_return: Fx,
    pub spy_vol: Fx,
    /// SGOV and cash (Morpho) annual yield, treated as riskless.
    pub safe_rate: Fx,
    /// The pool's annual fee on value (0.30%), charged monthly on the member's pot. Money drawn
    /// alone pays no pool fee, so the comparison doesn't charge it.
    pub pool_fee: Fx,
}

#[derive(Clone, Copy, Debug)]
pub struct Plan {
    /// Annuitant's age now, and the current calendar year (fractional).
    pub age: Fx,
    pub year: Fx,
    /// Age income starts (≥ age).
    pub start_age: Fx,
    pub lump_sum: Fx,
    pub monthly_contribution: Fx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    pub income_start: [Fx; 3],
    pub income_at_85: [Fx; 3],
    /// Median age at which the same starting income, drawn alone, runs out (120 = never).
    pub solo_runout_age_p50: Fx,
    /// Probability of still being alive when the solo pot runs out.
    pub solo_outlive_probability: Fx,
    pub paths: u32,
}

/// Glide path (§8): SPY share 80% until 40, falling linearly to 30% at 65, then flat.
pub fn spy_share(age: Fx) -> MathResult<Fx> {
    let (young, old) = (Fx::from_int(40), Fx::from_int(65));
    let (hi, lo) = (Fx::from_ratio(8, 10)?, Fx::from_ratio(3, 10)?);
    if age <= young {
        return Ok(hi);
    }
    if age >= old {
        return Ok(lo);
    }
    let t = age.sub(young)?.div(old.sub(young)?)?;
    hi.sub(hi.sub(lo)?.mul(t)?)
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn horner(coef: &[Fx], x: Fx) -> MathResult<Fx> {
    let mut acc = Fx::ZERO;
    for &c in coef {
        acc = acc.mul(x)?.add(c)?;
    }
    Ok(acc)
}

/// Inverse standard normal CDF (Acklam), for u in (0, 1).
pub fn inv_norm(u: Fx) -> MathResult<Fx> {
    if u.0 <= 0 || u >= Fx::ONE {
        return Err(MathError::Domain);
    }
    let tail = |p: Fx| -> MathResult<Fx> {
        let q = p.ln()?.mul_int(-2)?.sqrt()?;
        let num = horner(&C, q)?;
        let mut den = horner(&D, q)?;
        den = den.mul(q)?.add(Fx::ONE)?;
        num.div(den)
    };
    if u < P_LOW {
        tail(u)
    } else if u <= Fx::ONE.sub(P_LOW)? {
        let q = u.sub(Fx::HALF)?;
        let r = q.mul(q)?;
        let num = horner(&A, r)?.mul(q)?;
        let den = horner(&B, r)?.mul(r)?.add(Fx::ONE)?;
        num.div(den)
    } else {
        tail(Fx::ONE.sub(u)?)?.neg()
    }
}

/// Monthly survival probabilities along the cohort's path from `start_age` in `start_year` to
/// 120, and the natural-tontine payout fraction 1/ä at each of those months (backward recursion).
fn payout_schedule(gm: &Gm, start_age: Fx, start_year: Fx, rate: Fx) -> MathResult<(Vec<Fx>, Vec<Fx>)> {
    let n = ((MAX_AGE as i128 * 12) - start_age.mul_int(12)?.floor_int()).max(1) as usize;
    let mut surv = Vec::with_capacity(n);
    let mut age = start_age;
    let mut year = start_year;
    for _ in 0..n {
        surv.push(gm.survival(age, year, month())?);
        age = age.add(month())?;
        year = year.add(month())?;
    }
    let v = Fx::ONE.div(Fx::ONE.add(rate)?.pow(month())?)?;
    let mut fraction = alloc::vec![Fx::ZERO; n];
    let mut next = Fx::ONE; // ä beyond the horizon: pay everything that's left
    for m in (0..n).rev() {
        let a = Fx::ONE.add(v.mul(surv[m])?.mul(next)?)?;
        fraction[m] = Fx::ONE.div(a)?;
        next = a;
    }
    Ok((surv, fraction))
}

fn percentiles(mut v: Vec<Fx>) -> [Fx; 3] {
    v.sort_unstable();
    let pick = |p: usize| v[(v.len() - 1) * p / 100];
    [pick(10), pick(50), pick(90)]
}

/// Number of points in the monthly SPY return table (a power of two: a draw is the top bits of
/// one random word).
pub const RETURN_NODES: usize = 256;

/// The monthly SPY growth factor at `RETURN_NODES` equiprobable quantiles of its lognormal:
/// e^(μ + σ·z_i) with z_i = Φ⁻¹((i + ½)/K), the z_i rescaled to exactly unit variance and the
/// factors scaled so their mean is the lognormal's mean e^(μ + σ²/2). Drawing a node uniformly
/// then matches the model's first two moments; over hundreds of monthly draws per path the
/// discreteness vanishes (central limit), and each draw costs one table lookup instead of an
/// inverse normal and an exponential. That is what makes a many-path quote fit in one call.
pub fn return_table(mu: Fx, sigma: Fx) -> MathResult<Vec<Fx>> {
    let k = RETURN_NODES as i128;
    let mut z = Vec::with_capacity(RETURN_NODES);
    let mut sq = Fx::ZERO;
    for i in 0..k {
        let zi = inv_norm(Fx::from_ratio(2 * i + 1, 2 * k)?)?;
        sq = sq.add(zi.mul(zi)?)?;
        z.push(zi);
    }
    let sd = sq.div_int(k)?.sqrt()?;
    let mut g = Vec::with_capacity(RETURN_NODES);
    let mut sum = Fx::ZERO;
    for zi in z {
        let gi = mu.add(sigma.mul(zi.div(sd)?)?)?.exp()?;
        sum = sum.add(gi)?;
        g.push(gi);
    }
    let scale = mu.add(sigma.mul(sigma)?.div_int(2)?)?.exp()?.div(sum.div_int(k)?)?;
    g.iter().map(|x| x.mul(scale)).collect()
}

pub fn quote(gm: &Gm, plan: &Plan, market: &Market, valuation_rate: Fx, paths: u32, seed: u64) -> MathResult<Quote> {
    if paths == 0 || plan.start_age < plan.age || plan.start_age >= Fx::from_int(MAX_AGE) {
        return Err(MathError::Domain);
    }
    let acc_months = plan.start_age.sub(plan.age)?.mul_int(12)?.floor_int() as usize;
    let start_year = plan.year.add(plan.start_age.sub(plan.age)?)?;
    let (surv, fraction) = payout_schedule(gm, plan.start_age, start_year, valuation_rate)?;
    let n = surv.len();
    let at_85 = Fx::from_int(85).sub(plan.start_age)?.mul_int(12)?.floor_int();

    // Everything that doesn't depend on the path, computed once.
    let sigma_m = market.spy_vol.div(Fx::from_int(12).sqrt()?)?;
    let mu_m = market.spy_return.sub(market.spy_vol.mul(market.spy_vol)?.div_int(2)?)?.div_int(12)?;
    let safe_g = Fx::ONE.add(market.safe_rate)?.pow(month())?;
    let table = return_table(mu_m, sigma_m)?;
    // Glide-path weight and riskless part of the growth factor for every month of the path.
    let (mut w, mut rest) = (Vec::with_capacity(acc_months + n), Vec::with_capacity(acc_months + n));
    let mut age = plan.age;
    for _ in 0..acc_months + n {
        let s = spy_share(age)?;
        rest.push(Fx::ONE.sub(s)?.mul(safe_g)?);
        w.push(s);
        age = age.add(month())?;
    }
    // What the pool's fee leaves each month; folded into the per-month factors below, so it costs
    // nothing per path.
    let keep = Fx::ONE.sub(market.pool_fee.div_int(12)?)?;
    // Saving months: expected mortality credit 1 + q/(1−q), less the fee.
    let mut acc_credit = Vec::with_capacity(acc_months);
    let (mut age, mut year) = (plan.age, plan.year);
    for _ in 0..acc_months {
        let q = gm.q_month(age, year)?;
        acc_credit.push(Fx::ONE.add(q.div(Fx::ONE.sub(q)?)?)?.mul(keep)?);
        age = age.add(month())?;
        year = year.add(month())?;
    }
    // Paying months: pay f_m, then survivors share the dead's money (÷ S_m), and the fee is
    // charged, so the pot moves by k_m = (1 − f_m)/S_m · keep before returns. And the chance of
    // being alive m months in.
    let mut k = Vec::with_capacity(n);
    let mut alive = Vec::with_capacity(n + 1);
    let mut p = Fx::ONE;
    for m in 0..n {
        k.push(Fx::ONE.sub(fraction[m])?.div(surv[m])?.mul(keep)?);
        alive.push(p);
        p = p.mul(surv[m])?;
    }
    alive.push(p);

    let mut rng = Rng(seed);
    let mut inc_start = Vec::with_capacity(paths as usize);
    let mut inc_85 = Vec::with_capacity(paths as usize);
    let mut runout_months = Vec::with_capacity(paths as usize);
    let mut outlive = Fx::ZERO;

    for _ in 0..paths {
        let mut j = 0usize; // month of the path, for the glide path
        let mut gross = |j: usize, rng: &mut Rng| -> MathResult<Fx> {
            let node = table[(rng.next() >> (64 - RETURN_NODES.trailing_zeros())) as usize];
            w[j].mul(node)?.add(rest[j])
        };
        // Saving phase: contribute, grow, earn expected mortality credits.
        let mut pot = plan.lump_sum;
        for credit in acc_credit.iter() {
            pot = pot.add(plan.monthly_contribution)?.mul(gross(j, &mut rng)?)?.mul(*credit)?;
            j += 1;
        }
        // Paying phase: pooled income vs the same income drawn alone, on the same returns.
        let income0 = pot.mul(fraction[0])?;
        let mut pooled = pot;
        let mut solo = pot;
        let mut runout: Option<usize> = None;
        let mut income85 = Fx::ZERO;
        for m in 0..n {
            if m as i128 == at_85 {
                income85 = pooled.mul(fraction[m])?;
            }
            let g = gross(j, &mut rng)?;
            pooled = pooled.mul(k[m])?.mul(g)?;
            if runout.is_none() {
                solo = solo.sub(income0)?;
                if solo.0 <= 0 {
                    runout = Some(m);
                } else {
                    solo = solo.mul(g)?;
                }
            }
            j += 1;
        }
        inc_start.push(income0);
        if at_85 >= 0 {
            inc_85.push(income85);
        }
        let ro = runout.unwrap_or(n);
        runout_months.push(Fx::from_int(ro as i64));
        // Probability of being alive at run-out.
        if runout.is_some() {
            outlive = outlive.add(alive[ro])?;
        }
    }
    let ro = percentiles(runout_months);
    Ok(Quote {
        income_start: percentiles(inc_start),
        income_at_85: if inc_85.is_empty() { [Fx::ZERO; 3] } else { percentiles(inc_85) },
        solo_runout_age_p50: plan.start_age.add(ro[1].div_int(12)?)?,
        solo_outlive_probability: outlive.div_int(paths as i128)?,
        paths,
    })
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

    #[test]
    fn inv_norm_matches_known_quantiles() {
        for &(u, z) in &[(0.5, 0.0), (0.975, 1.959963984540054), (0.01, -2.326347874040841), (0.999, 3.090232306167813)] {
            assert!((f(inv_norm(fx(u)).unwrap()) - z).abs() < 1e-8, "u={u}");
        }
    }

    #[test]
    fn return_table_matches_the_lognormal_mean_and_variance() {
        let (mu, sigma) = (fx((0.06 - 0.16 * 0.16 / 2.0) / 12.0), fx(0.16 / 12f64.sqrt()));
        let t = return_table(mu, sigma).unwrap();
        let g: Vec<f64> = t.iter().map(|&x| f(x)).collect();
        let mean = g.iter().sum::<f64>() / g.len() as f64;
        let want_mean = (f(mu) + f(sigma).powi(2) / 2.0).exp();
        assert!((mean - want_mean).abs() < 1e-15, "mean {mean} vs {want_mean}");
        // Log-growth has the model's σ (to the discretisation of 256 nodes).
        let lg: Vec<f64> = g.iter().map(|x| x.ln()).collect();
        let m = lg.iter().sum::<f64>() / lg.len() as f64;
        let sd = (lg.iter().map(|x| (x - m).powi(2)).sum::<f64>() / lg.len() as f64).sqrt();
        assert!((sd / f(sigma) - 1.0).abs() < 1e-9, "sd {sd} vs {}", f(sigma));
        assert!(g.windows(2).all(|w| w[0] < w[1]), "increasing in the quantile");
    }

    #[test]
    fn glide_path_endpoints() {
        assert_eq!(spy_share(Fx::from_int(30)).unwrap(), Fx::from_ratio(8, 10).unwrap());
        assert_eq!(spy_share(Fx::from_int(70)).unwrap(), Fx::from_ratio(3, 10).unwrap());
        assert!((f(spy_share(Fx::from_int(52)).unwrap()) - 0.56).abs() < 1e-12);
    }

    #[test]
    fn quote_is_deterministic_and_pooled_beats_solo() {
        let gm = Gm { a: fx(0.0005), b: fx(0.00003), theta: fx(0.1), kappa: fx(0.015) };
        let plan = Plan { age: fx(40.0), year: fx(2026.75), start_age: fx(60.0), lump_sum: Fx::ZERO, monthly_contribution: Fx::from_int(50) };
        let market = Market { spy_return: fx(0.06), spy_vol: fx(0.16), safe_rate: fx(0.035), pool_fee: fx(0.003) };
        let q1 = quote(&gm, &plan, &market, fx(0.035), 64, 42).unwrap();
        let q2 = quote(&gm, &plan, &market, fx(0.035), 64, 42).unwrap();
        assert_eq!(q1, q2);
        assert!(q1.income_start[0] <= q1.income_start[1] && q1.income_start[1] <= q1.income_start[2]);
        assert!(q1.income_start[1] > Fx::ZERO);
        // Drawing the pooled starting income alone runs dry well before 120.
        assert!(q1.solo_runout_age_p50 < Fx::from_int(100));
        assert!(q1.solo_outlive_probability > Fx::ZERO);
    }
}
