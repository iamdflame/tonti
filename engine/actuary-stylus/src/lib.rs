//! Tonti actuarial engine on Robinhood Chain (Stylus).
//!
//! Stores the fitted mortality parameters per (country, sex) and the disclosed market
//! assumptions, and answers the gasp-test quote on-chain with the exact `actuary-core` code the
//! tests, simulator and backtest run. The pool contract calls `q_month` and `payout_fraction`
//! from here at settlement.
//!
//! Units at the ABI: ages and years are WAD (1e18); USDG amounts use 6 decimals; probabilities
//! and rates are WAD. Internally everything is Q64.64 fixed point (Stylus has no floats).

#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
extern crate alloc;

use actuary_core::mortality::Gm;
use actuary_core::quote::{quote, Market, Plan};
use actuary_core::{Fx, MathError};
use alloc::vec::Vec;
use alloy_primitives::{Address, I256, U256};
use alloy_sol_types::sol;
use stylus_sdk::prelude::*;

const USDG_SCALE: i128 = 1_000_000;
/// Measured with ink-meter: 512 paths cost 16.9M gas before the limb arithmetic, under the 32M
/// per-transaction cap that also bounds an eth_call; 1,024 paths did not fit.
const MAX_PATHS: u32 = 512;
/// A quote is for someone the pool could take (docs/protocol.md §4): an adult, income from 50 to
/// 80, and contributions a pension could plausibly hold. Anything else is refused, not extrapolated.
const MIN_AGE: u128 = 18;
const MIN_START: u128 = 50;
const MAX_START: u128 = 80;
const MIN_YEAR: u128 = 2020;
const MAX_YEAR: u128 = 2100;
const MAX_LUMP_USDG: u128 = 10_000_000;
const MAX_MONTHLY_USDG: u128 = 100_000;
const WAD_U: u128 = 1_000_000_000_000_000_000;

/// Only the deployer may initialise: the address is compiled in (`TONTI_DEPLOYER=0x…` at build
/// time), so nobody can front-run `init` between deployment and set-up. Built without it, the
/// contract can't be initialised at all.
#[cfg(not(test))]
const DEPLOYER: Address = match option_env!("TONTI_DEPLOYER") {
    Some(s) => Address::new(parse_address(s)),
    None => Address::ZERO,
};
#[cfg(test)]
const DEPLOYER: Address = Address::new({
    let mut a = [0u8; 20];
    a[19] = 0xaa; // the tests' OWNER
    a
});

#[allow(dead_code)]
const fn hex_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("TONTI_DEPLOYER: not hex"),
    }
}

#[allow(dead_code)]
const fn parse_address(s: &str) -> [u8; 20] {
    let b = s.as_bytes();
    assert!(b.len() == 42 && b[0] == b'0' && b[1] == b'x', "TONTI_DEPLOYER: expected 0x + 40 hex digits");
    let mut out = [0u8; 20];
    let mut i = 0;
    while i < 20 {
        out[i] = hex_nibble(b[2 + 2 * i]) << 4 | hex_nibble(b[3 + 2 * i]);
        i += 1;
    }
    out
}

sol! {
    error Unauthorized();
    error UnknownMortality(uint256 key);
    error Math(uint8 kind);
    error BadInput();
    event MortalitySet(uint256 indexed key, int256 a, int256 b, int256 theta, int256 kappa);
    event MarketSet(int256 spyReturn, int256 spyVol, int256 safeRate, int256 valuationRate, int256 poolFee);
    event EscalationSet(int256 escalation);
    /// Mortality can never change again: new tables mean a new Actuary and a new pool.
    event Sealed();
}

#[derive(SolidityError)]
pub enum ActuaryError {
    Unauthorized(Unauthorized),
    UnknownMortality(UnknownMortality),
    Math(Math),
    BadInput(BadInput),
}

impl core::fmt::Debug for ActuaryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ActuaryError::Unauthorized(_) => f.write_str("Unauthorized"),
            ActuaryError::UnknownMortality(e) => write!(f, "UnknownMortality({})", e.key),
            ActuaryError::Math(e) => write!(f, "Math({})", e.kind),
            ActuaryError::BadInput(_) => f.write_str("BadInput"),
        }
    }
}

impl From<MathError> for ActuaryError {
    fn from(e: MathError) -> Self {
        let kind = match e {
            MathError::Overflow => 1,
            MathError::DivisionByZero => 2,
            MathError::Domain => 3,
        };
        ActuaryError::Math(Math { kind })
    }
}

sol_storage! {
    #[entrypoint]
    pub struct Actuary {
        address owner;
        /// key = countryCode * 2 + sex (ISO 3166-1 numeric; sex 0 = female, 1 = male).
        /// Values are raw Q64.64: [A, B, θ, κ]; a zero θ means "not set".
        mapping(uint256 => int256) gm_a;
        mapping(uint256 => int256) gm_b;
        mapping(uint256 => int256) gm_theta;
        mapping(uint256 => int256) gm_kappa;
        /// Raw Q64.64 market assumptions and valuation rate.
        int256 spy_return;
        int256 spy_vol;
        int256 safe_rate;
        int256 valuation_rate;
        /// Escalating plan: priced this much below the valuation rate, so its income starts lower
        /// and grows by about this much a year when returns meet the level plan's assumption.
        int256 escalation;
        /// The pool's annual fee on value, which quotes deduct (raw Q64.64).
        int256 pool_fee;
        /// Once sealed, no mortality parameter can ever be set again.
        bool sealed;
    }
}

fn to_i128(v: I256) -> Result<i128, ActuaryError> {
    i128::try_from(v).map_err(|_| ActuaryError::BadInput(BadInput {}))
}

fn wad(v: U256) -> Result<Fx, ActuaryError> {
    let x = i128::try_from(v).map_err(|_| ActuaryError::BadInput(BadInput {}))?;
    Ok(Fx::from_wad(x)?)
}

fn usdg_in(v: U256) -> Result<Fx, ActuaryError> {
    let x = i128::try_from(v).map_err(|_| ActuaryError::BadInput(BadInput {}))?;
    Ok(Fx::from_ratio(x, USDG_SCALE)?)
}

fn usdg_out(v: Fx) -> Result<U256, ActuaryError> {
    // WAD → 6 decimals, truncating (pool-favoring).
    let w = v.to_wad()? / 1_000_000_000_000;
    Ok(U256::from(w.max(0) as u128))
}

fn wad_out(v: Fx) -> Result<U256, ActuaryError> {
    Ok(U256::from(v.to_wad()?.max(0) as u128))
}

impl Actuary {
    fn only_owner(&self) -> Result<(), ActuaryError> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(ActuaryError::Unauthorized(Unauthorized {}));
        }
        Ok(())
    }

    fn only_unsealed_owner(&self) -> Result<(), ActuaryError> {
        self.only_owner()?;
        if self.sealed.get() {
            return Err(ActuaryError::Unauthorized(Unauthorized {}));
        }
        Ok(())
    }

    fn gm(&self, key: U256) -> Result<Gm, ActuaryError> {
        let theta = to_i128(self.gm_theta.get(key))?;
        if theta == 0 {
            return Err(ActuaryError::UnknownMortality(UnknownMortality { key }));
        }
        Ok(Gm {
            a: Fx(to_i128(self.gm_a.get(key))?),
            b: Fx(to_i128(self.gm_b.get(key))?),
            theta: Fx(theta),
            kappa: Fx(to_i128(self.gm_kappa.get(key))?),
        })
    }

    /// The rate a plan is priced at: level plans at the valuation rate, escalating plans below it.
    fn rate(&self, escalating: bool) -> Result<Fx, ActuaryError> {
        let r = Fx(to_i128(self.valuation_rate.get())?);
        if !escalating {
            return Ok(r);
        }
        Ok(r.sub(Fx(to_i128(self.escalation.get())?))?)
    }
}

/// Escalation bounds: 0 to 4% a year (CPF LIFE's Escalating Plan uses 2%).
const MAX_ESCALATION_PCT: i128 = 4;

#[public]
impl Actuary {
    /// One-time setup by the compiled-in deployer, who becomes owner (later the timelock).
    pub fn init(&mut self) -> Result<(), ActuaryError> {
        let sender = self.vm().msg_sender();
        if self.owner.get() != Address::ZERO || sender != DEPLOYER {
            return Err(ActuaryError::Unauthorized(Unauthorized {}));
        }
        self.owner.set(sender);
        self.escalation.set(I256::try_from(Fx::from_ratio(2, 100)?.0).map_err(|_| ActuaryError::BadInput(BadInput {}))?);
        Ok(())
    }

    pub fn owner(&self) -> Address {
        self.owner.get()
    }

    pub fn transfer_ownership(&mut self, to: Address) -> Result<(), ActuaryError> {
        self.only_owner()?;
        self.owner.set(to);
        Ok(())
    }

    /// Freezes the mortality tables for good. The deployment seals them right after loading, so no
    /// owner, the timelock included, can ever steer mortality credits by changing them.
    pub fn seal(&mut self) -> Result<(), ActuaryError> {
        self.only_owner()?;
        self.sealed.set(true);
        self.vm().log(Sealed {});
        Ok(())
    }

    /// Whether the mortality tables are frozen for good (`sealed` is reserved in Solidity).
    pub fn is_sealed(&self) -> bool {
        self.sealed.get()
    }

    /// Sets one (country, sex)'s fitted Gompertz–Makeham parameters, raw Q64.64 (before sealing).
    pub fn set_mortality(&mut self, key: U256, a: I256, b: I256, theta: I256, kappa: I256) -> Result<(), ActuaryError> {
        self.only_unsealed_owner()?;
        if to_i128(theta)? <= 0 || to_i128(b)? <= 0 || to_i128(a)? < 0 {
            return Err(ActuaryError::BadInput(BadInput {}));
        }
        to_i128(kappa)?;
        self.gm_a.insert(key, a);
        self.gm_b.insert(key, b);
        self.gm_theta.insert(key, theta);
        self.gm_kappa.insert(key, kappa);
        self.vm().log(MortalitySet { key, a, b, theta, kappa });
        Ok(())
    }

    /// Sets many cohorts at once (the 1,704 fitted cohorts load in a handful of transactions).
    pub fn set_mortality_batch(&mut self, keys: Vec<U256>, a: Vec<I256>, b: Vec<I256>, theta: Vec<I256>, kappa: Vec<I256>) -> Result<(), ActuaryError> {
        self.only_unsealed_owner()?;
        let n = keys.len();
        if a.len() != n || b.len() != n || theta.len() != n || kappa.len() != n {
            return Err(ActuaryError::BadInput(BadInput {}));
        }
        for i in 0..n {
            if to_i128(theta[i])? <= 0 || to_i128(b[i])? <= 0 || to_i128(a[i])? < 0 {
                return Err(ActuaryError::BadInput(BadInput {}));
            }
            to_i128(kappa[i])?;
            self.gm_a.insert(keys[i], a[i]);
            self.gm_b.insert(keys[i], b[i]);
            self.gm_theta.insert(keys[i], theta[i]);
            self.gm_kappa.insert(keys[i], kappa[i]);
            self.vm().log(MortalitySet { key: keys[i], a: a[i], b: b[i], theta: theta[i], kappa: kappa[i] });
        }
        Ok(())
    }

    /// Sets the disclosed market assumptions, the valuation rate and the pool fee quotes deduct, raw
    /// Q64.64, within bounds (docs/protocol.md §4, §9): SPY return −5%…15%, volatility 1%…50%, safe
    /// rate 0%…10%, valuation rate 2%…5% and at least the escalation (so no plan is priced below
    /// 0%), pool fee 0%…1%.
    pub fn set_market(&mut self, spy_return: I256, spy_vol: I256, safe_rate: I256, valuation_rate: I256, pool_fee: I256) -> Result<(), ActuaryError> {
        self.only_owner()?;
        let within = |v: I256, lo: i128, hi: i128| -> Result<(), ActuaryError> {
            let x = Fx(to_i128(v)?);
            if x < Fx::from_ratio(lo, 100)? || x > Fx::from_ratio(hi, 100)? {
                return Err(ActuaryError::BadInput(BadInput {}));
            }
            Ok(())
        };
        within(spy_return, -5, 15)?;
        within(spy_vol, 1, 50)?;
        within(safe_rate, 0, 10)?;
        // Every member's payout fraction moves with this rate (about ±15% across the band), so the
        // band is narrow and disclosed; changes still wait out the 48-hour timelock.
        within(valuation_rate, 2, 5)?;
        within(pool_fee, 0, 1)?;
        if to_i128(valuation_rate)? < to_i128(self.escalation.get())? {
            return Err(ActuaryError::BadInput(BadInput {}));
        }
        self.spy_return.set(spy_return);
        self.spy_vol.set(spy_vol);
        self.safe_rate.set(safe_rate);
        self.valuation_rate.set(valuation_rate);
        self.pool_fee.set(pool_fee);
        self.vm().log(MarketSet { spyReturn: spy_return, spyVol: spy_vol, safeRate: safe_rate, valuationRate: valuation_rate, poolFee: pool_fee });
        Ok(())
    }

    /// Sets the escalating plan's yearly escalation, raw Q64.64, within 0–4%.
    pub fn set_escalation(&mut self, escalation: I256) -> Result<(), ActuaryError> {
        self.only_owner()?;
        let e = to_i128(escalation)?;
        // Within 0–4%, and never above the valuation rate (a zero rate allows only zero), so no
        // plan is ever priced below 0%.
        let r = to_i128(self.valuation_rate.get())?;
        if e < 0 || Fx(e) > Fx::from_ratio(MAX_ESCALATION_PCT, 100)? || e > r {
            return Err(ActuaryError::BadInput(BadInput {}));
        }
        self.escalation.set(escalation);
        self.vm().log(EscalationSet { escalation });
        Ok(())
    }

    pub fn escalation(&self) -> I256 {
        self.escalation.get()
    }

    /// Every assumption a quote uses, raw Q64.64: SPY return, SPY volatility, safe rate, valuation
    /// rate, pool fee, escalation.
    pub fn market(&self) -> (I256, I256, I256, I256, I256, I256) {
        (self.spy_return.get(), self.spy_vol.get(), self.safe_rate.get(), self.valuation_rate.get(), self.pool_fee.get(), self.escalation.get())
    }

    /// One-month death probability (WAD) for someone aged `age` (WAD years) in `year` (WAD).
    pub fn q_month(&self, key: U256, age: U256, year: U256) -> Result<U256, ActuaryError> {
        wad_out(self.gm(key)?.q_month(wad(age)?, wad(year)?)?)
    }

    /// Fraction of a paying cohort's value paid out this month (WAD), for the level or the
    /// escalating plan.
    pub fn payout_fraction(&self, key: U256, age: U256, year: U256, escalating: bool) -> Result<U256, ActuaryError> {
        wad_out(self.gm(key)?.payout_fraction(wad(age)?, wad(year)?, self.rate(escalating)?)?)
    }

    /// The gasp test. Returns monthly income at the start and at 85 (P10, P50, P90; USDG, 6dp),
    /// the median age the same starting income runs out if drawn alone (WAD), and the
    /// probability of being alive when it does (WAD).
    #[allow(clippy::too_many_arguments)]
    pub fn quote(
        &self,
        key: U256,
        age: U256,
        year: U256,
        start_age: U256,
        lump_sum: U256,
        monthly_contribution: U256,
        paths: u32,
        escalating: bool,
    ) -> Result<(U256, U256, U256, U256, U256, U256, U256, U256), ActuaryError> {
        let wad_in = |v: U256, lo: u128, hi: u128| v >= U256::from(lo * WAD_U) && v <= U256::from(hi * WAD_U);
        let usdg_max = |v: U256, hi: u128| v <= U256::from(hi * USDG_SCALE as u128);
        if paths == 0
            || paths > MAX_PATHS
            || !wad_in(start_age, MIN_START, MAX_START)
            || !wad_in(age, MIN_AGE, MAX_START)
            || age > start_age
            || !wad_in(year, MIN_YEAR, MAX_YEAR)
            || !usdg_max(lump_sum, MAX_LUMP_USDG)
            || !usdg_max(monthly_contribution, MAX_MONTHLY_USDG)
        {
            return Err(ActuaryError::BadInput(BadInput {}));
        }
        let gm = self.gm(key)?;
        let plan = Plan {
            age: wad(age)?,
            year: wad(year)?,
            start_age: wad(start_age)?,
            lump_sum: usdg_in(lump_sum)?,
            monthly_contribution: usdg_in(monthly_contribution)?,
        };
        let market = Market {
            spy_return: Fx(to_i128(self.spy_return.get())?),
            spy_vol: Fx(to_i128(self.spy_vol.get())?),
            safe_rate: Fx(to_i128(self.safe_rate.get())?),
            pool_fee: Fx(to_i128(self.pool_fee.get())?),
        };
        // Same inputs → same quote: the seed is a mix of the inputs.
        let mut seed: u64 = 0x5443_4f4e_5449_0001;
        for limb in [key, age, year, start_age, lump_sum, monthly_contribution] {
            for w in limb.as_limbs() {
                seed = (seed ^ w).wrapping_mul(0x100_0000_01B3).rotate_left(29);
            }
        }
        if escalating {
            seed = seed.wrapping_mul(0x100_0000_01B3).rotate_left(29) ^ 1;
        }
        let q = quote(&gm, &plan, &market, self.rate(escalating)?, paths, seed)?;
        let out: Vec<U256> = alloc::vec![
            usdg_out(q.income_start[0])?, usdg_out(q.income_start[1])?, usdg_out(q.income_start[2])?,
            usdg_out(q.income_at_85[0])?, usdg_out(q.income_at_85[1])?, usdg_out(q.income_at_85[2])?,
            wad_out(q.solo_runout_age_p50)?, wad_out(q.solo_outlive_probability)?,
        ];
        Ok((out[0], out[1], out[2], out[3], out[4], out[5], out[6], out[7]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;
    use stylus_sdk::testing::*;

    const OWNER: Address = address!("0x00000000000000000000000000000000000000aa");
    const OTHER: Address = address!("0x00000000000000000000000000000000000000bb");
    const WAD: u128 = 1_000_000_000_000_000_000;
    // Philippines, female, born 1966 (config/mortality.json, raw Q64.64).
    const KEY: u64 = 1216 * 10_000 + 1966;
    const B: i128 = 638_055_331_335_216;
    const THETA: i128 = 1_762_436_495_661_328_128;

    fn i(v: Fx) -> I256 {
        I256::try_from(v.0).unwrap()
    }
    fn pct(n: i128, d: i128) -> Fx {
        Fx::from_ratio(n, d).unwrap()
    }

    fn setup() -> (TestVM, Actuary) {
        let vm = TestVM::new();
        vm.set_sender(OWNER);
        let mut a = Actuary::from(&vm);
        a.init().unwrap();
        a.set_mortality(U256::from(KEY), I256::ZERO, I256::try_from(B).unwrap(), I256::try_from(THETA).unwrap(), I256::ZERO).unwrap();
        a.set_market(i(pct(6, 100)), i(pct(16, 100)), i(pct(35, 1000)), i(pct(35, 1000)), i(pct(3, 1000))).unwrap();
        (vm, a)
    }

    #[test]
    fn init_sets_the_owner_and_a_two_percent_escalation_once() {
        let (_vm, mut a) = setup();
        assert_eq!(a.owner(), OWNER);
        assert_eq!(a.escalation(), i(pct(2, 100)));
        assert!(a.init().is_err(), "init runs once");
    }

    #[test]
    fn escalation_is_owner_only_and_bounded_to_four_percent() {
        let (vm, mut a) = setup();
        assert!(a.set_escalation(i(pct(41, 1000))).is_err());
        assert!(a.set_escalation(I256::try_from(-1i128).unwrap()).is_err());
        // 4% is inside the band but above the 3.5% valuation rate, so it's refused.
        assert!(a.set_escalation(i(pct(4, 100))).is_err());
        a.set_escalation(i(pct(35, 1000))).unwrap();
        a.set_escalation(I256::ZERO).unwrap();
        vm.set_sender(OTHER);
        assert!(a.set_escalation(i(pct(1, 100))).is_err());
        assert!(a.set_market(I256::ZERO, I256::ZERO, I256::ZERO, I256::ZERO, I256::ZERO).is_err());
    }

    #[test]
    fn market_assumptions_are_bounded() {
        let (_vm, mut a) = setup();
        let ok = (i(pct(6, 100)), i(pct(16, 100)), i(pct(35, 1000)), i(pct(35, 1000)), i(pct(3, 1000)));
        a.set_market(ok.0, ok.1, ok.2, ok.3, ok.4).unwrap();
        assert!(a.set_market(ok.0, ok.1, ok.2, i(pct(6, 100)), ok.4).is_err(), "valuation rate above 5%");
        assert!(a.set_market(ok.0, ok.1, ok.2, i(pct(1, 100)), ok.4).is_err(), "valuation rate below 2%");
        assert!(a.set_market(ok.0, I256::ZERO, ok.2, ok.3, ok.4).is_err(), "zero volatility");
        assert!(a.set_market(i(pct(30, 100)), ok.1, ok.2, ok.3, ok.4).is_err(), "30% expected return");
        assert!(a.set_market(ok.0, ok.1, i(pct(-1, 100)), ok.3, ok.4).is_err(), "negative safe rate");
        assert!(a.set_market(ok.0, ok.1, ok.2, ok.3, i(pct(2, 100))).is_err(), "a 2% fee");
        assert_eq!(a.market(), (ok.0, ok.1, ok.2, ok.3, ok.4, i(pct(2, 100))));
    }

    #[test]
    fn each_plan_is_priced_exactly_as_the_core_prices_it() {
        let (_vm, a) = setup();
        let (age, year) = (U256::from(62 * WAD), U256::from(2028 * WAD));
        let gm = Gm { a: Fx::ZERO, b: Fx(B), theta: Fx(THETA), kappa: Fx::ZERO };
        let (x, y) = (Fx::from_wad(62 * WAD as i128).unwrap(), Fx::from_wad(2028 * WAD as i128).unwrap());
        let r = pct(35, 1000);
        let level = a.payout_fraction(U256::from(KEY), age, year, false).unwrap();
        let esc = a.payout_fraction(U256::from(KEY), age, year, true).unwrap();
        let want_level = gm.payout_fraction(x, y, r).unwrap().to_wad().unwrap() as u128;
        let want_esc = gm.payout_fraction(x, y, r.sub(pct(2, 100)).unwrap()).unwrap().to_wad().unwrap() as u128;
        assert_eq!(level, U256::from(want_level));
        assert_eq!(esc, U256::from(want_esc));
        // At 62 a level plan pays about 1/ä ≈ 0.5% a month; the escalating plan starts lower.
        let (l, e) = (level.to::<u128>() as f64 / 1e18, esc.to::<u128>() as f64 / 1e18);
        assert!(l > 0.004 && l < 0.007, "level {l}");
        assert!(e < l * 0.9 && e > l * 0.75, "escalating {e} vs level {l}");
        assert!(a.payout_fraction(U256::from(KEY + 1_000_000), age, year, false).is_err(), "unknown cohort");
    }

    #[test]
    fn escalating_quote_starts_lower_and_ends_higher() {
        let (_vm, a) = setup();
        let q = |esc| {
            a.quote(U256::from(KEY), U256::from(60 * WAD), U256::from(2026 * WAD), U256::from(62 * WAD), U256::from(3_000_000_000u64), U256::from(30_000_000u64), 128, esc).unwrap()
        };
        let (l, e) = (q(false), q(true));
        // (P10, P50, P90) at start, then at 85, in USDG with 6 decimals.
        assert!(l.0 <= l.1 && l.1 <= l.2 && l.3 <= l.4 && l.4 <= l.5, "percentiles ordered");
        assert!(e.1 < l.1, "escalating P50 starts lower: {} vs {}", e.1, l.1);
        assert!(e.4 > l.4, "and is higher at 85: {} vs {}", e.4, l.4);
        assert_eq!(q(false), l, "same inputs, same quote");
        assert!(a.quote(U256::from(KEY), U256::from(60 * WAD), U256::from(2026 * WAD), U256::from(62 * WAD), U256::ZERO, U256::ZERO, 0, false).is_err());
    }

    #[test]
    fn only_the_deployer_can_initialise() {
        let vm = TestVM::new();
        vm.set_sender(OTHER);
        let mut a = Actuary::from(&vm);
        assert!(a.init().is_err(), "a front-runner can't take the Actuary");
        vm.set_sender(OWNER);
        a.init().unwrap();
        assert_eq!(a.owner(), OWNER);
    }

    #[test]
    fn sealed_mortality_can_never_change() {
        let (vm, mut a) = setup();
        let (b, t) = (I256::try_from(B).unwrap(), I256::try_from(THETA).unwrap());
        vm.set_sender(OTHER);
        assert!(a.seal().is_err(), "owner only");
        vm.set_sender(OWNER);
        a.seal().unwrap();
        assert!(a.is_sealed());
        assert!(a.set_mortality(U256::from(KEY), I256::ZERO, b, t, I256::ZERO).is_err());
        assert!(a.set_mortality_batch(alloc::vec![U256::from(KEY)], alloc::vec![I256::ZERO], alloc::vec![b], alloc::vec![t], alloc::vec![I256::ZERO]).is_err());
        // Market assumptions stay adjustable, within their bounds.
        a.set_market(i(pct(5, 100)), i(pct(16, 100)), i(pct(35, 1000)), i(pct(35, 1000)), i(pct(3, 1000))).unwrap();
    }

    /// Skeptic: with the valuation rate at 0, escalation could reach 4% and price the escalating
    /// plan at −4%.
    #[test]
    fn escalation_never_exceeds_the_valuation_rate_even_at_zero() {
        let vm = TestVM::new();
        vm.set_sender(OWNER);
        let mut a = Actuary::from(&vm);
        a.init().unwrap();
        assert!(a.set_escalation(i(pct(4, 100))).is_err(), "no valuation rate yet: only zero");
        a.set_escalation(I256::ZERO).unwrap();
        assert!(a.set_market(i(pct(6, 100)), i(pct(16, 100)), i(pct(35, 1000)), I256::ZERO, i(pct(3, 1000))).is_err(), "no 0% valuation rate");
        a.set_market(i(pct(6, 100)), i(pct(16, 100)), i(pct(35, 1000)), i(pct(2, 100)), i(pct(3, 1000))).unwrap();
        assert!(a.set_escalation(i(pct(3, 100))).is_err(), "escalation above the valuation rate");
    }

    /// Skeptic: an 86-year-old starting at 119 was quoted 8.48e15 USDG a month.
    #[test]
    fn quotes_refuse_inputs_the_pool_would_never_take() {
        let (_vm, a) = setup();
        let q = |age: u128, start: u128, lump: u64, monthly: u64, year: u128| {
            a.quote(U256::from(KEY), U256::from(age * WAD), U256::from(year * WAD), U256::from(start * WAD), U256::from(lump), U256::from(monthly), 16, false)
        };
        assert!(q(60, 62, 3_000_000_000, 30_000_000, 2026).is_ok());
        for (age, start, lump, monthly, year, why) in [
            (86, 119, 10_000_000_000, 0, 2026, "start age 119"),
            (0, 62, 3_000_000_000, 0, 2026, "age 0"),
            (17, 62, 3_000_000_000, 0, 2026, "a minor"),
            (70, 62, 3_000_000_000, 0, 2026, "already past the start age"),
            (40, 45, 3_000_000_000, 0, 2026, "income before 50"),
            (40, 81, 3_000_000_000, 0, 2026, "income after 80"),
            (60, 62, 10_000_000_000_001, 0, 2026, "a lump sum over 10M USDG"),
            (60, 62, 0, 100_000_000_001, 2026, "over 100k USDG a month"),
            (60, 62, 3_000_000_000, 0, 1990, "the past"),
        ] {
            assert!(matches!(q(age, start, lump, monthly, year), Err(ActuaryError::BadInput(_))), "{why}");
        }
    }

    #[test]
    fn quotes_deduct_the_pool_fee_and_a_bigger_fee_pays_less() {
        let (_vm, mut a) = setup();
        let p50 = |a: &Actuary| a.quote(U256::from(KEY), U256::from(40 * WAD), U256::from(2026 * WAD), U256::from(62 * WAD), U256::from(3_000_000_000u64), U256::from(30_000_000u64), 64, false).unwrap().1;
        let with_fee = p50(&a);
        a.set_market(i(pct(6, 100)), i(pct(16, 100)), i(pct(35, 1000)), i(pct(35, 1000)), I256::ZERO).unwrap();
        let no_fee = p50(&a);
        assert!(with_fee < no_fee, "0.30% a year for 22 years costs income: {with_fee} vs {no_fee}");
        let ratio = with_fee.to::<u128>() as f64 / no_fee.to::<u128>() as f64;
        assert!(ratio > 0.92 && ratio < 0.97, "about (1 − 0.0025/12)^(22·12) less: {ratio}");
    }
}
