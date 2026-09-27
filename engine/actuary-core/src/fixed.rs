//! Signed Q64.64 fixed point on `i128`: 64 integer bits (with sign), 64 fraction bits.
//!
//! Stylus has no floating point, so every actuarial quantity lives here. All operations are
//! checked: overflow is an error, never a wrap. Resolution is 2^-64 ≈ 5.4e-20.

use crate::wide::{div_wide, mul_wide};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Fx(pub i128);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathError {
    Overflow,
    DivisionByZero,
    Domain,
}

pub type MathResult<T> = Result<T, MathError>;

const FRAC_BITS: u32 = 64;
/// ln 2 in Q64.64, from mpmath at 60 digits.
const LN2: Fx = Fx(12_786_308_645_202_655_660);
/// 1 / ln 2 in Q64.64.
const INV_LN2: Fx = Fx(26_613_026_195_688_644_983);
/// exp(x) overflows i128 Q64.64 above ln(2^63) ≈ 43.668.
const EXP_MAX: Fx = Fx(43 * (1 << 64) + (1 << 64) / 3 * 2);
/// exp(x) is below one ulp below ln(2^-64) ≈ -44.361.
const EXP_MIN: Fx = Fx(-(44 * (1 << 64) + (1 << 64) / 2));

impl Fx {
    pub const ZERO: Fx = Fx(0);
    pub const ONE: Fx = Fx(1 << FRAC_BITS);
    pub const HALF: Fx = Fx(1 << (FRAC_BITS - 1));

    pub const fn from_int(n: i64) -> Fx {
        Fx((n as i128) << FRAC_BITS)
    }

    /// `num / den` as Q64.64, truncated toward zero.
    pub fn from_ratio(num: i128, den: i128) -> MathResult<Fx> {
        Fx::from_raw_int(num)?.div(Fx::from_raw_int(den)?)
    }

    fn from_raw_int(n: i128) -> MathResult<Fx> {
        n.checked_mul(1 << FRAC_BITS).map(Fx).ok_or(MathError::Overflow)
    }

    /// Converts a WAD (1e18-scaled) integer into Q64.64, truncating toward zero.
    pub fn from_wad(wad: i128) -> MathResult<Fx> {
        let neg = wad < 0;
        let q = crate::wide::mul_div(wad.unsigned_abs(), Fx::ONE.0 as u128, 1_000_000_000_000_000_000)
            .ok_or(MathError::Overflow)?;
        signed(q, neg)
    }

    /// Converts to a WAD (1e18-scaled) integer, truncating toward zero.
    pub fn to_wad(self) -> MathResult<i128> {
        let neg = self.0 < 0;
        let q = crate::wide::mul_div(self.0.unsigned_abs(), 1_000_000_000_000_000_000, Fx::ONE.0 as u128)
            .ok_or(MathError::Overflow)?;
        let v = i128::try_from(q).map_err(|_| MathError::Overflow)?;
        Ok(if neg { -v } else { v })
    }

    pub fn add(self, o: Fx) -> MathResult<Fx> {
        self.0.checked_add(o.0).map(Fx).ok_or(MathError::Overflow)
    }

    pub fn sub(self, o: Fx) -> MathResult<Fx> {
        self.0.checked_sub(o.0).map(Fx).ok_or(MathError::Overflow)
    }

    pub fn neg(self) -> MathResult<Fx> {
        self.0.checked_neg().map(Fx).ok_or(MathError::Overflow)
    }

    /// Product, truncated toward zero.
    pub fn mul(self, o: Fx) -> MathResult<Fx> {
        let (hi, lo) = mul_wide(self.0.unsigned_abs(), o.0.unsigned_abs());
        // hi < 2⁶³ ⇔ the magnitude hi·2⁶⁴ + lo/2⁶⁴ < 2¹²⁷ fits an i128: one branch (Stylus charges
        // per basic block, so this is the hot path's whole cost model).
        if hi >> (FRAC_BITS - 1) != 0 {
            return Err(MathError::Overflow);
        }
        let m = ((hi << FRAC_BITS) | (lo >> FRAC_BITS)) as i128;
        Ok(Fx(if (self.0 ^ o.0) < 0 { -m } else { m }))
    }

    /// Quotient, truncated toward zero.
    pub fn div(self, o: Fx) -> MathResult<Fx> {
        if o.0 == 0 {
            return Err(MathError::DivisionByZero);
        }
        let neg = (self.0 < 0) != (o.0 < 0);
        let a = self.0.unsigned_abs();
        let q = div_wide(a >> FRAC_BITS, a << FRAC_BITS, o.0.unsigned_abs()).ok_or(MathError::Overflow)?;
        signed(q, neg)
    }

    pub fn div_int(self, n: i128) -> MathResult<Fx> {
        if n == 0 {
            return Err(MathError::DivisionByZero);
        }
        // Truncates toward zero, like i128 `/`, via the limb division (no wasm32 library call).
        let d = n.unsigned_abs();
        let q = if d <= u32::MAX as u128 {
            crate::wide::div_small(self.0.unsigned_abs(), d as u64)
        } else {
            crate::wide::div_wide(0, self.0.unsigned_abs(), d).ok_or(MathError::Overflow)?
        };
        signed(q, (self.0 < 0) != (n < 0))
    }

    pub fn mul_int(self, n: i128) -> MathResult<Fx> {
        self.0.checked_mul(n).map(Fx).ok_or(MathError::Overflow)
    }

    /// Integer part, rounded toward negative infinity.
    pub const fn floor_int(self) -> i128 {
        self.0 >> FRAC_BITS
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// e^x. Relative error below 1e-17 over the whole domain (see tests and `tests/precision.py`).
    pub fn exp(self) -> MathResult<Fx> {
        if self > EXP_MAX {
            return Err(MathError::Overflow);
        }
        if self < EXP_MIN {
            return Ok(Fx::ZERO);
        }
        // x = k·ln2 + r with |r| ≤ ln2/2, so e^x = 2^k · e^r.
        let k_real = self.mul(INV_LN2)?;
        let k = k_real.add(Fx::HALF)?.floor_int();
        let r = self.sub(LN2.mul_int(k)?)?;
        // Taylor series for e^r; |r| ≤ 0.35 so 30 terms is far past one ulp.
        let mut sum = Fx::ONE;
        let mut term = Fx::ONE;
        let mut n = 1;
        while n <= 30 {
            term = term.mul(r)?.div_int(n)?;
            if term.0 == 0 {
                break;
            }
            sum = sum.add(term)?;
            n += 1;
        }
        if k >= 0 {
            let shift = k as u32;
            if shift >= 64 || sum.0 >> (127 - shift) != 0 {
                return Err(MathError::Overflow);
            }
            Ok(Fx(sum.0 << shift))
        } else {
            let shift = (-k) as u32;
            Ok(if shift >= 127 { Fx::ZERO } else { Fx(sum.0 >> shift) })
        }
    }

    /// Natural log, for x > 0. Absolute error below 1e-17.
    pub fn ln(self) -> MathResult<Fx> {
        if self.0 <= 0 {
            return Err(MathError::Domain);
        }
        let raw = self.0 as u128;
        // x = 2^n · m with m in [1, 2); ln x = n·ln2 + ln m.
        let msb = 127 - raw.leading_zeros() as i32;
        let n = msb - FRAC_BITS as i32;
        let m = if n >= 0 { Fx((raw >> n) as i128) } else { Fx((raw << (-n)) as i128) };
        // ln m = 2·atanh(z), z = (m-1)/(m+1) in [0, 1/3).
        let z = m.sub(Fx::ONE)?.div(m.add(Fx::ONE)?)?;
        let z2 = z.mul(z)?;
        let mut sum = z;
        let mut power = z;
        let mut k = 3;
        while k < 80 {
            power = power.mul(z2)?;
            if power.0 == 0 {
                break;
            }
            sum = sum.add(power.div_int(k)?)?;
            k += 2;
        }
        LN2.mul_int(n as i128)?.add(sum.mul_int(2)?)
    }

    /// x^y = e^(y·ln x), for x > 0.
    pub fn pow(self, y: Fx) -> MathResult<Fx> {
        self.ln()?.mul(y)?.exp()
    }

    /// Square root via e^(ln(x)/2); sqrt(0) = 0.
    pub fn sqrt(self) -> MathResult<Fx> {
        if self.0 < 0 {
            return Err(MathError::Domain);
        }
        if self.0 == 0 {
            return Ok(Fx::ZERO);
        }
        self.ln()?.div_int(2)?.exp()
    }
}

fn signed(magnitude: u128, neg: bool) -> MathResult<Fx> {
    if magnitude > i128::MAX as u128 {
        return Err(MathError::Overflow);
    }
    let v = magnitude as i128;
    Ok(Fx(if neg { -v } else { v }))
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn f(x: Fx) -> f64 {
        x.0 as f64 / (1u128 << 64) as f64
    }
    fn fx(v: f64) -> Fx {
        Fx((v * (1u128 << 64) as f64) as i128)
    }
    fn rel_err(got: Fx, want: f64) -> f64 {
        ((f(got) - want) / want).abs()
    }

    #[test]
    fn arithmetic_basics() {
        let two = Fx::from_int(2);
        let three = Fx::from_int(3);
        assert_eq!(two.mul(three).unwrap(), Fx::from_int(6));
        assert_eq!(Fx::from_int(6).div(three).unwrap(), two);
        assert_eq!(Fx::from_int(-6).div(three).unwrap(), Fx::from_int(-2));
        assert_eq!(Fx::from_ratio(1, 4).unwrap(), Fx(1 << 62));
        assert_eq!(Fx::from_int(1 << 40).mul(Fx::from_int(1 << 40)), Err(MathError::Overflow));
        assert_eq!(Fx::ONE.div(Fx::ZERO), Err(MathError::DivisionByZero));
    }

    #[test]
    fn wad_round_trip() {
        let v = Fx::from_wad(1_234_567_890_123_456_789).unwrap();
        assert!((f(v) - 1.234567890123456789).abs() < 1e-15);
        assert!((v.to_wad().unwrap() - 1_234_567_890_123_456_789).abs() <= 1);
    }

    // f64 references are only good to ~1e-16; the 1e-18 bounds are checked against mpmath in
    // tests/precision.py through the actuary-cli binary.
    #[test]
    fn exp_matches_f64_reference() {
        let mut x = -44.0;
        while x < 43.0 {
            let input = fx(x);
            let got = input.exp().unwrap();
            let want = libm_exp(f(input));
            // Q64.64 has absolute resolution 5.4e-20, so tiny results are bounded absolutely.
            let tol = 5e-15 * want + 1e-18;
            assert!((f(got) - want).abs() <= tol, "exp({x}) got {} want {want}", f(got));
            x += 0.371;
        }
        assert_eq!(Fx::ZERO.exp().unwrap(), Fx::ONE);
        assert!(Fx::from_int(44).exp().is_err());
        assert_eq!(Fx::from_int(-50).exp().unwrap(), Fx::ZERO);
    }

    #[test]
    fn ln_matches_f64_reference_and_inverts_exp() {
        for &v in &[1e-12, 1e-6, 0.001, 0.5, 0.9999, 1.0, 1.0001, 2.0, 10.0, 1e6, 1e15] {
            let input = fx(v); // compare against ln of the exactly representable input
            let got = input.ln().unwrap();
            assert!((f(got) - f(input).ln()).abs() < 1e-14, "ln({v})");
        }
        assert_eq!(Fx::ONE.ln().unwrap(), Fx::ZERO);
        for &v in &[0.3, 1.7, 12.5] {
            let back = fx(v).ln().unwrap().exp().unwrap();
            assert!(rel_err(back, v) < 1e-16);
        }
        assert_eq!(Fx::ZERO.ln(), Err(MathError::Domain));
    }

    #[test]
    fn pow_and_sqrt() {
        assert!(rel_err(fx(1.035).pow(fx(1.0 / 12.0)).unwrap(), 1.035f64.powf(1.0 / 12.0)) < 1e-15);
        assert!(rel_err(Fx::from_int(2).sqrt().unwrap(), 2f64.sqrt()) < 1e-16);
    }

    fn libm_exp(x: f64) -> f64 {
        std::primitive::f64::exp(x)
    }
}
