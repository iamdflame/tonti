//! Tonti pool: the lifelong-income ledger on Robinhood Chain (docs/protocol.md §1, §3–§7).
//!
//! Cohorts (birth year × sex × country × start age × plan × class) hold shares of three sleeves
//! (cash, SGOV, SPY) that sit in the Treasury. Members hold units of one tontine cohort and one
//! bequest cohort. Once a month anyone may settle the epoch, which:
//!   1. freezes fresh prices (the Treasury refuses stale ones, so it only starts in market hours),
//!   2. releases members whose death became final (estate gets income only up to the date of death),
//!   3. asks the Actuary for each cohort's death probability and payout fraction,
//!   4. runs actuary-core's tested ledger steps (fees → releases → fair credits → income sales),
//!   5. sells what the ledger ordered and splits the actual USDG received by oracle value,
//!   6. invests waiting contributions at the frozen NAV (forward pricing).
//! Settlement is paged (docs/protocol.md §5.1): `settle_steps(budget)` does at most `budget` deaths,
//! cohorts or deposits per transaction, so no pool size or queue length can push it past the block
//! gas limit. Paged and one-call settlement are the same arithmetic in the same order.
//! Members claim income lazily, and only while the LifeRegistry says they are alive.

#![cfg_attr(not(any(test, feature = "export-abi")), no_main)]
extern crate alloc;

use actuary_core::fixed::Fx;
use actuary_core::ledger::{step, value, Class, Cohort, Shares, SumSq, SLEEVES};
use actuary_core::fraud::{Signal, Sprt};
use actuary_core::rebalance::step as rebalance_step;
use actuary_core::wide::mul_div;
use alloc::vec::Vec;
use alloy_primitives::{keccak256, Address, FixedBytes, U256};
use alloy_sol_types::sol;
use stylus_sdk::prelude::*;
use stylus_sdk::storage::{StorageArray, StorageU256};
use stylus_sdk::stylus_core::calls::Call;

const WAD: u128 = 1_000_000_000_000_000_000;
const USDG_UNIT: u128 = 1_000_000;
const Q64: u128 = 1 << 64;
/// Julian year in seconds.
const YEAR_SECONDS: u128 = 31_557_600;
const FLAG_QUEUED_DEPOSIT: u64 = 1;
const FLAG_QUEUED_DEAD: u64 = 2;
const FLAG_RELEASED: u64 = 4;
const FLAG_EXIT_REQUESTED: u64 = 8;
const FLAG_QUEUED_EXIT: u64 = 16;
/// Released by exit (paid to the member) rather than death (paid to the beneficiary).
const FLAG_EXITED: u64 = 32;
/// Counted among its home cohort's living members (for the ghost-member detector).
const FLAG_COUNTED: u64 = 64;
/// Released by a presumed (unreported) death: owed their release back if they revive.
const FLAG_PRESUMED: u64 = 128;
const FLAG_QUEUED_RESTORE: u64 = 256;
const FLAG_RESTORED: u64 = 512;
/// Revived, and the reserve still owes them part of their release: repaid as it refills.
const FLAG_OWED: u64 = 1024;
/// A reported death's estate (income before the death, uninvested money, the bequest) waits this
/// long before it goes to the beneficiary. A revival in that time returns it to the member, so a
/// false report can't pay the family either (skeptic review 3).
const ESTATE_HOLD: u128 = 365 * 86_400;
/// Revival reserve (docs/protocol.md §6): 5% of every death's at-risk release, flowing on to the
/// survivors at 1/60 a month (a mean holding of five years). Any death, reported or presumed, can
/// be undone within five years by a member who proves they're alive.
const RESERVE_BPS: u128 = 500;
const RESERVE_MONTHS: i128 = 60;
/// Exit fee, paid to the members who stay as a mortality credit (docs/protocol.md §7).
const EXIT_FEE_BPS: u128 = 100;
const DAY: u128 = 86_400;
/// A matured exit notice must be used within 150 days (longer than a pending recovery or a death
/// report's window, which hold exits); after that it lapses and a new one is needed.
const EXIT_WINDOW: u128 = 150 * DAY;
/// A pause holds settlements, rebalances and exits for at most this long, and the guardian can
/// pause again only this long after the last pause began: no one key can freeze income.
const PAUSE_MAX: u128 = 7 * DAY;
const PAUSE_COOLDOWN: u128 = 30 * DAY;
/// Members of a group the detector flags have 120 days to renew their identity before income is held.
const HOLD_GRACE: u128 = 120 * DAY;
/// A rebalance still planning or trading after a day may be abandoned by anyone (nothing has traded).
const REBALANCE_TIMEOUT: u128 = DAY;
/// Reported deaths become final about five months after they happen (a report, then the 120-day
/// challenge window), so the ghost detector tests the deaths made final in an epoch against the
/// deaths it expected that long before. Tested against the same epoch, every honest group looked
/// like it was hiding deaths at the start: 100% flagged by month 3 (`actuary-cli ghosts`).
const REPORT_LAG: u128 = 150 * DAY;
/// Epochs of expected deaths kept per group (the lag in epochs is at most 7).
const RING: u64 = 8;
const RING_EPOCH_SHIFT: usize = 192;
const STATUS_DECEASED: u8 = 5;
const STATUS_PRESUMED: u8 = 6;
/// Smallest contribution: every queued deposit costs its sender real capital.
const MIN_CONTRIBUTION: u128 = USDG_UNIT;
/// Rebalance a cohort only when some sleeve is more than 5% of its value off target.
const REBALANCE_THRESHOLD_PCT: i128 = 5;

/// Only the deployer may initialise: the address is compiled in (`TONTI_DEPLOYER=0x…` at build
/// time), so nobody can front-run `init` between deployment and set-up. Built without it, the
/// contract can't be initialised at all.
#[cfg(not(test))]
const DEPLOYER: Address = match option_env!("TONTI_DEPLOYER") {
    Some(s) => Address::new(parse_address(s)),
    None => Address::ZERO,
};
#[cfg(test)]
const DEPLOYER: Address = Address::new([0x55; 20]);

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

// Paged settlement and rebalance phases (docs/protocol.md §5.1).
const PH_IDLE: u8 = 0;
const PH_RELEASE: u8 = 1;
const PH_WEIGH: u8 = 2;
const PH_CREDIT: u8 = 3;
const PH_SELL: u8 = 4;
const PH_BEQUEST: u8 = 5;
const PH_BOOK: u8 = 6;
const PH_INVEST: u8 = 7;
const PH_MINT: u8 = 8;
const RB_PLAN: u8 = 10;
const RB_TRADE: u8 = 11;
const RB_APPLY: u8 = 12;
/// Per-cohort working slots (`scratch[cohort·8 + slot]`), reused every epoch. Settlement:
/// weight, payout fraction, sale value. Rebalance: shares given (3), value wanted (3).
const S_WEIGHT: u64 = 0;
const S_PAYOUT: u64 = 1;
const S_SALE: u64 = 2;
const S_GIVE: u64 = 0;
const S_WANT: u64 = 3;

sol_interface! {
    interface ITreasury {
        function prices() external view returns (uint256[3] memory);
        function depositCash(uint256 usdgAmount) external returns (uint256);
        function sell(uint256[3] calldata shares) external returns (uint256);
        function pay(address to, uint256 usdgAmount) external;
        function buy(uint8 sleeve, uint256 usdgIn) external returns (uint256);
    }
    interface ILifeRegistry {
        function enroll(uint256 memberId, uint256 key, bytes32 qx, bytes32 qy, address payout, address beneficiary, address[3] calldata guardians) external;
        function payoutOf(uint256 memberId) external view returns (address);
        function beneficiaryOf(uint256 memberId) external view returns (address);
        function status(uint256 memberId) external view returns (uint8);
        function canReceiveIncome(uint256 memberId) external view returns (bool);
        function dateOfDeath(uint256 memberId) external view returns (uint64);
        function lastStrong(uint256 memberId) external view returns (uint64);
        function reporterOf(uint256 memberId) external view returns (address);
        function identified(uint256 memberId) external view returns (bool);
        function revivedAt(uint256 memberId) external view returns (uint64);
    }
    interface IActuary {
        function qMonth(uint256 key, uint256 age, uint256 year) external view returns (uint256);
        function payoutFraction(uint256 key, uint256 age, uint256 year, bool escalating) external view returns (uint256);
    }
    interface IERC20 {
        function transferFrom(address from, address to, uint256 amount) external returns (bool);
    }
}

sol! {
    error Unauthorized();
    error BadInput();
    error TooEarly();
    error CapExceeded();
    error NotEligible();
    error Released();
    error Engine(uint8 kind);
    error External();
    error Busy();
    error Paused();
    event Joined(uint256 indexed memberId, uint256 key, uint256 startAge, uint256 betaBps, bool escalating);
    event Contributed(uint256 indexed memberId, address indexed payer, uint256 usdg);
    event DeathQueued(uint256 indexed memberId);
    event Settled(uint256 indexed epoch, uint256 releases, uint256 usdgSold, uint256 usdgInvested, uint256 incomeBooked);
    event IncomeClaimed(uint256 indexed memberId, uint256 usdg);
    event EstatePaid(uint256 indexed memberId, address indexed to, uint256 usdg);
    event Rebalanced(uint256 indexed epoch, uint256 usdgSold, uint256 sgovBought, uint256 spyBought);
    event ExitRequested(uint256 indexed memberId, uint256 executableAt);
    event ExitCancelled(uint256 indexed memberId);
    event ExitQueued(uint256 indexed memberId);
    event MemberExited(uint256 indexed memberId, address indexed to, uint256 usdgIncome);
    event PauseSet(bool paused, address by);
    event GuardianSet(address guardian);
    /// The ghost-member detector fired for a group (country × birth decade): kind 1 = too few
    /// deaths (members must give a strong proof), 2 = too many (death reports under review).
    event GroupFlagged(uint256 indexed group, uint8 kind, uint256 epoch);
    event RebalanceAborted(uint256 indexed epoch);
    event RestoreQueued(uint256 indexed memberId);
    event Restored(uint256 indexed memberId, address indexed to, uint256 usdg);
}

#[derive(SolidityError)]
pub enum PoolError {
    Unauthorized(Unauthorized),
    BadInput(BadInput),
    TooEarly(TooEarly),
    CapExceeded(CapExceeded),
    NotEligible(NotEligible),
    Released(Released),
    Engine(Engine),
    External(External),
    Busy(Busy),
    Paused(Paused),
}

impl core::fmt::Debug for PoolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            PoolError::Unauthorized(_) => "Unauthorized",
            PoolError::BadInput(_) => "BadInput",
            PoolError::TooEarly(_) => "TooEarly",
            PoolError::CapExceeded(_) => "CapExceeded",
            PoolError::NotEligible(_) => "NotEligible",
            PoolError::Released(_) => "Released",
            PoolError::Engine(_) => "Engine",
            PoolError::External(_) => "External",
            PoolError::Busy(_) => "Busy",
            PoolError::Paused(_) => "Paused",
        })
    }
}

fn engine<T>(r: Result<T, actuary_core::MathError>) -> Result<T, PoolError> {
    r.map_err(|e| {
        PoolError::Engine(Engine {
            kind: match e {
                actuary_core::MathError::Overflow => 1,
                actuary_core::MathError::DivisionByZero => 2,
                actuary_core::MathError::Domain => 3,
            },
        })
    })
}

fn ext<T, E>(r: Result<T, E>) -> Result<T, PoolError> {
    r.map_err(|_| PoolError::External(External {}))
}

fn u128_of(v: U256) -> Result<u128, PoolError> {
    u128::try_from(v).map_err(|_| PoolError::BadInput(BadInput {}))
}

fn fx_of_wad(v: U256) -> Result<Fx, PoolError> {
    engine(Fx::from_wad(u128_of(v)? as i128))
}

fn wad_of_fx(v: Fx) -> Result<U256, PoolError> {
    Ok(U256::from(engine(v.to_wad())?.max(0) as u128))
}

/// Raw Q64.64 USDG → USDG base units (6 decimals), truncating.
fn usdg_raw(v: Fx) -> u128 {
    if v.0 <= 0 {
        return 0;
    }
    mul_div(v.0 as u128, USDG_UNIT, Q64).unwrap_or(0)
}

/// USDG base units → Q64.64 USDG.
fn usdg_fx(raw: u128) -> Result<Fx, PoolError> {
    engine(Fx::from_ratio(raw as i128, USDG_UNIT as i128))
}

sol_storage! {
    #[entrypoint]
    pub struct TontiPool {
        address owner;
        /// May pause (not unpause) instantly; the owner, a timelock, unpauses.
        address guardian;
        bool paused;
        uint256 paused_at;
        /// Restored members' USDG, deposited for them at the next settlement.
        uint256 restored_total;
        address treasury;
        address registry;
        address actuary;
        address usdg;
        uint256 epoch_length;
        uint256 member_cap;
        uint256 fee_bps_year;
        uint256 exit_notice;
        mapping(uint256 => uint256) m_exit_at;
        uint256 epoch;
        uint256 last_settle;
        uint256 carry0;
        uint256 carry1;
        uint256 carry2;
        uint256 unallocated_usdg;
        uint256 protocol_usdg;
        uint256 pending_total;

        uint256 cohort_count;
        mapping(uint256 => uint256) c_key;
        mapping(uint256 => uint256) c_meta;
        mapping(uint256 => uint256) c_s0;
        mapping(uint256 => uint256) c_s1;
        mapping(uint256 => uint256) c_s2;
        mapping(uint256 => uint256) c_units;
        mapping(uint256 => uint256) c_ipu;
        mapping(bytes32 => uint256) c_index;
        mapping(bytes32 => uint256) c_ipu_hist;
        mapping(uint256 => uint256) epoch_ts;

        uint256 member_count;
        mapping(uint256 => uint256) m_t;
        mapping(uint256 => uint256) m_b;
        mapping(uint256 => uint256) m_tu;
        mapping(uint256 => uint256) m_bu;
        mapping(uint256 => uint256) m_tsnap;
        mapping(uint256 => uint256) m_bsnap;
        mapping(uint256 => uint256) m_beta;
        mapping(uint256 => uint256) m_pending;
        mapping(uint256 => uint256) m_contributed;
        mapping(uint256 => uint256) m_flags;
        uint256[] pending_members;
        uint256[] pending_deaths;
        mapping(address => uint256) claimable;
        uint256 rebalanced_epoch;

        // Paged settlement and rebalance (docs/protocol.md §5.1). The queues are append-only;
        // each settlement consumes [head, end) as they stood when it began.
        uint256 phase;
        uint256 cursor;
        uint256 deaths_head;
        uint256 members_head;
        uint256 run_deaths_end;
        uint256 run_members_end;
        uint256 run_cohorts;
        uint256 run_ts;
        uint256[3] run_prices;
        uint256 run_fee;
        uint256[3] run_pool;
        uint256[3] run_claims;
        uint256[3] run_fees;
        uint256[3] run_credited;
        uint256[3] run_sell;
        uint256[3] run_carry;
        uint256 run_weight;
        uint256 run_wsq_hi;
        uint256 run_wsq_lo;
        uint256 run_sum_sq;
        uint256 run_sale_value;
        uint256 run_sale;
        uint256 run_sold;
        uint256 run_total_value;
        uint256 run_income;
        uint256 run_given;
        uint256 run_booked;
        uint256 run_unallocated;
        uint256 run_invested;
        uint256 run_minted;
        uint256[3] rb_gave;
        uint256[3] rb_want;
        uint256[3] rb_pot;
        uint256[3] rb_given;
        uint256 rb_usdg;
        uint256[3] rb_bought;
        mapping(uint256 => uint256) c_fee_epoch;
        mapping(uint256 => uint256) scratch;
        mapping(uint256 => uint256) d_left;
        uint256[3] reserve;
        mapping(uint256 => uint256) m_owed;
        /// A reported death's estate, held for `ESTATE_HOLD` from `m_estate_at`.
        mapping(uint256 => uint256) m_estate;
        mapping(uint256 => uint256) m_estate_at;
        /// A reported death's income dated after the death: held with the estate, then credited to
        /// the survivors, or returned to the member if she is revived (skeptic review 4).
        mapping(uint256 => uint256) m_post;
        /// A repayment for a member already in this month's deposit queue: invested next month,
        /// never minted from this month's deposit (skeptic review 4).
        mapping(uint256 => uint256) m_restored;

        // Ghost-member detector (docs/protocol.md §6): Wald's SPRT per group, group = country ×
        // birth decade. Each epoch's deaths and expected deaths accumulate during settlement.
        mapping(uint256 => uint256) c_members;
        mapping(uint256 => uint256) g_epoch;
        mapping(uint256 => uint256) g_deaths;
        mapping(uint256 => uint256) g_expected;
        mapping(uint256 => int256) g_low;
        mapping(uint256 => int256) g_high;
        mapping(uint256 => uint256) g_updated;
        mapping(uint256 => uint256) g_flagged_at;
        /// Expected deaths per group per epoch, a ring of `RING` slots: epoch << 192 | Q64.64 value.
        mapping(uint256 => uint256) g_exp_ring;
    }
}

/// A settlement's running totals, loaded once per page and stored at its end.
struct Run {
    prices: [Fx; SLEEVES],
    fee: Fx,
    pool: Shares,
    claims: Shares,
    fees: Shares,
    credited: Shares,
    sell: Shares,
    carry: Shares,
    weight: u128,
    /// Σw² over the credit weights, and the concentration Σs² computed from it (§3 correction).
    wsq: SumSq,
    sum_sq: Fx,
    sale_value: Fx,
    /// Whether anything was sold (the ledger's sale may be dust worth zero).
    sale: bool,
    sold: u128,
    total_value: Fx,
    income: Fx,
    given: u128,
    booked: Fx,
    unallocated: u128,
    invested: u128,
    /// Cash-sleeve shares the invested USDG bought, shared among the waiting members.
    minted: u128,
}

fn get3(a: &StorageArray<StorageU256, 3>) -> Result<Shares, PoolError> {
    Ok([u128_of(a.get(0usize).unwrap_or_default())?, u128_of(a.get(1usize).unwrap_or_default())?, u128_of(a.get(2usize).unwrap_or_default())?])
}

fn set3(a: &mut StorageArray<StorageU256, 3>, v: &Shares) {
    for (i, x) in v.iter().enumerate() {
        if let Some(mut slot) = a.setter(i) {
            slot.set(U256::from(*x));
        }
    }
}

fn fx_get3(a: &StorageArray<StorageU256, 3>) -> Result<[Fx; SLEEVES], PoolError> {
    let v = get3(a)?;
    Ok([Fx(v[0] as i128), Fx(v[1] as i128), Fx(v[2] as i128)])
}

fn fx_set3(a: &mut StorageArray<StorageU256, 3>, v: &[Fx; SLEEVES]) {
    set3(a, &[v[0].0.max(0) as u128, v[1].0.max(0) as u128, v[2].0.max(0) as u128]);
}

fn fx_u(v: Fx) -> U256 {
    U256::from(v.0.max(0) as u128)
}

fn u_fx(v: U256) -> Result<Fx, PoolError> {
    Ok(Fx(u128_of(v)? as i128))
}

fn slot(cohort: u64, s: u64) -> U256 {
    U256::from(cohort) * U256::from(8u8) + U256::from(s)
}

impl TontiPool {
    fn not_paused(&self) -> Result<(), PoolError> {
        if self.paused.get() {
            return Err(PoolError::Paused(Paused {}));
        }
        Ok(())
    }

    /// Settlements, rebalances and exits wait out a pause, but only for `PAUSE_MAX`.
    fn runs_allowed(&self) -> Result<(), PoolError> {
        if self.paused.get() && self.now() < u128_of(self.paused_at.get())? + PAUSE_MAX {
            return Err(PoolError::Paused(Paused {}));
        }
        Ok(())
    }

    fn only_owner(&self) -> Result<(), PoolError> {
        if self.vm().msg_sender() != self.owner.get() {
            return Err(PoolError::Unauthorized(Unauthorized {}));
        }
        Ok(())
    }

    fn now(&self) -> u128 {
        self.vm().block_timestamp() as u128
    }

    /// Where a member's income goes. The LifeRegistry keeps it (the member can change it there).
    fn payout_of(&self, m: U256) -> Result<Address, PoolError> {
        ext(ILifeRegistry::new(self.registry.get()).payout_of(self.vm(), Call::new(), m))
    }

    /// Who inherits a member's bequest share and estate.
    fn beneficiary_of(&self, m: U256) -> Result<Address, PoolError> {
        ext(ILifeRegistry::new(self.registry.get()).beneficiary_of(self.vm(), Call::new(), m))
    }

    /// Calendar year now, as WAD (1970 + seconds/Julian year).
    fn year_wad(&self) -> u128 {
        year_wad_at(self.now())
    }

    /// Age (WAD years) now of a cohort whose key ends in its birth year.
    fn age_wad(&self, key: U256) -> Result<u128, PoolError> {
        age_wad_at(key, self.now())
    }

    /// A cohort is one (country, sex, birth year) × class × start age × plan.
    fn cohort_id(&mut self, key: U256, bequest: bool, start_age: u64, escalating: bool) -> u64 {
        let flags = (bequest as u8) | (escalating as u8) << 1;
        let tag: FixedBytes<32> = keccak256([key.to_be_bytes::<32>().as_slice(), &[flags], &start_age.to_be_bytes()].concat());
        let found = self.c_index.get(tag);
        if found != U256::ZERO {
            return (found - U256::from(1u8)).to::<u64>();
        }
        let id = self.cohort_count.get();
        self.c_key.insert(id, key);
        self.c_meta.insert(id, U256::from(flags as u64 | (start_age << 8)));
        self.cohort_count.set(id + U256::from(1u8));
        self.c_index.insert(tag, id + U256::from(1u8));
        id.to::<u64>()
    }

    /// Epoch length in months, L. The Actuary prices one-month probabilities and payout fractions.
    fn months_per_epoch(&self) -> Result<Fx, PoolError> {
        engine(Fx::from_ratio(u128_of(self.epoch_length.get())? as i128 * 12, YEAR_SECONDS as i128))
    }

    /// Converts a one-month probability (or payout fraction) f into one for an epoch of L months:
    /// 1 − (1 − f)^L. Exact identity when L = 1.
    fn per_epoch(&self, f: Fx) -> Result<Fx, PoolError> {
        let l = self.months_per_epoch()?;
        if f.0 == 0 || l == Fx::ONE {
            return Ok(f);
        }
        let keep = engine(Fx::ONE.sub(f))?;
        engine(Fx::ONE.sub(engine(keep.pow(l))?))
    }

    fn cohort_at(&self, id: U256) -> Result<Cohort, PoolError> {
        Ok(Cohort {
            class: if self.c_meta.get(id).to::<u64>() & 1 == 1 { Class::Bequest } else { Class::Tontine },
            shares: [u128_of(self.c_s0.get(id))?, u128_of(self.c_s1.get(id))?, u128_of(self.c_s2.get(id))?],
            units: u128_of(self.c_units.get(id))?,
            income_per_unit: Fx(u128_of(self.c_ipu.get(id))? as i128),
        })
    }

    fn put_shares(&mut self, id: U256, c: &Cohort) {
        self.c_s0.insert(id, U256::from(c.shares[0]));
        self.c_s1.insert(id, U256::from(c.shares[1]));
        self.c_s2.insert(id, U256::from(c.shares[2]));
    }

    fn put_cohort(&mut self, id: U256, c: &Cohort) {
        self.put_shares(id, c);
        self.c_units.insert(id, U256::from(c.units));
        self.c_ipu.insert(id, fx_u(c.income_per_unit));
    }

    /// Charges a cohort this epoch's fee the first time the settlement touches it. The fee
    /// depends only on the cohort's own shares, so charging it at first touch instead of all
    /// at once changes nothing.
    fn touch(&mut self, id: U256, c: &mut Cohort, epoch: U256, run: &mut Run) -> Result<(), PoolError> {
        // An empty cohort owes no fee: skipping its write keeps never-funded cohorts cheap to walk.
        if c.shares.iter().all(|&x| x == 0) {
            return Ok(());
        }
        if self.c_fee_epoch.get(id) != epoch {
            engine(step::fee(c, run.fee, &mut run.fees))?;
            self.c_fee_epoch.insert(id, epoch);
        }
        Ok(())
    }

    /// This epoch's death probability and payout fraction for a cohort, at the settlement's
    /// frozen time, scaled to the epoch length.
    fn rates(&self, id: U256, c: &Cohort, ts: u128, has_members: bool) -> Result<(Fx, Fx), PoolError> {
        let (mut q, mut pay) = (Fx::ZERO, Fx::ZERO);
        if c.units == 0 && !has_members {
            return Ok((q, pay));
        }
        let (key, meta) = (self.c_key.get(id), self.c_meta.get(id).to::<u64>());
        let (age, year) = (U256::from(age_wad_at(key, ts)?), U256::from(year_wad_at(ts)));
        let actuary = IActuary::new(self.actuary.get());
        if c.class == Class::Tontine {
            q = fx_of_wad(ext(actuary.q_month(self.vm(), Call::new(), key, age, year))?)?;
        }
        // Only the pooled (tontine) part pays income. The bequest part stays invested for the family:
        // paid out at a life-contingent rate without credits it would dwindle, not stay level.
        if c.class == Class::Tontine && c.units > 0 && age >= U256::from(meta >> 8) * U256::from(WAD) {
            pay = fx_of_wad(ext(actuary.payout_fraction(self.vm(), Call::new(), key, age, year, meta & 2 != 0))?)?;
        }
        Ok((self.per_epoch(q)?, self.per_epoch(pay)?))
    }

    fn load_run(&self) -> Result<Run, PoolError> {
        Ok(Run {
            prices: fx_get3(&self.run_prices)?,
            fee: u_fx(self.run_fee.get())?,
            pool: get3(&self.run_pool)?,
            claims: get3(&self.run_claims)?,
            fees: get3(&self.run_fees)?,
            credited: get3(&self.run_credited)?,
            sell: get3(&self.run_sell)?,
            carry: get3(&self.run_carry)?,
            weight: u128_of(self.run_weight.get())?,
            wsq: SumSq { hi: u128_of(self.run_wsq_hi.get())?, lo: u128_of(self.run_wsq_lo.get())? },
            sum_sq: u_fx(self.run_sum_sq.get())?,
            sale_value: u_fx(self.run_sale_value.get())?,
            sale: self.run_sale.get() != U256::ZERO,
            sold: u128_of(self.run_sold.get())?,
            total_value: u_fx(self.run_total_value.get())?,
            income: u_fx(self.run_income.get())?,
            given: u128_of(self.run_given.get())?,
            booked: u_fx(self.run_booked.get())?,
            unallocated: u128_of(self.run_unallocated.get())?,
            invested: u128_of(self.run_invested.get())?,
            minted: u128_of(self.run_minted.get())?,
        })
    }

    fn store_run(&mut self, r: &Run) {
        fx_set3(&mut self.run_prices, &r.prices);
        self.run_fee.set(fx_u(r.fee));
        set3(&mut self.run_pool, &r.pool);
        set3(&mut self.run_claims, &r.claims);
        set3(&mut self.run_fees, &r.fees);
        set3(&mut self.run_credited, &r.credited);
        set3(&mut self.run_sell, &r.sell);
        set3(&mut self.run_carry, &r.carry);
        self.run_weight.set(U256::from(r.weight));
        self.run_wsq_hi.set(U256::from(r.wsq.hi));
        self.run_wsq_lo.set(U256::from(r.wsq.lo));
        self.run_sum_sq.set(fx_u(r.sum_sq));
        self.run_sale_value.set(fx_u(r.sale_value));
        self.run_sale.set(U256::from(r.sale as u8));
        self.run_sold.set(U256::from(r.sold));
        self.run_total_value.set(fx_u(r.total_value));
        self.run_income.set(fx_u(r.income));
        self.run_given.set(U256::from(r.given));
        self.run_booked.set(fx_u(r.booked));
        self.run_unallocated.set(U256::from(r.unallocated));
        self.run_invested.set(U256::from(r.invested));
        self.run_minted.set(U256::from(r.minted));
    }

    /// The part of the USDG received that a set of sold shares is worth, by oracle value.
    fn part(run: &Run, v: Fx) -> Result<Fx, PoolError> {
        if run.total_value.0 == 0 {
            return Ok(Fx::ZERO);
        }
        let received = usdg_fx(run.sold)?;
        Ok(Fx(mul_div(received.0 as u128, v.0 as u128, run.total_value.0 as u128).ok_or(PoolError::Engine(Engine { kind: 1 }))? as i128))
    }

    /// Income per unit of `cohort` as of the last epoch settled at or before `ts`. Epoch
    /// timestamps only increase, so this is a binary search: O(log epochs) storage reads.
    fn ipu_at(&self, cohort: U256, ts: u128) -> U256 {
        let (mut lo, mut hi, mut found) = (1u64, self.epoch.get().to::<u64>(), 0u64);
        while lo <= hi {
            let mid = lo + (hi - lo) / 2;
            if u128::try_from(self.epoch_ts.get(U256::from(mid))).unwrap_or(u128::MAX) <= ts {
                found = mid;
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
        }
        if found == 0 {
            return U256::ZERO;
        }
        self.c_ipu_hist.get(hist_key(cohort, U256::from(found)))
    }

    /// Unclaimed income for units held since `snap`, split at the date of death: the estate gets
    /// income up to that date, the rest returns to the pool. Values are raw Q64.64 USDG.
    fn split_income(&self, cohort: U256, units: U256, snap: U256, died_at: u128) -> Result<(u128, u128), PoolError> {
        let now = self.c_ipu.get(cohort);
        let at_death = self.ipu_at(cohort, died_at).max(snap).min(now);
        let owed = |from: U256, to: U256| -> Result<u128, PoolError> {
            if to <= from {
                return Ok(0);
            }
            mul_div(u128_of(units)?, u128_of(to - from)?, WAD).ok_or(PoolError::Engine(Engine { kind: 1 }))
        };
        Ok((owed(snap, at_death)?, owed(at_death, now)?))
    }
}

/// The income snapshot after `added` units join `held` units snapped at `snap`, with the cohort's
/// income index now at `ipu`: the held units keep exactly the income they had accrued (rounded
/// down, never up) and the new ones earn only from now on. (docs/protocol.md §5)
fn topped_up_snap(held: u128, snap: u128, added: u128, ipu: u128) -> Result<u128, PoolError> {
    let total = held + added;
    if held == 0 || ipu <= snap || total == 0 {
        return Ok(ipu);
    }
    let accrued = mul_div(held, ipu - snap, WAD).ok_or(PoolError::Engine(Engine { kind: 1 }))?;
    // (total)·(ipu − s)/WAD ≤ accrued  ⇔  s ≥ ipu − accrued·WAD/total: round the gap down.
    let gap = mul_div(accrued, WAD, total).ok_or(PoolError::Engine(Engine { kind: 1 }))?;
    Ok(ipu - gap)
}

fn hist_key(cohort: U256, epoch: U256) -> FixedBytes<32> {
    keccak256([cohort.to_be_bytes::<32>(), epoch.to_be_bytes::<32>()].concat())
}

/// Calendar year at `ts`, as WAD (1970 + seconds/Julian year).
fn year_wad_at(ts: u128) -> u128 {
    1970 * WAD + mul_div(ts, WAD, YEAR_SECONDS).unwrap_or(0)
}

/// Age (WAD years) at `ts` of a cohort whose key ends in its birth year; assumes mid-year birth.
fn age_wad_at(key: U256, ts: u128) -> Result<u128, PoolError> {
    let birth = u128_of(key % U256::from(10_000u32))?;
    year_wad_at(ts).checked_sub(birth * WAD + WAD / 2).ok_or(PoolError::BadInput(BadInput {}))
}

/// The detector's group for a cohort key: country × birth decade. The same people as a 10-year age
/// band, but nobody ages out of a flagged group.
fn group_of(key: U256) -> U256 {
    let iso = key / U256::from(20_000u32);
    let birth = key % U256::from(10_000u32);
    iso * U256::from(1_000u32) + birth / U256::from(10u8)
}

/// The settlement's steps, one item each (docs/protocol.md §5.1).
impl TontiPool {
    /// This epoch's (deaths, expected) accumulators for a group, reset on first touch.
    fn group_touch(&mut self, g: U256, epoch: U256) {
        if self.g_epoch.get(g) != epoch {
            self.g_epoch.insert(g, epoch);
            self.g_deaths.insert(g, U256::ZERO);
            self.g_expected.insert(g, U256::ZERO);
        }
    }

    /// Runs the group's SPRT once per epoch: the deaths made final this epoch against the deaths
    /// expected `lag` epochs ago, when those deaths mostly happened (docs/protocol.md §6).
    fn group_update(&mut self, g: U256, epoch: U256) -> Result<(), PoolError> {
        if self.g_updated.get(g) == epoch {
            return Ok(());
        }
        self.g_updated.insert(g, epoch);
        let (deaths, expected_now) = if self.g_epoch.get(g) == epoch {
            (self.g_deaths.get(g).to::<u64>(), self.g_expected.get(g))
        } else {
            (0, U256::ZERO)
        };
        let ring = |e: U256| g * U256::from(RING) + (e % U256::from(RING));
        self.g_exp_ring.insert(ring(epoch), (epoch << RING_EPOCH_SHIFT) | expected_now);
        let length = u128_of(self.epoch_length.get())?;
        let lag = U256::from(REPORT_LAG.div_ceil(length).clamp(1, (RING - 1) as u128));
        if epoch <= lag {
            return Ok(()); // warm-up: no deaths could be final yet
        }
        let then = self.g_exp_ring.get(ring(epoch - lag));
        if then >> RING_EPOCH_SHIFT != epoch - lag {
            return Ok(()); // the group wasn't weighed that epoch: nothing to compare
        }
        let expected = u_fx(then & ((U256::from(1u8) << RING_EPOCH_SHIFT) - U256::from(1u8)))?;
        let i = |v: alloy_primitives::I256| i128::try_from(v).map(Fx).map_err(|_| PoolError::Engine(Engine { kind: 1 }));
        let mut sprt = Sprt { low: i(self.g_low.get(g))?, high: i(self.g_high.get(g))? };
        let signal = engine(sprt.update(deaths, expected))?;
        let s = |v: Fx| alloy_primitives::I256::try_from(v.0).unwrap_or_default();
        self.g_low.insert(g, s(sprt.low));
        self.g_high.insert(g, s(sprt.high));
        let kind = match signal {
            Signal::HiddenDeaths => {
                self.g_flagged_at.insert(g, self.run_ts.get());
                1u8
            }
            Signal::ExcessDeaths => 2,
            Signal::Continue => 0,
        };
        if kind != 0 {
            self.vm().log(GroupFlagged { group: g, kind, epoch });
        }
        Ok(())
    }

    /// A member of a group flagged for hidden deaths has 120 days to give a strong proof dated after
    /// the flag; after that their income and exit are held until they do.
    fn not_held(&self, member_id: U256) -> Result<(), PoolError> {
        let flagged = self.g_flagged_at.get(group_of(self.c_key.get(self.m_t.get(member_id))));
        if flagged == U256::ZERO || U256::from(self.now()) < flagged + U256::from(HOLD_GRACE) {
            return Ok(());
        }
        let registry = ILifeRegistry::new(self.registry.get());
        if U256::from(ext(registry.last_strong(self.vm(), Call::new(), member_id))?) < flagged {
            return Err(PoolError::NotEligible(NotEligible {}));
        }
        Ok(())
    }

    fn begin_settlement(&mut self) -> Result<(), PoolError> {
        let now = self.now();
        let last = u128_of(self.last_settle.get())?;
        if last != 0 && now < last + u128_of(self.epoch_length.get())? {
            return Err(PoolError::TooEarly(TooEarly {}));
        }
        let treasury = ITreasury::new(self.treasury.get());
        let p = ext(treasury.prices(self.vm(), Call::new()))?;
        let run = Run {
            prices: [fx_of_wad(p[0])?, fx_of_wad(p[1])?, fx_of_wad(p[2])?],
            fee: engine(Fx::from_ratio(
                u128_of(self.fee_bps_year.get())? as i128 * u128_of(self.epoch_length.get())? as i128,
                10_000 * YEAR_SECONDS as i128,
            ))?,
            pool: [u128_of(self.carry0.get())?, u128_of(self.carry1.get())?, u128_of(self.carry2.get())?],
            claims: [0; SLEEVES],
            fees: [0; SLEEVES],
            credited: [0; SLEEVES],
            sell: [0; SLEEVES],
            carry: [0; SLEEVES],
            weight: 0,
            wsq: SumSq::default(),
            sum_sq: Fx::ZERO,
            sale_value: Fx::ZERO,
            sale: false,
            sold: 0,
            total_value: Fx::ZERO,
            income: Fx::ZERO,
            given: 0,
            booked: Fx::ZERO,
            unallocated: u128_of(self.unallocated_usdg.get())?,
            invested: 0,
            minted: 0,
        };
        let mut run = run;
        let mut reserve = get3(&self.reserve)?;
        let months = self.months_per_epoch()?;
        for a in 0..SLEEVES {
            let x = mul_div(reserve[a], months.0 as u128, (RESERVE_MONTHS as u128) << 64).unwrap_or(0);
            reserve[a] -= x;
            run.pool[a] += x;
        }
        set3(&mut self.reserve, &reserve);
        self.store_run(&run);
        self.run_ts.set(U256::from(now));
        self.run_cohorts.set(self.cohort_count.get());
        self.run_deaths_end.set(U256::from(self.pending_deaths.len()));
        self.run_members_end.set(U256::from(self.pending_members.len()));
        self.cursor.set(self.deaths_head.get());
        Ok(())
    }

    /// Releases one queued death: the estate's income up to the date of death, any uninvested
    /// contribution back to the estate, and both holdings out of their cohorts.
    fn release_one(&mut self, i: u64, epoch: U256, run: &mut Run) -> Result<(), PoolError> {
        let m = self.pending_deaths.get(i as usize).unwrap_or_default();
        let flags = self.m_flags.get(m).to::<u64>();
        if flags & FLAG_QUEUED_RESTORE != 0 {
            return self.restore_one(i, m, run);
        }
        if flags & FLAG_RELEASED != 0 {
            return Ok(()); // queued twice (an exit overtaken by a death): released once already
        }
        if flags & FLAG_QUEUED_DEAD == 0 {
            return self.exit_one(i, m, epoch, run);
        }
        let registry = ILifeRegistry::new(self.registry.get());
        // Revived before the settlement reached them (skeptic review 3): nothing is released.
        let st = ext(registry.status(self.vm(), Call::new(), m))?;
        if st != STATUS_DECEASED && st != STATUS_PRESUMED {
            self.m_flags.insert(m, U256::from(flags & !FLAG_QUEUED_DEAD));
            return Ok(());
        }
        let presumed = ext(registry.reporter_of(self.vm(), Call::new(), m))? == Address::ZERO;
        let died = ext(registry.date_of_death(self.vm(), Call::new(), m))? as u128;
        let (t, b) = (self.m_t.get(m), self.m_b.get(m));
        let (tu, bu) = (self.m_tu.get(m), self.m_bu.get(m));
        let (et, pt) = self.split_income(t, tu, self.m_tsnap.get(m), died)?;
        let (eb, pb) = self.split_income(b, bu, self.m_bsnap.get(m), died)?;
        let estate = usdg_raw(Fx((et + eb) as i128));
        let post = usdg_raw(Fx((pt + pb) as i128));
        if presumed {
            run.unallocated += post; // income after the death: the survivors'
        } else {
            // A report can be false: its post-date income waits with the estate.
            self.m_post.insert(m, self.m_post.get(m) + U256::from(post));
            self.m_estate_at.insert(m, U256::from(self.now())); // every reported death restarts the hold
        }
        // Waiting (uninvested) contributions go back to the estate as well.
        let pending = self.m_pending.get(m);
        if pending > U256::ZERO {
            self.pending_total.set(self.pending_total.get() - pending);
            self.m_pending.insert(m, U256::ZERO);
        }
        self.to_estate(m, U256::from(estate) + pending, presumed)?;
        let g = group_of(self.c_key.get(t));
        self.group_touch(g, epoch);
        self.g_deaths.insert(g, self.g_deaths.get(g) + U256::from(1u8));
        self.uncount(m, t);
        let mut ct = self.cohort_at(t)?;
        self.touch(t, &mut ct, epoch, run)?;
        let before = run.pool;
        engine(step::release(&mut ct, u128_of(tu)?, &mut run.pool, &mut run.claims))?;
        self.put_cohort(t, &ct);
        // 5% of the release waits in the revival reserve, and the member is owed the whole release
        // back if they prove they're alive: a false report or a wrong presumption can be undone.
        let mut reserve = get3(&self.reserve)?;
        for a in 0..SLEEVES {
            let released = run.pool[a] - before[a];
            let x = mul_div(released, RESERVE_BPS, 10_000).unwrap_or(0);
            run.pool[a] -= x;
            reserve[a] += x;
            // Added to anything an earlier undone death still owes her, never written over it.
            let key = U256::from(m) * U256::from(4u8) + U256::from(a);
            self.m_owed.insert(key, self.m_owed.get(key) + U256::from(released));
        }
        set3(&mut self.reserve, &reserve);
        let mut cb = self.cohort_at(b)?;
        self.touch(b, &mut cb, epoch, run)?;
        let left = engine(step::release(&mut cb, u128_of(bu)?, &mut run.pool, &mut run.claims))?;
        self.put_cohort(b, &cb);
        for (a, x) in left.iter().enumerate() {
            self.d_left.insert(U256::from(i) * U256::from(4u8) + U256::from(a), U256::from(*x));
        }
        self.m_tu.insert(m, U256::ZERO);
        self.m_bu.insert(m, U256::ZERO);
        let f = self.m_flags.get(m).to::<u64>();
        let presumed_flag = if presumed { FLAG_PRESUMED } else { 0 };
        self.m_flags.insert(m, U256::from((f | FLAG_RELEASED | presumed_flag) & !(FLAG_QUEUED_DEPOSIT | FLAG_OWED)));
        Ok(())
    }

    /// Releases a member who gave notice and is still alive: their bequest shares and 99% of their
    /// at-risk shares are sold for them; the other 1% stays with the survivors as a credit.
    fn exit_one(&mut self, i: u64, m: U256, epoch: U256, run: &mut Run) -> Result<(), PoolError> {
        let (t, b) = (self.m_t.get(m), self.m_b.get(m));
        let (tu, bu) = (self.m_tu.get(m), self.m_bu.get(m));
        let to = self.payout_of(m)?;
        // Any income and any uninvested contribution go back to the member.
        let (it, _) = self.split_income(t, tu, self.m_tsnap.get(m), u128::MAX)?;
        let (ib, _) = self.split_income(b, bu, self.m_bsnap.get(m), u128::MAX)?;
        let income = usdg_raw(Fx((it + ib) as i128));
        let back = U256::from(income) + self.m_pending.get(m);
        if self.m_pending.get(m) > U256::ZERO {
            self.pending_total.set(self.pending_total.get() - self.m_pending.get(m));
            self.m_pending.insert(m, U256::ZERO);
        }
        self.claimable.insert(to, self.claimable.get(to) + back);
        self.uncount(m, t);
        let mut ct = self.cohort_at(t)?;
        self.touch(t, &mut ct, epoch, run)?;
        let before = run.pool;
        engine(step::release(&mut ct, u128_of(tu)?, &mut run.pool, &mut run.claims))?;
        self.put_cohort(t, &ct);
        let mut left = [0u128; SLEEVES];
        for a in 0..SLEEVES {
            let released = run.pool[a] - before[a];
            let keep = released - mul_div(released, EXIT_FEE_BPS, 10_000).unwrap_or(0);
            run.pool[a] -= keep;
            run.claims[a] += keep;
            left[a] = keep;
        }
        let mut cb = self.cohort_at(b)?;
        self.touch(b, &mut cb, epoch, run)?;
        let lb = engine(step::release(&mut cb, u128_of(bu)?, &mut run.pool, &mut run.claims))?;
        self.put_cohort(b, &cb);
        for a in 0..SLEEVES {
            self.d_left.insert(U256::from(i) * U256::from(4u8) + U256::from(a), U256::from(left[a] + lb[a]));
        }
        self.m_tu.insert(m, U256::ZERO);
        self.m_bu.insert(m, U256::ZERO);
        let f = self.m_flags.get(m).to::<u64>();
        self.m_flags.insert(m, U256::from((f | FLAG_RELEASED | FLAG_EXITED) & !(FLAG_QUEUED_DEPOSIT | FLAG_QUEUED_EXIT | FLAG_EXIT_REQUESTED)));
        self.vm().log(MemberExited { memberId: m, to, usdgIncome: back });
        Ok(())
    }

    /// A death's estate: to the beneficiary at once for a presumed death (two years of silence at
    /// least); held for `ESTATE_HOLD` for a reported one (`pay_estate`).
    fn to_estate(&mut self, m: U256, usdg: U256, presumed: bool) -> Result<(), PoolError> {
        if usdg == U256::ZERO {
            return Ok(());
        }
        if presumed {
            let heir = self.beneficiary_of(m)?;
            self.claimable.insert(heir, self.claimable.get(heir) + usdg);
            self.vm().log(EstatePaid { memberId: m, to: heir, usdg });
        } else {
            self.m_estate.insert(m, self.m_estate.get(m) + usdg);
            self.m_estate_at.insert(m, U256::from(self.now()));
        }
        Ok(())
    }

    /// Repays a member whose death was undone (reported or presumed, `LifeRegistry.revive`): their
    /// released at-risk shares, from the revival reserve, as far as it goes. Sold with the month's
    /// sale; `bequest_one` puts the proceeds back into their account.
    fn restore_one(&mut self, i: u64, m: U256, run: &mut Run) -> Result<(), PoolError> {
        let mut reserve = get3(&self.reserve)?;
        let mut short = false;
        for a in 0..SLEEVES {
            let key = U256::from(m) * U256::from(4u8) + U256::from(a);
            let owed = u128_of(self.m_owed.get(key))?;
            let pay = owed.min(reserve[a]);
            reserve[a] -= pay;
            run.claims[a] += pay;
            self.d_left.insert(U256::from(i) * U256::from(4u8) + U256::from(a), U256::from(pay));
            // What the reserve can't cover yet stays owed, repaid by a later `restore` as it refills.
            self.m_owed.insert(key, U256::from(owed - pay));
            short |= owed > pay;
        }
        set3(&mut self.reserve, &reserve);
        let f = self.m_flags.get(m).to::<u64>();
        let owed_flag = if short { FLAG_OWED } else { 0 };
        self.m_flags.insert(m, U256::from(((f | FLAG_RESTORED) & !(FLAG_QUEUED_RESTORE | FLAG_OWED)) | owed_flag));
        Ok(())
    }

    /// Removes a released member from their home cohort's living count.
    fn uncount(&mut self, m: U256, t: U256) {
        let f = self.m_flags.get(m).to::<u64>();
        if f & FLAG_COUNTED != 0 {
            self.c_members.insert(t, self.c_members.get(t).saturating_sub(U256::from(1u8)));
            self.m_flags.insert(m, U256::from(f & !FLAG_COUNTED));
        }
    }

    /// Charges a cohort's fee (if no release did) and records its credit weight and payout rate.
    fn weigh_one(&mut self, k: u64, epoch: U256, run: &mut Run) -> Result<(), PoolError> {
        let id = U256::from(k);
        let mut c = self.cohort_at(id)?;
        self.touch(id, &mut c, epoch, run)?;
        let members = u128_of(self.c_members.get(id))?;
        let (q, pay) = self.rates(id, &c, u128_of(self.run_ts.get())?, members > 0)?;
        if members > 0 {
            let g = group_of(self.c_key.get(id));
            self.group_touch(g, epoch);
            let e = engine(u_fx(self.g_expected.get(g))?.add(engine(q.mul_int(members as i128))?))?;
            self.g_expected.insert(g, fx_u(e));
        }
        let w = engine(step::weight(&c, q, &run.prices))?;
        run.weight = run.weight.checked_add(w).ok_or(PoolError::Engine(Engine { kind: 1 }))?;
        engine(run.wsq.add(w))?;
        self.scratch.insert(slot(k, S_WEIGHT), U256::from(w));
        self.scratch.insert(slot(k, S_PAYOUT), fx_u(pay));
        self.put_shares(id, &c);
        Ok(())
    }

    /// Credits a cohort its share of the released pool and takes its income sale.
    fn credit_one(&mut self, k: u64, run: &mut Run) -> Result<(), PoolError> {
        let id = U256::from(k);
        let mut c = self.cohort_at(id)?;
        let w = u128_of(self.scratch.get(slot(k, S_WEIGHT)))?;
        let pay = u_fx(self.scratch.get(slot(k, S_PAYOUT)))?;
        let sale = engine(step::credit_and_sell(&mut c, w, &run.pool, run.weight, run.sum_sq, pay, &mut run.credited, &mut run.sell))?;
        let sv = match sale {
            Some(sh) => engine(value(&sh, &run.prices))?,
            None => Fx::ZERO,
        };
        run.sale_value = engine(run.sale_value.add(sv))?;
        self.scratch.insert(slot(k, S_SALE), fx_u(sv));
        if w > 0 || sale.is_some() {
            self.put_shares(id, &c);
        }
        Ok(())
    }

    /// One sale for income, bequests and fees. The USDG received is split by oracle value.
    fn sell_all(&mut self, run: &mut Run) -> Result<(), PoolError> {
        for a in 0..SLEEVES {
            run.carry[a] = run.pool[a] - run.credited[a];
        }
        let sell: Shares = core::array::from_fn(|a| run.sell[a] + run.claims[a] + run.fees[a]);
        if sell.iter().all(|&x| x == 0) {
            return Ok(());
        }
        let treasury = ITreasury::new(self.treasury.get());
        let ctx = Call::new_mutating(self);
        run.sold = u128_of(ext(treasury.sell(self.vm(), ctx, [U256::from(sell[0]), U256::from(sell[1]), U256::from(sell[2])]))?)?;
        run.sale = true;
        run.total_value = engine(value(&sell, &run.prices))?;
        run.income = Self::part(run, engine(value(&run.sell, &run.prices))?)?;
        let fees = usdg_raw(Self::part(run, engine(value(&run.fees, &run.prices))?)?);
        run.given = fees;
        self.protocol_usdg.set(self.protocol_usdg.get() + U256::from(fees));
        Ok(())
    }

    /// Pays what one released member's shares sold for: bequest shares to the beneficiary after a
    /// death, everything the member takes with them after an exit. A revived member's repayment
    /// goes back into the pool as their own deposit, not out of it: an undone death is never a way
    /// to leave after income has started.
    fn bequest_one(&mut self, i: u64, run: &mut Run) -> Result<(), PoolError> {
        let m = self.pending_deaths.get(i as usize).unwrap_or_default();
        let flags = self.m_flags.get(m).to::<u64>();
        let base = U256::from(i) * U256::from(4u8);
        let left: Shares = [
            u128_of(self.d_left.get(base))?,
            u128_of(self.d_left.get(base + U256::from(1u8)))?,
            u128_of(self.d_left.get(base + U256::from(2u8)))?,
        ];
        let amount = if run.sale { usdg_raw(Self::part(run, engine(value(&left, &run.prices))?)?) } else { 0 };
        if flags & FLAG_RESTORED != 0 {
            run.given += amount;
            let registry = ILifeRegistry::new(self.registry.get());
            let st = ext(registry.status(self.vm(), Call::new(), m))?;
            if st == STATUS_DECEASED || st == STATUS_PRESUMED {
                // She has really died since the restore was queued (skeptic review 4): she stays
                // released, and the repayment is part of her estate.
                self.m_flags.insert(m, U256::from(flags & !FLAG_RESTORED));
                let presumed = ext(registry.reporter_of(self.vm(), Call::new(), m))? == Address::ZERO;
                self.to_estate(m, U256::from(amount), presumed)?;
                return Ok(());
            }
            if flags & FLAG_EXITED != 0 {
                // She has left the pool: what the reserve still owed her is paid to her in cash.
                let to = self.payout_of(m)?;
                self.claimable.insert(to, self.claimable.get(to) + U256::from(amount));
                self.m_flags.insert(m, U256::from(flags & !FLAG_RESTORED));
                self.vm().log(Restored { memberId: m, to, usdg: U256::from(amount) });
                return Ok(());
            }
            // Back into her own account, with the estate and post-date income her false death held.
            let back = U256::from(amount) + self.m_estate.get(m) + self.m_post.get(m);
            self.m_estate.insert(m, U256::ZERO);
            self.m_post.insert(m, U256::ZERO);
            let mut f = flags & !(FLAG_RESTORED | FLAG_RELEASED | FLAG_QUEUED_DEAD | FLAG_PRESUMED);
            if back > U256::ZERO {
                self.restored_total.set(self.restored_total.get() + back);
                if f & FLAG_QUEUED_DEPOSIT != 0 {
                    // Already in this month's deposit queue: kept apart, invested next month.
                    self.m_restored.insert(m, self.m_restored.get(m) + back);
                } else {
                    self.m_pending.insert(m, self.m_pending.get(m) + back);
                    f |= FLAG_QUEUED_DEPOSIT;
                    self.pending_members.push(m);
                }
            }
            self.m_flags.insert(m, U256::from(f));
            let to = self.vm().contract_address();
            self.vm().log(Restored { memberId: m, to, usdg: back });
            return Ok(());
        }
        if amount > 0 {
            run.given += amount;
            if flags & FLAG_EXITED != 0 {
                // The member themselves, for an exit.
                let to = self.payout_of(m)?;
                self.claimable.insert(to, self.claimable.get(to) + U256::from(amount));
            } else {
                // The bequest, for a death: the estate.
                self.to_estate(m, U256::from(amount), flags & FLAG_PRESUMED != 0)?;
            }
        }
        Ok(())
    }

    /// Raises a paying cohort's income per unit by its share of the income proceeds, and records
    /// every cohort's income index for this epoch (estates are paid up to the date of death).
    fn book_one(&mut self, k: u64, epoch: U256, run: &mut Run) -> Result<(), PoolError> {
        let id = U256::from(k);
        let sv = u_fx(self.scratch.get(slot(k, S_SALE)))?;
        if run.sale && run.sale_value.0 > 0 && sv.0 > 0 {
            let mut c = self.cohort_at(id)?;
            let paid = engine(step::book(&mut c, sv, run.sale_value, run.income))?;
            run.booked = engine(run.booked.add(paid))?;
            self.c_ipu.insert(id, fx_u(c.income_per_unit));
        }
        self.c_ipu_hist.insert(hist_key(id, epoch), self.c_ipu.get(id));
        if self.c_meta.get(id).to::<u64>() & 1 == 0 {
            self.group_update(group_of(self.c_key.get(id)), epoch)?;
        }
        Ok(())
    }

    /// Rounding leftovers stay with the pool and are credited to survivors next month.
    fn close_books(&mut self, run: &mut Run) {
        if run.sale {
            run.given += usdg_raw(run.booked);
            run.unallocated += run.sold.saturating_sub(run.given);
        }
    }

    /// Forward pricing: waiting contributions and pool leftovers go into the cash sleeve.
    fn invest(&mut self, run: &mut Run) -> Result<(), PoolError> {
        let to_invest = u128_of(self.pending_total.get())? + run.unallocated;
        run.invested = to_invest;
        if to_invest == 0 {
            return Ok(());
        }
        let treasury = ITreasury::new(self.treasury.get());
        let ctx = Call::new_mutating(self);
        run.minted = u128_of(ext(treasury.deposit_cash(self.vm(), ctx, U256::from(to_invest)))?)?;
        run.carry[0] += mul_div(run.minted, run.unallocated, to_invest).unwrap_or(0);
        Ok(())
    }

    /// Mints one waiting member's units at the frozen NAV, split by their bequest share.
    fn mint_one(&mut self, i: u64, run: &Run) -> Result<(), PoolError> {
        let m = self.pending_members.get(i as usize).unwrap_or_default();
        self.mint_units(m, run)?;
        // A repayment that arrived while she was queued this month waits for next month's deposit
        // (this month's `invested` never included it).
        let r = self.m_restored.get(m);
        if r > U256::ZERO {
            self.m_restored.insert(m, U256::ZERO);
            self.m_pending.insert(m, self.m_pending.get(m) + r);
            let f = self.m_flags.get(m).to::<u64>();
            self.m_flags.insert(m, U256::from(f | FLAG_QUEUED_DEPOSIT));
            self.pending_members.push(m);
        }
        Ok(())
    }

    fn mint_units(&mut self, m: U256, run: &Run) -> Result<(), PoolError> {
        let amount = u128_of(self.m_pending.get(m))?;
        if amount == 0 || run.invested == 0 {
            return Ok(());
        }
        let sh = mul_div(run.minted, amount, run.invested).unwrap_or(0);
        let beta = u128_of(self.m_beta.get(m))?;
        let sh_b = mul_div(sh, beta, 10_000).unwrap_or(0);
        for (cohort, sh_k, tontine) in [(self.m_t.get(m), sh - sh_b, true), (self.m_b.get(m), sh_b, false)] {
            if sh_k == 0 {
                continue;
            }
            let mut c = self.cohort_at(cohort)?;
            let add = engine(value(&[sh_k, 0, 0], &run.prices))?;
            let units = if c.units == 0 {
                engine(add.to_wad())? as u128 // 1 unit = 1 USDG for a new cohort
            } else {
                mul_div(c.units, add.0 as u128, engine(value(&c.shares, &run.prices))?.0 as u128).unwrap_or(0)
            };
            c.shares[0] += sh_k;
            c.units += units;
            self.put_cohort(cohort, &c);
            let (held, snapped) = if tontine { (self.m_tu.get(m), self.m_tsnap.get(m)) } else { (self.m_bu.get(m), self.m_bsnap.get(m)) };
            let snap = topped_up_snap(u128_of(held)?, u128_of(snapped)?, units, fx_u(c.income_per_unit).to::<u128>())?;
            let total = U256::from(u128_of(held)? + units);
            if tontine {
                self.m_tu.insert(m, total);
                self.m_tsnap.insert(m, U256::from(snap));
            } else {
                self.m_bu.insert(m, total);
                self.m_bsnap.insert(m, U256::from(snap));
            }
        }
        self.m_pending.insert(m, U256::ZERO);
        let f = self.m_flags.get(m).to::<u64>();
        if f & FLAG_COUNTED == 0 {
            let t = self.m_t.get(m);
            self.c_members.insert(t, self.c_members.get(t) + U256::from(1u8));
        }
        self.m_flags.insert(m, U256::from((f | FLAG_COUNTED) & !FLAG_QUEUED_DEPOSIT));
        Ok(())
    }

    /// The rebalance's market trades: sell the net excess, then spend the USDG on the net wants,
    /// pro rata to their value (the last sleeve takes the remainder). Returns the shares bought.
    fn trade(&mut self, sell: &Shares, buy_value: &[Fx; SLEEVES]) -> Result<Shares, PoolError> {
        let treasury = ITreasury::new(self.treasury.get());
        let mut usdg = 0u128;
        if sell.iter().any(|&x| x > 0) {
            let ctx = Call::new_mutating(self);
            usdg = u128_of(ext(treasury.sell(self.vm(), ctx, [U256::from(sell[0]), U256::from(sell[1]), U256::from(sell[2])]))?)?;
        }
        self.rb_usdg.set(U256::from(usdg));
        let total_buy = engine(buy_value.iter().try_fold(Fx::ZERO, |acc, v| acc.add(*v)))?;
        let mut bought = [0u128; SLEEVES];
        if usdg > 0 && total_buy.0 > 0 {
            let buyers: Vec<usize> = (0..SLEEVES).filter(|&a| buy_value[a].0 > 0).collect();
            let mut spent = 0u128;
            for (i, &a) in buyers.iter().enumerate() {
                let amount = if i + 1 == buyers.len() {
                    usdg - spent
                } else {
                    mul_div(usdg, buy_value[a].0 as u128, total_buy.0 as u128).unwrap_or(0)
                };
                spent += amount;
                if amount == 0 {
                    continue;
                }
                let ctx = Call::new_mutating(self);
                bought[a] = u128_of(if a == 0 {
                    ext(treasury.deposit_cash(self.vm(), ctx, U256::from(amount)))?
                } else {
                    ext(treasury.buy(self.vm(), ctx, a as u8, U256::from(amount)))?
                })?;
            }
        } else if usdg > 0 {
            // Sold but nothing to buy: keep the USDG for survivors (reinvested at the next settlement).
            self.unallocated_usdg.set(self.unallocated_usdg.get() + U256::from(usdg));
        }
        set3(&mut self.rb_bought, &bought);
        Ok(bought)
    }

    fn finish_settlement(&mut self, run: &Run, epoch: U256) {
        self.carry0.set(U256::from(run.carry[0]));
        self.carry1.set(U256::from(run.carry[1]));
        self.carry2.set(U256::from(run.carry[2]));
        self.unallocated_usdg.set(U256::ZERO);
        // Restored members' money was sold this month; it is invested for them with the next one.
        self.pending_total.set(self.restored_total.get());
        self.restored_total.set(U256::ZERO);
        let (d_head, d_end) = (self.deaths_head.get(), self.run_deaths_end.get());
        self.deaths_head.set(d_end);
        self.members_head.set(self.run_members_end.get());
        let ts = self.run_ts.get();
        self.epoch.set(epoch);
        self.epoch_ts.insert(epoch, ts);
        self.last_settle.set(ts);
        self.vm().log(Settled {
            epoch,
            releases: d_end - d_head,
            usdgSold: U256::from(run.sold),
            usdgInvested: U256::from(run.invested),
            incomeBooked: U256::from(usdg_raw(run.booked)),
        });
    }
}

#[public]
impl TontiPool {
    pub fn init(&mut self, treasury: Address, registry: Address, actuary: Address, usdg: Address) -> Result<(), PoolError> {
        let sender = self.vm().msg_sender();
        if self.owner.get() != Address::ZERO || sender != DEPLOYER {
            return Err(PoolError::Unauthorized(Unauthorized {}));
        }
        self.owner.set(sender);
        self.treasury.set(treasury);
        self.registry.set(registry);
        self.actuary.set(actuary);
        self.usdg.set(usdg);
        self.epoch_length.set(U256::from(YEAR_SECONDS / 12)); // one Julian month: L = 1
        self.member_cap.set(U256::from(25u128 * USDG_UNIT)); // research preview cap
        self.fee_bps_year.set(U256::from(30u8));
        self.exit_notice.set(U256::from(365 * DAY));
        Ok(())
    }

    /// Bounded: epochs of 28 to 31 days (the detector's reporting lag is counted in epochs, and its
    /// ring holds 7 of them); cap at most 100,000 USDG per member.
    pub fn set_params(&mut self, epoch_length: U256, member_cap: U256) -> Result<(), PoolError> {
        self.only_owner()?;
        if self.phase.get() != U256::ZERO {
            return Err(PoolError::Busy(Busy {}));
        }
        if epoch_length < U256::from(28u64 * 86_400) || epoch_length > U256::from(31u64 * 86_400) || member_cap > U256::from(100_000u128 * USDG_UNIT) {
            return Err(PoolError::BadInput(BadInput {}));
        }
        self.epoch_length.set(epoch_length);
        self.member_cap.set(member_cap);
        Ok(())
    }

    pub fn transfer_ownership(&mut self, to: Address) -> Result<(), PoolError> {
        self.only_owner()?;
        self.owner.set(to);
        Ok(())
    }

    /// Opens an account for an annuitant (whose passkey and guardians go to the LifeRegistry).
    /// `key` = (isoNumeric·2 + sex)·10000 + birthYear; `beta_bps` is the bequest share;
    /// `escalating` picks the plan whose income starts lower and grows (docs/protocol.md §4.1).
    #[allow(clippy::too_many_arguments)]
    pub fn join(
        &mut self,
        key: U256,
        start_age: u64,
        beta_bps: u64,
        escalating: bool,
        payout: Address,
        beneficiary: Address,
        qx: FixedBytes<32>,
        qy: FixedBytes<32>,
        guardians: [Address; 3],
    ) -> Result<U256, PoolError> {
        self.not_paused()?;
        if !(50..=80).contains(&start_age) || beta_bps > 10_000 || payout == Address::ZERO || beneficiary == Address::ZERO {
            return Err(PoolError::BadInput(BadInput {}));
        }
        // The key must be priced by the Actuary (a known country, sex and birth year).
        let age = U256::from(self.age_wad(key)?);
        let actuary = IActuary::new(self.actuary.get());
        ext(actuary.q_month(self.vm(), Call::new(), key, age, U256::from(self.year_wad())))?;

        let id = self.member_count.get();
        self.member_count.set(id + U256::from(1u8));
        let t = self.cohort_id(key, false, start_age, escalating);
        let b = self.cohort_id(key, true, start_age, escalating);
        self.m_t.insert(id, U256::from(t));
        self.m_b.insert(id, U256::from(b));
        self.m_beta.insert(id, U256::from(beta_bps));

        let registry = ILifeRegistry::new(self.registry.get());
        let ctx = Call::new_mutating(self);
        // The registry keeps the cohort key (no income or exit until an identity adapter confirms
        // it), the passkey, the guardians, and the payout and beneficiary addresses.
        ext(registry.enroll(self.vm(), ctx, id, key, qx, qy, payout, beneficiary, guardians))?;
        self.vm().log(Joined { memberId: id, key, startAge: U256::from(start_age), betaBps: U256::from(beta_bps), escalating });
        Ok(id)
    }

    /// Pays USDG in for a member (anyone may pay: a daughter funds her mother). Invested at the
    /// next settlement's fresh NAV. Only once the member's identity is verified: money that went in
    /// before could never come out, since income and exits need a verified identity.
    pub fn contribute(&mut self, member_id: U256, amount: U256) -> Result<(), PoolError> {
        self.not_paused()?;
        if member_id >= self.member_count.get() || amount < U256::from(MIN_CONTRIBUTION) {
            return Err(PoolError::BadInput(BadInput {}));
        }
        let phase = self.phase.get().to::<u8>();
        if phase != PH_IDLE && phase < RB_PLAN {
            return Err(PoolError::Busy(Busy {}));
        }
        let flags = self.m_flags.get(member_id).to::<u64>();
        if flags & (FLAG_RELEASED | FLAG_QUEUED_DEAD | FLAG_EXIT_REQUESTED | FLAG_QUEUED_EXIT) != 0 {
            return Err(PoolError::Released(Released {}));
        }
        let registry = ILifeRegistry::new(self.registry.get());
        if !ext(registry.identified(self.vm(), Call::new(), member_id))? {
            return Err(PoolError::NotEligible(NotEligible {}));
        }
        // Nor for a member the registry says has died, even before the pool has released her.
        let s = ext(registry.status(self.vm(), Call::new(), member_id))?;
        if s == STATUS_DECEASED || s == STATUS_PRESUMED {
            return Err(PoolError::NotEligible(NotEligible {}));
        }
        let total = self.m_contributed.get(member_id) + amount;
        if total > self.member_cap.get() {
            return Err(PoolError::CapExceeded(CapExceeded {}));
        }
        let (sender, treasury) = (self.vm().msg_sender(), self.treasury.get());
        let token = IERC20::new(self.usdg.get());
        let ctx = Call::new_mutating(self);
        if !ext(token.transfer_from(self.vm(), ctx, sender, treasury, amount))? {
            return Err(PoolError::External(External {}));
        }
        self.m_contributed.insert(member_id, total);
        self.m_pending.insert(member_id, self.m_pending.get(member_id) + amount);
        self.pending_total.set(self.pending_total.get() + amount);
        if flags & FLAG_QUEUED_DEPOSIT == 0 {
            self.m_flags.insert(member_id, U256::from(flags | FLAG_QUEUED_DEPOSIT));
            self.pending_members.push(member_id);
        }
        self.vm().log(Contributed { memberId: member_id, payer: sender, usdg: amount });
        Ok(())
    }

    /// Queues a member whose death the LifeRegistry has made final. Anyone may call.
    pub fn mark_dead(&mut self, member_id: U256) -> Result<(), PoolError> {
        let flags = self.m_flags.get(member_id).to::<u64>();
        if flags & (FLAG_QUEUED_DEAD | FLAG_RELEASED) != 0 {
            return Err(PoolError::Released(Released {}));
        }
        let registry = ILifeRegistry::new(self.registry.get());
        let s = ext(registry.status(self.vm(), Call::new(), member_id))?;
        if s != STATUS_DECEASED && s != STATUS_PRESUMED {
            return Err(PoolError::NotEligible(NotEligible {}));
        }
        self.m_flags.insert(member_id, U256::from(flags | FLAG_QUEUED_DEAD));
        self.pending_deaths.push(member_id);
        self.vm().log(DeathQueued { memberId: member_id });
        Ok(())
    }

    /// Gives notice to leave (docs/protocol.md §7). Only the member's own payout address may, only
    /// while saving, and only if the notice ends before income would start: paying members can't
    /// exit, as with an annuity. The member stays at risk, and keeps earning credits, until then.
    pub fn request_exit(&mut self, member_id: U256) -> Result<U256, PoolError> {
        self.runs_allowed()?;
        if self.vm().msg_sender() != self.payout_of(member_id)? {
            return Err(PoolError::Unauthorized(Unauthorized {}));
        }
        let flags = self.m_flags.get(member_id).to::<u64>();
        if flags & (FLAG_RELEASED | FLAG_QUEUED_DEAD | FLAG_EXIT_REQUESTED | FLAG_QUEUED_EXIT) != 0 {
            return Err(PoolError::Released(Released {}));
        }
        let t = self.m_t.get(member_id);
        let start = U256::from(self.c_meta.get(t).to::<u64>() >> 8) * U256::from(WAD);
        let at = self.now() + u128_of(self.exit_notice.get())?;
        if U256::from(age_wad_at(self.c_key.get(t), at)?) >= start {
            return Err(PoolError::NotEligible(NotEligible {}));
        }
        self.m_exit_at.insert(member_id, U256::from(at));
        self.m_flags.insert(member_id, U256::from(flags | FLAG_EXIT_REQUESTED));
        self.vm().log(ExitRequested { memberId: member_id, executableAt: U256::from(at) });
        Ok(U256::from(at))
    }

    /// Withdraws a notice before it's executed; the member simply stays.
    pub fn cancel_exit(&mut self, member_id: U256) -> Result<(), PoolError> {
        if self.vm().msg_sender() != self.payout_of(member_id)? {
            return Err(PoolError::Unauthorized(Unauthorized {}));
        }
        let flags = self.m_flags.get(member_id).to::<u64>();
        if flags & FLAG_EXIT_REQUESTED == 0 || flags & FLAG_QUEUED_EXIT != 0 {
            return Err(PoolError::BadInput(BadInput {}));
        }
        self.m_flags.insert(member_id, U256::from(flags & !FLAG_EXIT_REQUESTED));
        self.vm().log(ExitCancelled { memberId: member_id });
        Ok(())
    }

    /// Queues an exit once the notice has run. Anyone may call, but only while the LifeRegistry
    /// says the member is alive and checked in: nobody can cash out a dead member's at-risk money.
    /// A death found later still wins; the queue releases each member once. The notice must be
    /// used within 90 days of maturing, and never once income has started: a notice is not an
    /// option to hold until one falls ill.
    pub fn exit(&mut self, member_id: U256) -> Result<(), PoolError> {
        self.runs_allowed()?;
        let flags = self.m_flags.get(member_id).to::<u64>();
        if flags & FLAG_EXIT_REQUESTED == 0 || flags & (FLAG_QUEUED_EXIT | FLAG_QUEUED_DEAD | FLAG_RELEASED) != 0 {
            return Err(PoolError::BadInput(BadInput {}));
        }
        let (now, at) = (self.now(), u128_of(self.m_exit_at.get(member_id))?);
        if now < at {
            return Err(PoolError::TooEarly(TooEarly {}));
        }
        let t = self.m_t.get(member_id);
        let start = U256::from(self.c_meta.get(t).to::<u64>() >> 8) * U256::from(WAD);
        if now > at + EXIT_WINDOW || U256::from(age_wad_at(self.c_key.get(t), now)?) >= start {
            return Err(PoolError::NotEligible(NotEligible {}));
        }
        let registry = ILifeRegistry::new(self.registry.get());
        if !ext(registry.can_receive_income(self.vm(), Call::new(), member_id))? {
            return Err(PoolError::NotEligible(NotEligible {}));
        }
        self.not_held(member_id)?;
        self.m_flags.insert(member_id, U256::from(flags | FLAG_QUEUED_EXIT));
        self.pending_deaths.push(member_id);
        self.vm().log(ExitQueued { memberId: member_id });
        Ok(())
    }

    pub fn set_guardian(&mut self, guardian: Address) -> Result<(), PoolError> {
        self.only_owner()?;
        self.guardian.set(guardian);
        self.vm().log(GuardianSet { guardian });
        Ok(())
    }

    /// Emergency brake: stops joins and deposits until unpaused, and holds exits and the start of
    /// settlements and rebalances for at most 7 days. Claims, withdrawals and death reports go on,
    /// and it moves no money. At most one pause in 30 days, by the guardian or the owner.
    pub fn pause(&mut self) -> Result<(), PoolError> {
        let sender = self.vm().msg_sender();
        if sender != self.guardian.get() && sender != self.owner.get() {
            return Err(PoolError::Unauthorized(Unauthorized {}));
        }
        if self.paused.get() {
            return Ok(()); // already paused: the 7-day hold keeps counting from the first pause
        }
        // At most one pause in 30 days, whoever asks (skeptic review 3: the owner could re-pause
        // every few days and hold settlements forever).
        let last = u128_of(self.paused_at.get())?;
        if last != 0 && self.now() < last + PAUSE_COOLDOWN {
            return Err(PoolError::TooEarly(TooEarly {}));
        }
        self.paused_at.set(U256::from(self.now()));
        self.paused.set(true);
        self.vm().log(PauseSet { paused: true, by: sender });
        Ok(())
    }

    pub fn unpause(&mut self) -> Result<(), PoolError> {
        self.only_owner()?;
        self.paused.set(false);
        let sender = self.vm().msg_sender();
        self.vm().log(PauseSet { paused: false, by: sender });
        Ok(())
    }

    /// The detector's state for a group (country × birth decade): its running log-likelihood
    /// ratios (raw Q64.64), when it was last flagged for hidden deaths, and the last epoch counted.
    pub fn group_state(&self, group: U256) -> (alloy_primitives::I256, alloy_primitives::I256, U256, U256) {
        (self.g_low.get(group), self.g_high.get(group), self.g_flagged_at.get(group), self.g_updated.get(group))
    }

    pub fn group_of_member(&self, member_id: U256) -> U256 {
        group_of(self.c_key.get(self.m_t.get(member_id)))
    }

    pub fn is_paused(&self) -> bool {
        self.paused.get()
    }

    /// Queues the repayment of a member whose death, reported or presumed, was undone
    /// (`LifeRegistry.revive`). Anyone may call, once the registry says they are alive. The next
    /// settlement sells their released at-risk shares from the revival reserve (as far as it goes)
    /// and the one after invests the proceeds back into their account.
    pub fn restore(&mut self, member_id: U256) -> Result<(), PoolError> {
        let flags = self.m_flags.get(member_id).to::<u64>();
        // Released by a death and not yet repaid, or repaid in part and still owed.
        let first = flags & FLAG_RELEASED != 0 && flags & (FLAG_EXITED | FLAG_RESTORED) == 0;
        if !(first || flags & FLAG_OWED != 0) || flags & FLAG_QUEUED_RESTORE != 0 {
            return Err(PoolError::BadInput(BadInput {}));
        }
        // Alive now, by the registry (which clears a revival whenever a death becomes final): a
        // revival from an earlier, false death can never repay a real one.
        let registry = ILifeRegistry::new(self.registry.get());
        let s = ext(registry.status(self.vm(), Call::new(), member_id))?;
        if s == 0 || s == STATUS_DECEASED || s == STATUS_PRESUMED || ext(registry.revived_at(self.vm(), Call::new(), member_id))? == 0 {
            return Err(PoolError::NotEligible(NotEligible {}));
        }
        self.m_flags.insert(member_id, U256::from(flags | FLAG_QUEUED_RESTORE));
        self.pending_deaths.push(member_id);
        self.vm().log(RestoreQueued { memberId: member_id });
        Ok(())
    }

    /// Pays a reported death's held estate to the beneficiary, and its post-death income to the
    /// survivors, once `ESTATE_HOLD` has passed and the registry still says the member is dead.
    /// A revived member's estate comes back with her `restore` instead. Anyone may call.
    pub fn pay_estate(&mut self, member_id: U256) -> Result<(), PoolError> {
        if self.phase.get() != U256::ZERO {
            return Err(PoolError::Busy(Busy {}));
        }
        let usdg = self.m_estate.get(member_id);
        let post = self.m_post.get(member_id);
        if usdg == U256::ZERO && post == U256::ZERO {
            return Err(PoolError::BadInput(BadInput {}));
        }
        let registry = ILifeRegistry::new(self.registry.get());
        let s = ext(registry.status(self.vm(), Call::new(), member_id))?;
        if s != STATUS_DECEASED && s != STATUS_PRESUMED {
            return Err(PoolError::NotEligible(NotEligible {})); // alive: it returns with her restore
        }
        if self.now() < u128_of(self.m_estate_at.get(member_id))? + ESTATE_HOLD {
            return Err(PoolError::TooEarly(TooEarly {}));
        }
        let heir = self.beneficiary_of(member_id)?;
        self.claimable.insert(heir, self.claimable.get(heir) + usdg);
        self.vm().log(EstatePaid { memberId: member_id, to: heir, usdg });
        // Her income after the date of death: the survivors', credited at the next settlement.
        self.unallocated_usdg.set(self.unallocated_usdg.get() + post);
        self.m_estate.insert(member_id, U256::ZERO);
        self.m_post.insert(member_id, U256::ZERO);
        Ok(())
    }

    /// A reported death's estate still held (USDG), when the hold began, and the income dated
    /// after the death held with it (USDG).
    pub fn estate_of(&self, member_id: U256) -> (U256, U256, U256) {
        (self.m_estate.get(member_id), self.m_estate_at.get(member_id), self.m_post.get(member_id))
    }

    /// The revival reserve's shares, per sleeve.
    pub fn revival_reserve(&self) -> (U256, U256, U256) {
        (self.reserve.get(0usize).unwrap_or_default(), self.reserve.get(1usize).unwrap_or_default(), self.reserve.get(2usize).unwrap_or_default())
    }

    /// Bounded: 180 to 730 days.
    pub fn set_exit_notice(&mut self, notice: U256) -> Result<(), PoolError> {
        self.only_owner()?;
        if notice < U256::from(180 * DAY) || notice > U256::from(730 * DAY) {
            return Err(PoolError::BadInput(BadInput {}));
        }
        self.exit_notice.set(notice);
        Ok(())
    }

    /// Governance, for anyone to verify: after deployment the owner is the 48-hour timelock.
    pub fn owner(&self) -> Address {
        self.owner.get()
    }

    pub fn guardian(&self) -> Address {
        self.guardian.get()
    }

    pub fn exit_notice(&self) -> U256 {
        self.exit_notice.get()
    }

    /// Settles the epoch in one transaction, for pools small enough to fit one block. Larger
    /// pools use `settle_steps`; the result is identical.
    pub fn settle(&mut self) -> Result<(), PoolError> {
        self.settle_steps(u32::MAX)?;
        Ok(())
    }

    /// Monthly settlement in pages (docs/protocol.md §5.1). Anyone may call. If no settlement is
    /// running, starts one (once an epoch has elapsed, with fresh prices), then does up to
    /// `budget` units of work: a death, a cohort or a deposit each. Returns true when the epoch
    /// is settled. Contributions and parameter changes wait while a settlement runs.
    pub fn settle_steps(&mut self, budget: u32) -> Result<bool, PoolError> {
        let mut phase = self.phase.get().to::<u8>();
        if phase >= RB_PLAN {
            return Err(PoolError::Busy(Busy {}));
        }
        if phase == PH_IDLE {
            // A pause holds new runs for up to a week; one already under way may finish.
            self.runs_allowed()?;
            self.begin_settlement()?;
            phase = PH_RELEASE;
        }
        let mut run = self.load_run()?;
        let epoch = self.epoch.get() + U256::from(1u8);
        let (d_head, d_end) = (self.deaths_head.get().to::<u64>(), self.run_deaths_end.get().to::<u64>());
        let (m_head, m_end) = (self.members_head.get().to::<u64>(), self.run_members_end.get().to::<u64>());
        let n = self.run_cohorts.get().to::<u64>();
        let mut cursor = self.cursor.get().to::<u64>();
        let mut left = budget;
        while left > 0 && phase != PH_IDLE {
            // Work arms do one item and advance; the others only change phase.
            let work = match phase {
                PH_RELEASE if cursor < d_end => self.release_one(cursor, epoch, &mut run).map(|_| true)?,
                PH_WEIGH if cursor < n => self.weigh_one(cursor, epoch, &mut run).map(|_| true)?,
                PH_CREDIT if cursor < n => self.credit_one(cursor, &mut run).map(|_| true)?,
                PH_BEQUEST if cursor < d_end => self.bequest_one(cursor, &mut run).map(|_| true)?,
                PH_BOOK if cursor < n => self.book_one(cursor, epoch, &mut run).map(|_| true)?,
                PH_MINT if cursor < m_end => self.mint_one(cursor, &run).map(|_| true)?,
                PH_RELEASE => {
                    (phase, cursor) = (PH_WEIGH, 0);
                    false
                }
                PH_WEIGH => {
                    // Every weight is in: the concentration for the finite-pool correction.
                    run.sum_sq = engine(run.wsq.share(run.weight))?;
                    (phase, cursor) = (PH_CREDIT, 0);
                    false
                }
                PH_CREDIT => {
                    (phase, cursor) = (PH_SELL, 0);
                    false
                }
                PH_SELL => {
                    self.sell_all(&mut run)?;
                    (phase, cursor) = (PH_BEQUEST, d_head);
                    left -= 1;
                    false
                }
                PH_BEQUEST => {
                    (phase, cursor) = (PH_BOOK, 0);
                    false
                }
                PH_BOOK => {
                    self.close_books(&mut run);
                    (phase, cursor) = (PH_INVEST, 0);
                    false
                }
                PH_INVEST => {
                    self.invest(&mut run)?;
                    (phase, cursor) = (PH_MINT, m_head);
                    left -= 1;
                    false
                }
                PH_MINT => {
                    self.finish_settlement(&run, epoch);
                    (phase, cursor) = (PH_IDLE, 0);
                    false
                }
                _ => return Err(PoolError::BadInput(BadInput {})),
            };
            if work {
                cursor += 1;
                left -= 1;
            }
        }
        self.store_run(&run);
        self.phase.set(U256::from(phase));
        self.cursor.set(U256::from(cursor));
        Ok(phase == PH_IDLE)
    }

    /// Rebalances in one transaction; large pools use `rebalance_steps`.
    pub fn rebalance(&mut self) -> Result<(), PoolError> {
        self.rebalance_steps(u32::MAX)?;
        Ok(())
    }

    /// Moves each cohort toward its glide-path mix (docs/protocol.md §8), in pages. Once per
    /// epoch, after settlement, with fresh prices. Opposite wishes cross inside the pool at the
    /// oracle price; only the net trades, and every fill is bounded by the Treasury's oracle check.
    pub fn rebalance_steps(&mut self, budget: u32) -> Result<bool, PoolError> {
        let mut phase = self.phase.get().to::<u8>();
        if phase != PH_IDLE && phase < RB_PLAN {
            return Err(PoolError::Busy(Busy {}));
        }
        let epoch = self.epoch.get();
        if phase == PH_IDLE {
            self.runs_allowed()?;
            if epoch == U256::ZERO || self.rebalanced_epoch.get() == epoch {
                return Err(PoolError::TooEarly(TooEarly {}));
            }
            let treasury = ITreasury::new(self.treasury.get());
            let p = ext(treasury.prices(self.vm(), Call::new()))?;
            fx_set3(&mut self.run_prices, &[fx_of_wad(p[0])?, fx_of_wad(p[1])?, fx_of_wad(p[2])?]);
            self.run_ts.set(U256::from(self.now()));
            self.run_cohorts.set(self.cohort_count.get());
            for a in [&mut self.rb_gave, &mut self.rb_want, &mut self.rb_given, &mut self.rb_bought] {
                set3(a, &[0; SLEEVES]);
            }
            self.rb_usdg.set(U256::ZERO);
            self.cursor.set(U256::ZERO);
            phase = RB_PLAN;
        }
        let prices = fx_get3(&self.run_prices)?;
        let (mut gave, mut want) = (get3(&self.rb_gave)?, fx_get3(&self.rb_want)?);
        let (mut pot, mut given) = (get3(&self.rb_pot)?, get3(&self.rb_given)?);
        let ts = u128_of(self.run_ts.get())?;
        let threshold = engine(Fx::from_ratio(REBALANCE_THRESHOLD_PCT, 100))?;
        let n = self.run_cohorts.get().to::<u64>();
        let mut cursor = self.cursor.get().to::<u64>();
        let mut left = budget;
        while left > 0 && phase != PH_IDLE {
            match phase {
                RB_PLAN if cursor < n => {
                    let id = U256::from(cursor);
                    let c = self.cohort_at(id)?;
                    let (g, w) = if c.units == 0 {
                        ([0; SLEEVES], [Fx::ZERO; SLEEVES])
                    } else {
                        let age = engine(Fx::from_wad(age_wad_at(self.c_key.get(id), ts)? as i128))?;
                        engine(rebalance_step::plan(&c, age, &prices, threshold))?
                    };
                    for a in 0..SLEEVES {
                        gave[a] += g[a];
                        want[a] = engine(want[a].add(w[a]))?;
                        self.scratch.insert(slot(cursor, S_GIVE + a as u64), U256::from(g[a]));
                        self.scratch.insert(slot(cursor, S_WANT + a as u64), fx_u(w[a]));
                    }
                    cursor += 1;
                    left -= 1;
                }
                RB_PLAN => (phase, cursor) = (RB_TRADE, 0),
                RB_TRADE => {
                    let (crossed, sell, buy_value) = engine(rebalance_step::net(&gave, &want, &prices))?;
                    let bought = self.trade(&sell, &buy_value)?;
                    pot = core::array::from_fn(|a| crossed[a] + bought[a]);
                    given = [0; SLEEVES];
                    (phase, cursor) = (RB_APPLY, 0);
                    left -= 1;
                }
                RB_APPLY if cursor < n => {
                    let id = U256::from(cursor);
                    let mut c = self.cohort_at(id)?;
                    let mut g = [0u128; SLEEVES];
                    let mut w = [Fx::ZERO; SLEEVES];
                    for a in 0..SLEEVES {
                        g[a] = u128_of(self.scratch.get(slot(cursor, S_GIVE + a as u64)))?;
                        w[a] = u_fx(self.scratch.get(slot(cursor, S_WANT + a as u64)))?;
                    }
                    if g.iter().any(|&x| x > 0) || w.iter().any(|x| x.0 > 0) {
                        engine(rebalance_step::apply(&mut c, &g, &w, &pot, &want, &mut given))?;
                        self.put_shares(id, &c);
                    }
                    cursor += 1;
                    left -= 1;
                }
                RB_APPLY => {
                    // Whatever couldn't be assigned carries to next month's credits.
                    self.carry0.set(self.carry0.get() + U256::from(pot[0] - given[0]));
                    self.carry1.set(self.carry1.get() + U256::from(pot[1] - given[1]));
                    self.carry2.set(self.carry2.get() + U256::from(pot[2] - given[2]));
                    self.rebalanced_epoch.set(epoch);
                    let bought = get3(&self.rb_bought)?;
                    self.vm().log(Rebalanced { epoch, usdgSold: self.rb_usdg.get(), sgovBought: U256::from(bought[1]), spyBought: U256::from(bought[2]) });
                    (phase, cursor) = (PH_IDLE, 0);
                }
                _ => return Err(PoolError::BadInput(BadInput {})),
            }
        }
        set3(&mut self.rb_gave, &gave);
        fx_set3(&mut self.rb_want, &want);
        set3(&mut self.rb_pot, &pot);
        set3(&mut self.rb_given, &given);
        self.phase.set(U256::from(phase));
        self.cursor.set(U256::from(cursor));
        Ok(phase == PH_IDLE)
    }

    /// Abandons a rebalance that has been planning or trading for a day without finishing: a
    /// paused token or a dry pool must not stop everyone's income. Nothing has traded in those
    /// phases (the trade step is a single transaction), so discarding the run loses nothing; the
    /// epoch counts as rebalanced and the next one tries again. Anyone may call.
    pub fn abort_rebalance(&mut self) -> Result<(), PoolError> {
        let phase = self.phase.get().to::<u8>();
        if phase != RB_PLAN && phase != RB_TRADE {
            return Err(PoolError::BadInput(BadInput {}));
        }
        if self.now() < u128_of(self.run_ts.get())? + REBALANCE_TIMEOUT {
            return Err(PoolError::TooEarly(TooEarly {}));
        }
        let epoch = self.epoch.get();
        self.rebalanced_epoch.set(epoch);
        self.phase.set(U256::from(PH_IDLE));
        self.cursor.set(U256::ZERO);
        self.vm().log(RebalanceAborted { epoch });
        Ok(())
    }

    /// A member's bequest share (bps) and, if they gave notice, when their exit becomes possible.
    pub fn member_terms(&self, id: U256) -> (U256, U256) {
        (self.m_beta.get(id), self.m_exit_at.get(id))
    }

    /// Where a settlement or rebalance stands, for keepers: phase, cursor, cohorts covered,
    /// death queue [head, end), deposit queue [head, end).
    pub fn run_state(&self) -> (U256, U256, U256, U256, U256, U256, U256) {
        (
            self.phase.get(),
            self.cursor.get(),
            self.run_cohorts.get(),
            self.deaths_head.get(),
            self.run_deaths_end.get(),
            self.members_head.get(),
            self.run_members_end.get(),
        )
    }

    /// Income owed to a member now (USDG base units).
    pub fn owed(&self, member_id: U256) -> Result<U256, PoolError> {
        let (t, b) = (self.m_t.get(member_id), self.m_b.get(member_id));
        let part = |cohort: U256, units: U256, snap: U256| -> Result<u128, PoolError> {
            let now = self.c_ipu.get(cohort);
            if now <= snap {
                return Ok(0);
            }
            mul_div(u128_of(units)?, u128_of(now - snap)?, WAD).ok_or(PoolError::Engine(Engine { kind: 1 }))
        };
        let q = part(t, self.m_tu.get(member_id), self.m_tsnap.get(member_id))? + part(b, self.m_bu.get(member_id), self.m_bsnap.get(member_id))?;
        Ok(U256::from(usdg_raw(Fx(q as i128))))
    }

    /// Pays a living member's income to their payout address. Anyone may trigger it.
    pub fn claim(&mut self, member_id: U256) -> Result<U256, PoolError> {
        if self.m_flags.get(member_id).to::<u64>() & (FLAG_RELEASED | FLAG_QUEUED_DEAD) != 0 {
            return Err(PoolError::Released(Released {}));
        }
        let registry = ILifeRegistry::new(self.registry.get());
        if !ext(registry.can_receive_income(self.vm(), Call::new(), member_id))? {
            return Err(PoolError::NotEligible(NotEligible {}));
        }
        self.not_held(member_id)?;
        let amount = self.owed(member_id)?;
        let (t, b) = (self.m_t.get(member_id), self.m_b.get(member_id));
        self.m_tsnap.insert(member_id, self.c_ipu.get(t));
        self.m_bsnap.insert(member_id, self.c_ipu.get(b));
        if amount > U256::ZERO {
            let to = self.payout_of(member_id)?;
            let treasury = ITreasury::new(self.treasury.get());
            let ctx = Call::new_mutating(self);
            ext(treasury.pay(self.vm(), ctx, to, amount))?;
        }
        self.vm().log(IncomeClaimed { memberId: member_id, usdg: amount });
        Ok(amount)
    }

    /// Estates and beneficiaries withdraw what the pool owes them.
    pub fn withdraw(&mut self) -> Result<U256, PoolError> {
        let who = self.vm().msg_sender();
        let amount = self.claimable.get(who);
        if amount > U256::ZERO {
            self.claimable.insert(who, U256::ZERO);
            let treasury = ITreasury::new(self.treasury.get());
            let ctx = Call::new_mutating(self);
            ext(treasury.pay(self.vm(), ctx, who, amount))?;
        }
        Ok(amount)
    }

    pub fn withdraw_fees(&mut self, to: Address) -> Result<U256, PoolError> {
        self.only_owner()?;
        let amount = self.protocol_usdg.get();
        self.protocol_usdg.set(U256::ZERO);
        if amount > U256::ZERO {
            let treasury = ITreasury::new(self.treasury.get());
            let ctx = Call::new_mutating(self);
            ext(treasury.pay(self.vm(), ctx, to, amount))?;
        }
        Ok(amount)
    }

    // ------------------------------------------------------------------ views

    pub fn epoch_info(&self) -> (U256, U256, U256) {
        (self.epoch.get(), self.last_settle.get(), self.epoch_length.get())
    }

    pub fn cohort(&self, id: U256) -> (U256, U256, U256, U256, U256, U256, U256) {
        (self.c_key.get(id), self.c_meta.get(id), self.c_s0.get(id), self.c_s1.get(id), self.c_s2.get(id), self.c_units.get(id), self.c_ipu.get(id))
    }

    pub fn member(&self, id: U256) -> (U256, U256, U256, U256, U256, U256, U256) {
        (self.m_t.get(id), self.m_b.get(id), self.m_tu.get(id), self.m_bu.get(id), self.m_pending.get(id), self.m_contributed.get(id), self.m_flags.get(id))
    }

    pub fn counts(&self) -> (U256, U256) {
        (self.member_count.get(), self.cohort_count.get())
    }

    pub fn claimable_of(&self, who: Address) -> U256 {
        self.claimable.get(who)
    }

    /// A member's current value in USDG (WAD) at the given prices, for the SDK.
    pub fn member_value(&self, id: U256, p0: U256, p1: U256, p2: U256) -> Result<U256, PoolError> {
        let prices = [fx_of_wad(p0)?, fx_of_wad(p1)?, fx_of_wad(p2)?];
        let mut total = Fx::ZERO;
        for (cohort, units) in [(self.m_t.get(id), self.m_tu.get(id)), (self.m_b.get(id), self.m_bu.get(id))] {
            let cu = self.c_units.get(cohort);
            if cu == U256::ZERO {
                continue;
            }
            let shares = [u128_of(self.c_s0.get(cohort))?, u128_of(self.c_s1.get(cohort))?, u128_of(self.c_s2.get(cohort))?];
            let v = engine(value(&shares, &prices))?;
            let mine = mul_div(v.0 as u128, u128_of(units)?, u128_of(cu)?).unwrap_or(0);
            total = engine(total.add(Fx(mine as i128)))?;
        }
        wad_of_fx(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actuary_core::ledger::{CohortMonth, Epoch, Release};
    use alloy_sol_types::{SolCall, SolValue};
    use stylus_sdk::testing::*;

    // Call structs for encoding exact calldata (sol_interface! generates call wrappers, not these).
    alloy_sol_types::sol! {
        interface Abi {
            function prices() external view returns (uint256[3] memory);
            function depositCash(uint256 usdgAmount) external returns (uint256);
            function sell(uint256[3] calldata shares) external returns (uint256);
            function pay(address to, uint256 usdgAmount) external;
            function enroll(uint256 memberId, uint256 key, bytes32 qx, bytes32 qy, address payout, address beneficiary, address[3] calldata guardians) external;
            function payoutOf(uint256 memberId) external view returns (address);
            function beneficiaryOf(uint256 memberId) external view returns (address);
            function canReceiveIncome(uint256 memberId) external view returns (bool);
            function qMonth(uint256 key, uint256 age, uint256 year) external view returns (uint256);
            function payoutFraction(uint256 key, uint256 age, uint256 year, bool escalating) external view returns (uint256);
            function transferFrom(address from, address to, uint256 amount) external returns (bool);
            function status(uint256 memberId) external view returns (uint8);
            function buy(uint8 sleeve, uint256 usdgIn) external returns (uint256);
            function dateOfDeath(uint256 memberId) external view returns (uint64);
            function lastStrong(uint256 memberId) external view returns (uint64);
            function reporterOf(uint256 memberId) external view returns (address);
            function identified(uint256 memberId) external view returns (bool);
            function revivedAt(uint256 memberId) external view returns (uint64);
        }
    }

    fn load_cohort(pool: &TontiPool, id: u64, bequest: bool) -> Cohort {
        let (_, _, s0, s1, s2, units, ipu) = pool.cohort(U256::from(id));
        Cohort {
            class: if bequest { Class::Bequest } else { Class::Tontine },
            shares: [s0.to::<u128>(), s1.to::<u128>(), s2.to::<u128>()],
            units: units.to::<u128>(),
            income_per_unit: Fx(ipu.to::<u128>() as i128),
        }
    }

    const TREASURY: Address = Address::new([0x11; 20]);
    const REGISTRY: Address = Address::new([0x22; 20]);
    const ACTUARY: Address = Address::new([0x33; 20]);
    const USDG: Address = Address::new([0x44; 20]);
    const MARIA: Address = Address::new([0x55; 20]);
    const MOTHER_PAYOUT: Address = Address::new([0x66; 20]);
    const HEIR: Address = Address::new([0x77; 20]);
    const T0: u64 = 1_790_460_000; // 2026-09-26
    // WAD prices: steakUSDG share 1.008, SGOV 101.17, SPY 772.33.
    fn prices() -> [U256; 3] {
        [U256::from(1_008_000_000_000_000_000u128), U256::from(101_170_000_000_000_000_000u128), U256::from(772_330_000_000_000_000_000u128)]
    }
    fn key(birth: u64) -> U256 {
        U256::from((608u64 * 2) * 10_000 + birth) // Philippines, female
    }
    fn year_age(ts: u64, birth: u64) -> (U256, U256) {
        let year = 1970 * WAD + mul_div(ts as u128, WAD, YEAR_SECONDS).unwrap();
        (U256::from(year), U256::from(year - (birth as u128 * WAD + WAD / 2)))
    }

    fn setup() -> (TestVM, TontiPool) {
        set_fallback(None);
        let vm = TestVM::new();
        vm.set_block_timestamp(T0);
        vm.set_sender(MARIA);
        let mut pool = TontiPool::from(&vm);
        pool.init(TREASURY, REGISTRY, ACTUARY, USDG).unwrap();
        (vm, pool)
    }

    fn mock_join(vm: &TestVM, ts: u64, birth: u64, id: u64, payout: Address) {
        let (year, age) = year_age(ts, birth);
        let q = Abi::qMonthCall { key: key(birth), age, year }.abi_encode();
        vm.mock_static_call(ACTUARY, q, Ok(U256::from(100_000_000_000_000u128).abi_encode()));
        let enroll = Abi::enrollCall {
            memberId: U256::from(id),
            key: key(birth),
            qx: FixedBytes::ZERO,
            qy: FixedBytes::ZERO,
            payout,
            beneficiary: HEIR,
            guardians: [Address::ZERO; 3],
        }
        .abi_encode();
        vm.mock_call(REGISTRY, enroll, U256::ZERO, Ok(Vec::new()));
        // Alive (Active) unless a test says otherwise: deposits check it.
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: U256::from(id) }.abi_encode(), Ok(U256::from(1u8).abi_encode()));
        // The registry keeps the payout and beneficiary addresses; the pool reads them there.
        vm.mock_static_call(REGISTRY, Abi::payoutOfCall { memberId: U256::from(id) }.abi_encode(), Ok(payout.abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::beneficiaryOfCall { memberId: U256::from(id) }.abi_encode(), Ok(HEIR.abi_encode()));
        // Tests fund members after their identity check; `contributions_wait_for_a_verified_identity` covers the gate.
        vm.mock_static_call(REGISTRY, Abi::identifiedCall { memberId: U256::from(id) }.abi_encode(), Ok(true.abi_encode()));
    }

    fn mock_contribute(vm: &TestVM, payer: Address, amount: u128) {
        let tf = Abi::transferFromCall { from: payer, to: TREASURY, amount: U256::from(amount) }.abi_encode();
        vm.mock_call(USDG, tf, U256::ZERO, Ok(true.abi_encode()));
    }

    #[derive(Clone, Copy)]
    enum Out {
        Death,
        Exit,
    }

    /// What one month's settlement must do, from the stored state, step by step in the pool's
    /// order: every cohort's fee; each release (a death puts 5% of its at-risk release in the
    /// revival reserve, an exit hands 99% of it to the member); then weights, fair credits and
    /// income sales. Built only from actuary-core's tested steps.
    struct Month {
        cohorts: Vec<Cohort>,
        fees: Shares,
        claims: Shares,
        credited: Shares,
        to_sell: Shares,
        reserved: Shares,
        carry: Shares,
    }

    impl Month {
        fn sale(&self) -> Shares {
            core::array::from_fn(|a| self.to_sell[a] + self.claims[a] + self.fees[a])
        }
    }

    fn ledger_month(pool: &TontiPool, start: Shares, rates: &dyn Fn(usize, &Cohort) -> (Fx, Fx), releases: &[(U256, Out)], claims0: Shares) -> Month {
        let p = prices();
        let fxp = [fx_of_wad(p[0]).unwrap(), fx_of_wad(p[1]).unwrap(), fx_of_wad(p[2]).unwrap()];
        let n = pool.counts().1.to::<u64>();
        let mut c: Vec<Cohort> = (0..n).map(|i| load_cohort(pool, i, pool.cohort(U256::from(i)).1.to::<u64>() & 1 == 1)).collect();
        let fee = Fx::from_ratio(30, 120_000).unwrap();
        let (mut fees, mut pl, mut claims, mut reserved) = ([0u128; SLEEVES], start, claims0, [0u128; SLEEVES]);
        for x in c.iter_mut() {
            step::fee(x, fee, &mut fees).unwrap();
        }
        for &(m, out) in releases {
            let (t, b, tu, bu, ..) = pool.member(m);
            let before = pl;
            step::release(&mut c[t.to::<usize>()], tu.to::<u128>(), &mut pl, &mut claims).unwrap();
            for a in 0..SLEEVES {
                let released = pl[a] - before[a];
                match out {
                    Out::Death => {
                        let x = mul_div(released, 500, 10_000).unwrap();
                        pl[a] -= x;
                        reserved[a] += x;
                    }
                    Out::Exit => {
                        let keep = released - released / 100;
                        pl[a] -= keep;
                        claims[a] += keep;
                    }
                }
            }
            step::release(&mut c[b.to::<usize>()], bu.to::<u128>(), &mut pl, &mut claims).unwrap();
        }
        let r: Vec<(Fx, Fx)> = c.iter().enumerate().map(|(k, x)| rates(k, x)).collect();
        let ws: Vec<u128> = c.iter().enumerate().map(|(k, x)| step::weight(x, r[k].0, &fxp).unwrap()).collect();
        let (mut total, mut sq) = (0u128, SumSq::default());
        for &w in &ws {
            total += w;
            sq.add(w).unwrap();
        }
        let sum_sq = sq.share(total).unwrap();
        let (mut credited, mut to_sell) = ([0u128; SLEEVES], [0u128; SLEEVES]);
        for (k, x) in c.iter_mut().enumerate() {
            step::credit_and_sell(x, ws[k], &pl, total, sum_sq, r[k].1, &mut credited, &mut to_sell).unwrap();
        }
        let carry = core::array::from_fn(|a| pl[a] - credited[a]);
        Month { cohorts: c, fees, claims, credited, to_sell, reserved, carry }
    }

    fn carry_of(pool: &TontiPool) -> Shares {
        [pool.carry0.get().to::<u128>(), pool.carry1.get().to::<u128>(), pool.carry2.get().to::<u128>()]
    }

    #[test]
    fn contributions_are_forward_priced_into_units_split_by_bequest_share() {
        let (vm, mut pool) = setup();
        mock_join(&vm, T0, 1990, 0, MARIA);
        let id = pool.join(key(1990), 60, 2_000, false, MARIA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        assert_eq!(id, U256::ZERO);
        mock_contribute(&vm, MARIA, 25_000_000);
        pool.contribute(id, U256::from(25_000_000u64)).unwrap();
        // Over the research-preview cap of 25 USDG.
        assert!(pool.contribute(id, U256::from(1u64)).is_err());

        // First settlement: nothing held yet, so no actuary calls or sales; the 25 USDG are invested.
        vm.mock_static_call(TREASURY, Abi::pricesCall {}.abi_encode(), Ok(prices().abi_encode()));
        let minted = U256::from(24_801_587_301_587_301_587u128); // 25 / 1.008
        vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(25_000_000u64) }.abi_encode(), U256::ZERO, Ok(minted.abi_encode()));
        pool.settle().unwrap();

        let (t, b, tu, bu, pending, contributed, _) = pool.member(id);
        assert_eq!((pending, contributed), (U256::ZERO, U256::from(25_000_000u64)));
        // 80% tontine / 20% bequest, 1 unit = 1 USDG for a new cohort (truncation favors the pool).
        let tu_usdg = tu.to::<u128>() as f64 / 1e18;
        let bu_usdg = bu.to::<u128>() as f64 / 1e18;
        assert!((tu_usdg - 20.0).abs() < 1e-9 && (bu_usdg - 5.0).abs() < 1e-9, "units {tu_usdg} {bu_usdg}");
        let (_, _, s0t, _, _, ut, _) = pool.cohort(t);
        let (_, _, s0b, _, _, ub, _) = pool.cohort(b);
        assert_eq!(s0t + s0b, minted, "every minted share is owned by a cohort");
        assert_eq!((ut, ub), (tu, bu));
        assert_eq!(pool.epoch_info().0, U256::from(1u8));
        // A second settlement before the epoch elapses is refused.
        assert!(pool.settle().is_err());
    }

    #[test]
    fn paying_cohort_sells_its_payout_fraction_and_only_the_living_are_paid() {
        let (vm, mut pool) = setup();
        mock_join(&vm, T0, 1966, 0, MOTHER_PAYOUT);
        let id = pool.join(key(1966), 55, 0, false, MOTHER_PAYOUT, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        mock_contribute(&vm, MARIA, 25_000_000);
        pool.contribute(id, U256::from(25_000_000u64)).unwrap();
        vm.mock_static_call(TREASURY, Abi::pricesCall {}.abi_encode(), Ok(prices().abi_encode()));
        let minted = 24_801_587_301_587_301_587u128;
        vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(25_000_000u64) }.abi_encode(), U256::ZERO, Ok(U256::from(minted).abi_encode()));
        pool.settle().unwrap();

        // One month later: she is 60, past her start age of 55, so the cohort pays.
        let t1 = T0 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t1);
        let (year, age) = year_age(t1, 1966);
        let q_wad = U256::from(600_000_000_000_000u128); // 0.0006
        let p_wad = U256::from(4_000_000_000_000_000u128); // 0.4% of value this month
        vm.mock_static_call(ACTUARY, Abi::qMonthCall { key: key(1966), age, year }.abi_encode(), Ok(q_wad.abi_encode()));
        vm.mock_static_call(ACTUARY, Abi::payoutFractionCall { key: key(1966), age, year, escalating: false }.abi_encode(), Ok(p_wad.abi_encode()));

        // The sale the ledger must order: fee (0.30%/yr ÷ 12) then the 0.4% payout, in cash shares.
        let fee = mul_div(minted, Fx::from_ratio(30, 120_000).unwrap().0 as u128, Q64).unwrap();
        let after_fee = minted - fee;
        let p = Fx::from_wad(p_wad.to::<u128>() as i128).unwrap();
        let payout = mul_div(after_fee, p.0 as u128, Q64).unwrap();
        let sell = [U256::from(fee + payout), U256::ZERO, U256::ZERO];
        let usdg_back = U256::from((fee + payout) * 1_008 / 1_000 / 1_000_000_000_000); // at the vault price
        vm.mock_call(TREASURY, Abi::sellCall { shares: sell }.abi_encode(), U256::ZERO, Ok(usdg_back.abi_encode()));
        // Rounding leftovers are reinvested for survivors; accept whatever tiny amount it is.
        for leftover in 0u64..8 {
            vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(leftover) }.abi_encode(), U256::ZERO, Ok(U256::from(leftover).abi_encode()));
        }
        pool.settle().unwrap();

        let owed = pool.owed(id).unwrap();
        // Income ≈ 0.4% of ~25 USDG ≈ 0.0999 USDG.
        assert!(owed > U256::from(99_000u64) && owed < U256::from(101_000u64), "owed {owed}");

        // Lapsed: the registry says she can't receive income, so the claim is refused and nothing is paid.
        let can = Abi::canReceiveIncomeCall { memberId: id }.abi_encode();
        vm.mock_static_call(REGISTRY, can.clone(), Ok(false.abi_encode()));
        assert!(pool.claim(id).is_err());
        assert_eq!(pool.owed(id).unwrap(), owed, "held income is not lost");

        // Alive again: the claim pays exactly what is owed to her payout address.
        vm.mock_static_call(REGISTRY, can, Ok(true.abi_encode()));
        vm.mock_call(TREASURY, Abi::payCall { to: MOTHER_PAYOUT, usdgAmount: owed }.abi_encode(), U256::ZERO, Ok(Vec::new()));
        assert_eq!(pool.claim(id).unwrap(), owed);
        assert_eq!(pool.owed(id).unwrap(), U256::ZERO);
    }

    #[test]
    fn level_and_escalating_members_are_separate_cohorts_paid_at_their_own_rate() {
        let (vm, mut pool) = setup();
        let p = prices();
        // Two mothers born 1966, both drawing from 55: one level, one escalating.
        mock_join(&vm, T0, 1966, 0, MOTHER_PAYOUT);
        let level = pool.join(key(1966), 55, 0, false, MOTHER_PAYOUT, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        mock_join(&vm, T0, 1966, 1, MARIA);
        let esc = pool.join(key(1966), 55, 0, true, MARIA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        let (l_t, _, _, _, _, _, _) = pool.member(level);
        let (e_t, _, _, _, _, _, _) = pool.member(esc);
        assert_ne!(l_t, e_t, "the plan is part of the cohort");
        assert_eq!(pool.counts().1, U256::from(4u8));
        assert_eq!(pool.cohort(l_t).1.to::<u64>() & 2, 0);
        assert_eq!(pool.cohort(e_t).1.to::<u64>() & 2, 2);
        for m in [level, esc] {
            mock_contribute(&vm, MARIA, 25_000_000);
            pool.contribute(m, U256::from(25_000_000u64)).unwrap();
        }
        vm.mock_static_call(TREASURY, Abi::pricesCall {}.abi_encode(), Ok(p.abi_encode()));
        let minted = 49_603_174_603_174_603_174u128;
        vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(50_000_000u64) }.abi_encode(), U256::ZERO, Ok(U256::from(minted).abi_encode()));
        pool.settle().unwrap();

        // A month later both are paying. The Actuary prices the escalating plan lower.
        let t1 = T0 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t1);
        let (year, age) = year_age(t1, 1966);
        let q_wad = U256::from(600_000_000_000_000u128);
        let (p_level, p_esc) = (U256::from(4_000_000_000_000_000u128), U256::from(3_300_000_000_000_000u128));
        vm.mock_static_call(ACTUARY, Abi::qMonthCall { key: key(1966), age, year }.abi_encode(), Ok(q_wad.abi_encode()));
        vm.mock_static_call(ACTUARY, Abi::payoutFractionCall { key: key(1966), age, year, escalating: false }.abi_encode(), Ok(p_level.abi_encode()));
        vm.mock_static_call(ACTUARY, Abi::payoutFractionCall { key: key(1966), age, year, escalating: true }.abi_encode(), Ok(p_esc.abi_encode()));

        // The expected sale, from the core ledger on the pool's stored cohorts.
        let n = pool.counts().1.to::<u64>();
        let mut cohorts: Vec<Cohort> = (0..n).map(|i| load_cohort(&pool, i, i % 2 == 1)).collect();
        let wadfx = |w: U256| Fx::from_wad(w.to::<u128>() as i128).unwrap();
        let months: Vec<CohortMonth> = (0..n)
            .map(|i| {
                let c = &cohorts[i as usize];
                let escalating = pool.cohort(U256::from(i)).1.to::<u64>() & 2 != 0;
                if c.units == 0 {
                    return CohortMonth { q: Fx::ZERO, payout_fraction: Fx::ZERO };
                }
                CohortMonth {
                    q: if c.class == Class::Tontine { wadfx(q_wad) } else { Fx::ZERO },
                    payout_fraction: wadfx(if escalating { p_esc } else { p_level }),
                }
            })
            .collect();
        let fx_prices = [fx_of_wad(p[0]).unwrap(), fx_of_wad(p[1]).unwrap(), fx_of_wad(p[2]).unwrap()];
        let mut ep = Epoch { cohorts: &mut cohorts, months: &months, prices: fx_prices, fee_fraction: Fx::from_ratio(30, 120_000).unwrap() };
        let want = ep.settle(&[], [0; SLEEVES]).unwrap();
        let sold = want.to_sell[0] + want.fees[0];
        let usdg_back = sold * 1_008 / 1_000 / 1_000_000_000_000;
        vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(sold), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(usdg_back).abi_encode()));
        for leftover in 0u64..16 {
            vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(leftover) }.abi_encode(), U256::ZERO, Ok(U256::from(leftover).abi_encode()));
        }
        pool.settle().unwrap();

        // Each is paid her own plan's fraction of the same ~25 USDG: 0.4% against 0.33%.
        let (ol, oe) = (pool.owed(level).unwrap().to::<u128>(), pool.owed(esc).unwrap().to::<u128>());
        assert!(ol > 99_000 && ol < 101_000, "level {ol}");
        assert!(oe > 81_700 && oe < 83_300, "escalating {oe}");
        let ratio = ol as f64 / oe as f64;
        assert!((ratio - 4.0 / 3.3).abs() < 0.001, "ratio {ratio}");
        assert_eq!(pool.cohort(e_t).2.to::<u128>(), cohorts[e_t.to::<usize>()].shares[0], "pool matches the core ledger exactly");
    }

    #[test]
    fn a_death_credits_survivors_pays_the_heir_and_nothing_more_to_the_dead() {
        let (vm, mut pool) = setup();
        let p = prices();
        // Ana (born 1985) keeps 20% as bequest; Bea (born 1980) is fully at risk. Both still saving.
        mock_join(&vm, T0, 1985, 0, MARIA);
        let ana = pool.join(key(1985), 65, 2_000, false, MARIA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        mock_join(&vm, T0, 1980, 1, MOTHER_PAYOUT);
        let bea = pool.join(key(1980), 65, 0, false, MOTHER_PAYOUT, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        for m in [ana, bea] {
            mock_contribute(&vm, MARIA, 25_000_000);
            pool.contribute(m, U256::from(25_000_000u64)).unwrap();
        }
        vm.mock_static_call(TREASURY, Abi::pricesCall {}.abi_encode(), Ok(p.abi_encode()));
        let minted = 49_603_174_603_174_603_174u128; // 50 / 1.008
        vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(50_000_000u64) }.abi_encode(), U256::ZERO, Ok(U256::from(minted).abi_encode()));
        pool.settle().unwrap();
        let (a_t, a_b, a_tu, a_bu, _, _, _) = pool.member(ana);
        let (b_t, _, b_tu, _, _, _, _) = pool.member(bea);

        // A month later Ana's death is final (reported and unanswered); she died a week into the month.
        let t1 = T0 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t1);
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: ana }.abi_encode(), Ok(U256::from(5u8).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::dateOfDeathCall { memberId: ana }.abi_encode(), Ok(U256::from(T0 + 7 * 86_400).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::reporterOfCall { memberId: ana }.abi_encode(), Ok(Address::new([0xbb; 20]).abi_encode()));
        pool.mark_dead(ana).unwrap();
        assert!(pool.mark_dead(ana).is_err(), "queued once");

        // Mocks for the month: each cohort's q (accumulating, so no payout fraction is asked).
        let q = U256::from(300_000_000_000_000u128); // 0.0003
        for birth in [1985u64, 1980] {
            let (year, age) = year_age(t1, birth);
            vm.mock_static_call(ACTUARY, Abi::qMonthCall { key: key(birth), age, year }.abi_encode(), Ok(q.abi_encode()));
        }
        // The expected settlement, from the tested core ledger steps on the pool's stored state.
        let qf = Fx::from_wad(q.to::<u128>() as i128).unwrap();
        let before_bea = load_cohort(&pool, b_t.to::<u64>(), false).shares[0];
        let want = ledger_month(&pool, carry_of(&pool), &|_, c| (if c.class == Class::Tontine && c.units > 0 { qf } else { Fx::ZERO }, Fx::ZERO), &[(ana, Out::Death)], [0; SLEEVES]);
        let cohorts = &want.cohorts;
        let _ = (a_t, a_b, a_tu, a_bu, p);
        let sold = want.sale()[0];
        let sell = [U256::from(sold), U256::ZERO, U256::ZERO];
        let usdg_back = sold * 1_008 / 1_000 / 1_000_000_000_000;
        vm.mock_call(TREASURY, Abi::sellCall { shares: sell }.abi_encode(), U256::ZERO, Ok(U256::from(usdg_back).abi_encode()));
        for leftover in 0u64..16 {
            vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(leftover) }.abi_encode(), U256::ZERO, Ok(U256::from(leftover).abi_encode()));
        }
        pool.settle().unwrap();

        // Bea's cohort received 95% of Ana's released tontine shares (less dust carried forward);
        // the other 5% waits in the revival reserve in case the death was wrong.
        let after_bea = pool.cohort(b_t).2.to::<u128>();
        assert_eq!(after_bea, cohorts[b_t.to::<usize>()].shares[0], "pool matches the core ledger exactly");
        assert!(after_bea > before_bea, "survivor credited");
        let credited = want.credited[0];
        assert!(credited > 18_500_000_000_000_000_000 && credited <= 18_849_206_349_206_349_207, "≈ 95% of Ana's 80% tontine share: {credited}");
        assert_eq!(pool.revival_reserve().0.to::<u128>(), want.reserved[0], "5% of the release is held for a revival");
        // The carry is the ledger's, plus the cash shares that reinvested USDG dust bought.
        assert!(carry_of(&pool)[0] >= want.carry[0] && carry_of(&pool)[0] - want.carry[0] < 1_000_000);
        // Ana is out: no units, no income, flagged released; her 20% bequest went to her heir.
        let (_, _, tu, bu, _, _, flags) = pool.member(ana);
        assert_eq!((tu, bu), (U256::ZERO, U256::ZERO));
        assert!(flags.to::<u64>() & FLAG_RELEASED != 0);
        // A reported death's estate is held for a year (skeptic review 3: a false report must not
        // pay the family either), then paid to the beneficiary if she is still dead.
        let (heir, since, _) = pool.estate_of(ana);
        let heir = heir.to::<u128>();
        assert!(heir > 4_980_000 && heir <= 5_000_000, "heir is owed ~5 USDG, got {heir}");
        assert_eq!(pool.claimable_of(HEIR), U256::ZERO, "held, not paid yet");
        assert!(matches!(pool.pay_estate(ana), Err(PoolError::TooEarly(_))));
        vm.set_block_timestamp(since.to::<u64>() + 364 * 86_400);
        assert!(matches!(pool.pay_estate(ana), Err(PoolError::TooEarly(_))), "a whole year");
        vm.set_block_timestamp(since.to::<u64>() + 365 * 86_400);
        pool.pay_estate(ana).unwrap();
        assert_eq!(pool.claimable_of(HEIR).to::<u128>(), heir, "paid after a year");
        assert!(pool.pay_estate(ana).is_err(), "once");
        assert!(pool.claim(ana).is_err(), "the dead are never paid");
        assert!(pool.contribute(ana, U256::from(1u8)).is_err());
        let _ = b_tu;
    }

    #[test]
    fn rebalance_moves_a_young_cohort_from_cash_into_sgov_and_spy() {
        let (vm, mut pool) = setup();
        let p = prices();
        mock_join(&vm, T0, 1990, 0, MARIA);
        let id = pool.join(key(1990), 60, 0, false, MARIA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        mock_contribute(&vm, MARIA, 25_000_000);
        pool.contribute(id, U256::from(25_000_000u64)).unwrap();
        vm.mock_static_call(TREASURY, Abi::pricesCall {}.abi_encode(), Ok(p.abi_encode()));
        let minted = 24_801_587_301_587_301_587u128;
        vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(25_000_000u64) }.abi_encode(), U256::ZERO, Ok(U256::from(minted).abi_encode()));
        pool.settle().unwrap();

        // Expected plan: the tested core function on the pool's stored state and ages.
        let n = pool.counts().1.to::<u64>();
        let mut cohorts: Vec<Cohort> = (0..n).map(|i| load_cohort(&pool, i, i % 2 == 1)).collect();
        let (_, age_wad) = year_age(T0, 1990);
        let age = Fx::from_wad(age_wad.to::<u128>() as i128).unwrap();
        let ages: Vec<Fx> = (0..n).map(|_| age).collect();
        let fx_prices = [fx_of_wad(p[0]).unwrap(), fx_of_wad(p[1]).unwrap(), fx_of_wad(p[2]).unwrap()];
        let plan = actuary_core::rebalance::plan(&cohorts, &ages, &fx_prices, Fx::from_ratio(5, 100).unwrap()).unwrap();
        assert!(plan.sell[0] > 0 && plan.sell[1] == 0 && plan.sell[2] == 0, "a young all-cash cohort sells cash only");
        let sold_usdg = plan.sell[0] * 1_008 / 1_000 / 1_000_000_000_000; // vault price, 6 dp
        vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(plan.sell[0]), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(sold_usdg).abi_encode()));
        // USDG split pro rata to the net buys (SGOV first, SPY takes the remainder), filled at oracle.
        let total = plan.buy_value[1].0 as u128 + plan.buy_value[2].0 as u128;
        let to_sgov = mul_div(sold_usdg, plan.buy_value[1].0 as u128, total).unwrap();
        let to_spy = sold_usdg - to_sgov;
        let sgov_out = to_sgov * 1_000_000_000_000 * WAD / p[1].to::<u128>();
        let spy_out = to_spy * 1_000_000_000_000 * WAD / p[2].to::<u128>();
        vm.mock_call(TREASURY, Abi::buyCall { sleeve: 1, usdgIn: U256::from(to_sgov) }.abi_encode(), U256::ZERO, Ok(U256::from(sgov_out).abi_encode()));
        vm.mock_call(TREASURY, Abi::buyCall { sleeve: 2, usdgIn: U256::from(to_spy) }.abi_encode(), U256::ZERO, Ok(U256::from(spy_out).abi_encode()));
        pool.rebalance().unwrap();
        assert!(pool.rebalance().is_err(), "once per epoch");

        let dust = actuary_core::rebalance::apply(&mut cohorts, &plan, &[0, sgov_out, spy_out]).unwrap();
        let (t, _, _, _, _, _, _) = pool.member(id);
        let got = load_cohort(&pool, t.to::<u64>(), false);
        assert_eq!(got.shares, cohorts[t.to::<usize>()].shares, "pool matches the core rebalance exactly");
        // The cohort now holds roughly its glide-path mix: 80% SPY, 14% SGOV, 6% cash.
        let v = value(&got.shares, &fx_prices).unwrap();
        let spy_w = value(&[0, 0, got.shares[2]], &fx_prices).unwrap().div(v).unwrap();
        let sgov_w = value(&[0, got.shares[1], 0], &fx_prices).unwrap().div(v).unwrap();
        let f = |x: Fx| x.0 as f64 / Q64 as f64;
        assert!((f(spy_w) - 0.80).abs() < 0.01 && (f(sgov_w) - 0.14).abs() < 0.01, "spy {} sgov {}", f(spy_w), f(sgov_w));
        let _ = dust;
    }

    const BEA: Address = Address::new([0x99; 20]);
    const AUNT: Address = Address::new([0x88; 20]);

    /// A busy second month, set up identically on any VM: four members (one dies, two draw income
    /// on different plans, one keeps a bequest share, one tops up), with every mock the settlement
    /// needs. Returns the pool, ready to settle at `t1`, and its cohort count.
    fn busy_month(vm: &TestVM) -> (TontiPool, u64) {
        vm.set_block_timestamp(T0);
        vm.set_sender(MARIA);
        let mut pool = TontiPool::from(vm);
        pool.init(TREASURY, REGISTRY, ACTUARY, USDG).unwrap();
        pool.set_params(U256::from((YEAR_SECONDS / 12) as u64), U256::from(100_000_000u64)).unwrap();
        let p = prices();
        // (birth, start age, bequest bps, escalating, payout address)
        let people = [(1985u64, 65u64, 2_000u64, false, MARIA), (1980, 65, 0, false, BEA), (1966, 55, 0, false, MOTHER_PAYOUT), (1966, 55, 1_000, true, AUNT)];
        for (i, &(birth, start, beta, esc, payout)) in people.iter().enumerate() {
            mock_join(vm, T0, birth, i as u64, payout);
            pool.join(key(birth), start, beta, esc, payout, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
            mock_contribute(vm, MARIA, 25_000_000);
            pool.contribute(U256::from(i), U256::from(25_000_000u64)).unwrap();
        }
        vm.mock_static_call(TREASURY, Abi::pricesCall {}.abi_encode(), Ok(p.abi_encode()));
        let minted = 99_206_349_206_349_206_349u128; // 100 / 1.008
        vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(100_000_000u64) }.abi_encode(), U256::ZERO, Ok(U256::from(minted).abi_encode()));
        pool.settle().unwrap();

        // A month on: Ana's death is final (she died a week in) and Bea tops up.
        let t1 = T0 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t1);
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: U256::ZERO }.abi_encode(), Ok(U256::from(5u8).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::dateOfDeathCall { memberId: U256::ZERO }.abi_encode(), Ok(U256::from(T0 + 7 * 86_400).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::reporterOfCall { memberId: U256::ZERO }.abi_encode(), Ok(Address::new([0xbb; 20]).abi_encode()));
        pool.mark_dead(U256::ZERO).unwrap();
        pool.contribute(U256::from(1u8), U256::from(25_000_000u64)).unwrap();

        // This month's rates.
        let q = U256::from(300_000_000_000_000u128);
        let (p_level, p_esc) = (U256::from(4_000_000_000_000_000u128), U256::from(3_300_000_000_000_000u128));
        for birth in [1985u64, 1980, 1966] {
            let (year, age) = year_age(t1, birth);
            vm.mock_static_call(ACTUARY, Abi::qMonthCall { key: key(birth), age, year }.abi_encode(), Ok(q.abi_encode()));
        }
        let (year, age) = year_age(t1, 1966);
        vm.mock_static_call(ACTUARY, Abi::payoutFractionCall { key: key(1966), age, year, escalating: false }.abi_encode(), Ok(p_level.abi_encode()));
        vm.mock_static_call(ACTUARY, Abi::payoutFractionCall { key: key(1966), age, year, escalating: true }.abi_encode(), Ok(p_esc.abi_encode()));

        // The sale the settlement must place, from the core ledger steps on the stored state.
        let n = pool.counts().1.to::<u64>();
        let wadfx = |w: U256| Fx::from_wad(w.to::<u128>() as i128).unwrap();
        let rates = |i: usize, c: &Cohort| {
            let (k, meta, ..) = pool.cohort(U256::from(i));
            // Only pooled (tontine) cohorts pay income; the bequest part stays invested.
            let paying = k == key(1966) && c.units > 0 && c.class == Class::Tontine;
            (
                if c.class == Class::Tontine && c.units > 0 { wadfx(q) } else { Fx::ZERO },
                if !paying { Fx::ZERO } else if meta.to::<u64>() & 2 != 0 { wadfx(p_esc) } else { wadfx(p_level) },
            )
        };
        let want = ledger_month(&pool, carry_of(&pool), &rates, &[(U256::ZERO, Out::Death)], [0; SLEEVES]);
        let sold = want.sale()[0];
        assert!(want.to_sell[0] > 0 && want.claims[0] > 0 && want.credited[0] > 0 && want.reserved[0] > 0, "income, a bequest, credits and the reserve all happen");
        let usdg_back = sold * 1_008 / 1_000 / 1_000_000_000_000;
        vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(sold), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(usdg_back).abi_encode()));
        // Bea's 25 USDG plus the rounding leftovers are invested at the vault price.
        for extra in 0u128..64 {
            let amount = 25_000_000 + extra;
            let shares = amount * 1_000_000_000_000 * 1_000 / 1_008;
            vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(amount) }.abi_encode(), U256::ZERO, Ok(U256::from(shares).abi_encode()));
        }
        (pool, n)
    }

    fn assert_same(a: &TontiPool, b: &TontiPool, cohorts: u64, members: u64) {
        for i in 0..cohorts {
            let id = U256::from(i);
            assert_eq!(a.cohort(id), b.cohort(id), "cohort {i}");
            let e = a.epoch.get();
            assert_eq!(a.c_ipu_hist.get(hist_key(id, e)), b.c_ipu_hist.get(hist_key(id, e)), "history {i}");
        }
        for m in 0..members {
            let id = U256::from(m);
            assert_eq!(a.member(id), b.member(id), "member {m}");
            assert_eq!(a.owed(id).unwrap(), b.owed(id).unwrap(), "owed {m}");
            assert_eq!((a.m_tsnap.get(id), a.m_bsnap.get(id)), (b.m_tsnap.get(id), b.m_bsnap.get(id)), "snapshots {m}");
        }
        for who in [HEIR, MARIA, BEA, AUNT] {
            assert_eq!(a.claimable_of(who), b.claimable_of(who));
        }
        assert_eq!(a.epoch_info(), b.epoch_info());
        assert_eq!((a.carry0.get(), a.carry1.get(), a.carry2.get()), (b.carry0.get(), b.carry1.get(), b.carry2.get()), "carry");
        assert_eq!((a.unallocated_usdg.get(), a.protocol_usdg.get(), a.pending_total.get()), (b.unallocated_usdg.get(), b.protocol_usdg.get(), b.pending_total.get()));
        assert_eq!(a.run_state(), b.run_state());
    }

    #[test]
    fn settlement_one_item_per_transaction_equals_settlement_in_one() {
        let (vm_a, vm_b) = (TestVM::new(), TestVM::new());
        let (mut a, n) = busy_month(&vm_a);
        let (mut b, _) = busy_month(&vm_b);
        a.settle().unwrap();

        let mut pages = 0u64;
        while !b.settle_steps(1).unwrap() {
            pages += 1;
            if pages == 5 {
                // Mid-settlement: deposits and parameter changes wait, and no rebalance can start.
                assert!(matches!(b.contribute(U256::from(2u8), U256::from(1_000_000u64)), Err(PoolError::Busy(_))));
                assert!(matches!(b.set_params(U256::from(86_400u64), U256::from(1u8)), Err(PoolError::Busy(_))));
                assert!(matches!(b.rebalance_steps(1), Err(PoolError::Busy(_))));
                assert!(!b.run_state().0.is_zero());
            }
        }
        // 1 death + 8 cohorts × 3 passes + 1 bequest + 2 single steps + 1 deposit, one per call.
        assert!(pages >= 1 + 3 * n + 1 + 2, "{pages} pages");
        assert_same(&a, &b, n, 4);
        assert!(a.estate_of(U256::ZERO).0 > U256::ZERO, "Ana's heir is owed her bequest (held a year)");
        assert_eq!(a.estate_of(U256::ZERO), b.estate_of(U256::ZERO));
        assert!(a.owed(U256::from(2u8)).unwrap() > U256::ZERO && a.owed(U256::from(3u8)).unwrap() > U256::ZERO, "both plans paid");

        // Rebalance: the core plan on the settled state gives the exact trades to expect.
        let t1 = T0 + (YEAR_SECONDS / 12) as u64;
        let p = prices();
        let fx_prices = [fx_of_wad(p[0]).unwrap(), fx_of_wad(p[1]).unwrap(), fx_of_wad(p[2]).unwrap()];
        let cohorts: Vec<Cohort> = (0..n).map(|i| load_cohort(&a, i, i % 2 == 1)).collect();
        let ages: Vec<Fx> = (0..n).map(|i| Fx::from_wad(age_wad_at(a.cohort(U256::from(i)).0, t1 as u128).unwrap() as i128).unwrap()).collect();
        let plan = actuary_core::rebalance::plan(&cohorts, &ages, &fx_prices, Fx::from_ratio(5, 100).unwrap()).unwrap();
        assert!(plan.sell[0] > 0 && plan.sell[1] == 0 && plan.sell[2] == 0);
        let sold_usdg = plan.sell[0] * 1_008 / 1_000 / 1_000_000_000_000;
        let total = plan.buy_value[1].0 as u128 + plan.buy_value[2].0 as u128;
        let to_sgov = mul_div(sold_usdg, plan.buy_value[1].0 as u128, total).unwrap();
        let to_spy = sold_usdg - to_sgov;
        for vm in [&vm_a, &vm_b] {
            vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(plan.sell[0]), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(sold_usdg).abi_encode()));
            vm.mock_call(TREASURY, Abi::buyCall { sleeve: 1, usdgIn: U256::from(to_sgov) }.abi_encode(), U256::ZERO, Ok(U256::from(to_sgov * 1_000_000_000_000 * WAD / p[1].to::<u128>()).abi_encode()));
            vm.mock_call(TREASURY, Abi::buyCall { sleeve: 2, usdgIn: U256::from(to_spy) }.abi_encode(), U256::ZERO, Ok(U256::from(to_spy * 1_000_000_000_000 * WAD / p[2].to::<u128>()).abi_encode()));
        }
        a.rebalance().unwrap();
        let mut pages = 0u64;
        while !b.rebalance_steps(1).unwrap() {
            pages += 1;
            if pages == 3 {
                assert!(matches!(b.settle_steps(1), Err(PoolError::Busy(_))), "no settlement during a rebalance");
            }
        }
        assert!(pages >= 2 * n, "{pages} pages");
        assert_same(&a, &b, n, 4);
        assert!(a.cohort(U256::from(2u8)).4 > U256::ZERO, "Bea's cohort now holds SPY");
    }

    #[test]
    fn income_history_lookup_is_a_binary_search_over_epoch_times() {
        let (_vm, mut pool) = setup();
        let c = U256::from(3u8);
        for e in 1u64..=9 {
            pool.epoch_ts.insert(U256::from(e), U256::from(e * 100));
            pool.c_ipu_hist.insert(hist_key(c, U256::from(e)), U256::from(e * 10));
        }
        pool.epoch.set(U256::from(9u8));
        for (ts, want) in [(0u128, 0u64), (99, 0), (100, 10), (150, 10), (499, 40), (500, 50), (900, 90), (10_000, 90)] {
            assert_eq!(pool.ipu_at(c, ts), U256::from(want), "at {ts}");
        }
    }

    #[test]
    fn contributions_wait_for_a_verified_identity() {
        let (vm, mut pool) = setup();
        mock_join(&vm, T0, 1990, 0, MARIA);
        let id = pool.join(key(1990), 60, 0, false, MARIA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        vm.mock_static_call(REGISTRY, Abi::identifiedCall { memberId: id }.abi_encode(), Ok(false.abi_encode()));
        assert!(matches!(pool.contribute(id, U256::from(1_000_000u64)), Err(PoolError::NotEligible(_))), "no money before identity");
        vm.mock_static_call(REGISTRY, Abi::identifiedCall { memberId: id }.abi_encode(), Ok(true.abi_encode()));
        mock_contribute(&vm, MARIA, 1_000_000);
        pool.contribute(id, U256::from(1_000_000u64)).unwrap();
    }

    #[test]
    fn contributions_below_one_usdg_are_refused() {
        let (vm, mut pool) = setup();
        mock_join(&vm, T0, 1990, 0, MARIA);
        let id = pool.join(key(1990), 60, 0, false, MARIA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        assert!(pool.contribute(id, U256::from(999_999u64)).is_err());
        mock_contribute(&vm, MARIA, 1_000_000);
        pool.contribute(id, U256::from(1_000_000u64)).unwrap();
    }

    /// Ana (1985, keeps 20% as bequest) and Bea (1980, all at risk), both saving for 65, 25 USDG each,
    /// invested. Returns the pool and both member ids.
    fn two_savers(vm: &TestVM) -> (TontiPool, U256, U256) {
        vm.set_block_timestamp(T0);
        vm.set_sender(MARIA);
        let mut pool = TontiPool::from(vm);
        pool.init(TREASURY, REGISTRY, ACTUARY, USDG).unwrap();
        mock_join(vm, T0, 1985, 0, MARIA);
        let ana = pool.join(key(1985), 65, 2_000, false, MARIA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        mock_join(vm, T0, 1980, 1, BEA);
        let bea = pool.join(key(1980), 65, 0, false, BEA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        for m in [ana, bea] {
            mock_contribute(vm, MARIA, 25_000_000);
            pool.contribute(m, U256::from(25_000_000u64)).unwrap();
        }
        vm.mock_static_call(TREASURY, Abi::pricesCall {}.abi_encode(), Ok(prices().abi_encode()));
        vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(50_000_000u64) }.abi_encode(), U256::ZERO, Ok(U256::from(49_603_174_603_174_603_174u128).abi_encode()));
        pool.settle().unwrap();
        (pool, ana, bea)
    }

    /// Mocks the month at `t` for the two savers and returns the ledger's month with Ana released
    /// (by death or exit), and Bea's tontine cohort after it.
    fn release_ana(vm: &TestVM, pool: &TontiPool, ana: U256, bea: U256, t: u64, out: Out) -> (Month, u128) {
        let q = U256::from(300_000_000_000_000u128);
        for birth in [1985u64, 1980] {
            let (year, age) = year_age(t, birth);
            vm.mock_static_call(ACTUARY, Abi::qMonthCall { key: key(birth), age, year }.abi_encode(), Ok(q.abi_encode()));
        }
        let qf = Fx::from_wad(q.to::<u128>() as i128).unwrap();
        let want = ledger_month(pool, carry_of(pool), &|_, c| (if c.class == Class::Tontine && c.units > 0 { qf } else { Fx::ZERO }, Fx::ZERO), &[(ana, out)], [0; SLEEVES]);
        for leftover in 0u64..16 {
            vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(leftover) }.abi_encode(), U256::ZERO, Ok(U256::from(leftover).abi_encode()));
        }
        let b_t = pool.member(bea).0.to::<usize>();
        let bea_after = want.cohorts[b_t].shares[0];
        (want, bea_after)
    }

    #[test]
    fn an_exit_after_notice_pays_the_member_and_leaves_one_percent_with_those_who_stay() {
        let vm = TestVM::new();
        let (mut pool, ana, bea) = two_savers(&vm);
        // Only Ana's own payout address can give notice, once; no new money while it runs.
        vm.set_sender(BEA);
        assert!(matches!(pool.request_exit(ana), Err(PoolError::Unauthorized(_))));
        vm.set_sender(MARIA);
        let at = pool.request_exit(ana).unwrap();
        assert_eq!(at, U256::from(T0 as u128 + 365 * DAY));
        assert!(pool.request_exit(ana).is_err());
        assert!(pool.contribute(ana, U256::from(1_000_000u64)).is_err());
        assert!(matches!(pool.exit(ana), Err(PoolError::TooEarly(_))));

        // A year on. Lapsed members can't be exited (an heir can't cash out the dead); alive, she can.
        let t1 = T0 + (365 * DAY) as u64;
        vm.set_block_timestamp(t1);
        let can = Abi::canReceiveIncomeCall { memberId: ana }.abi_encode();
        vm.mock_static_call(REGISTRY, can.clone(), Ok(false.abi_encode()));
        assert!(matches!(pool.exit(ana), Err(PoolError::NotEligible(_))));
        vm.mock_static_call(REGISTRY, can, Ok(true.abi_encode()));
        pool.exit(ana).unwrap();
        assert!(pool.cancel_exit(ana).is_err(), "too late to cancel once queued");

        // Expected: 99% of her at-risk shares and all her bequest shares are sold for her; the 1% left
        // is credited to Bea, and no reserve is taken (an exit is not a death).
        let (want, bea_after) = release_ana(&vm, &pool, ana, bea, t1, Out::Exit);
        assert_eq!(want.reserved, [0; SLEEVES]);
        let sold = want.sale()[0];
        let usdg_back = sold * 1_008 / 1_000 / 1_000_000_000_000;
        vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(sold), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(usdg_back).abi_encode()));
        pool.settle().unwrap();

        let b_t = pool.member(bea).0;
        assert_eq!(pool.cohort(b_t).2.to::<u128>(), bea_after, "Bea keeps exactly the 1%");
        assert_eq!(pool.revival_reserve().0, U256::ZERO);
        let flags = pool.member(ana).6.to::<u64>();
        assert!(flags & FLAG_RELEASED != 0 && flags & FLAG_EXITED != 0);
        // 99% of her ~20 USDG at risk plus her ~5 USDG bequest share, less a month's fee.
        let paid = pool.claimable_of(MARIA).to::<u128>();
        assert!(paid > 24_780_000 && paid < 24_800_000, "paid {paid}");
        assert_eq!(pool.claimable_of(HEIR), U256::ZERO, "an exit pays no beneficiary");
        assert!(pool.claim(ana).is_err());
    }

    #[test]
    fn a_death_during_notice_beats_the_exit_and_paying_members_cannot_leave() {
        let vm = TestVM::new();
        let (mut pool, ana, bea) = two_savers(&vm);
        let at = pool.request_exit(ana).unwrap();
        vm.set_block_timestamp(at.to::<u64>());
        vm.mock_static_call(REGISTRY, Abi::canReceiveIncomeCall { memberId: ana }.abi_encode(), Ok(true.abi_encode()));
        pool.exit(ana).unwrap();
        // Her death is made final before the settlement: the queue now holds her twice.
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: ana }.abi_encode(), Ok(U256::from(5u8).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::dateOfDeathCall { memberId: ana }.abi_encode(), Ok(U256::from(at - U256::from(DAY)).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::reporterOfCall { memberId: ana }.abi_encode(), Ok(Address::new([0xbb; 20]).abi_encode()));
        pool.mark_dead(ana).unwrap();
        let t = at.to::<u64>();
        let (want, bea_if_death) = release_ana(&vm, &pool, ana, bea, t, Out::Death);
        let sold = want.sale()[0];
        let usdg_back = sold * 1_008 / 1_000 / 1_000_000_000_000;
        vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(sold), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(usdg_back).abi_encode()));
        pool.settle().unwrap();
        // Settled as a death, once: survivors get her at-risk money (less the reserve's 5%), her
        // heir the bequest.
        assert_eq!(pool.cohort(pool.member(bea).0).2.to::<u128>(), bea_if_death);
        assert_eq!(pool.revival_reserve().0.to::<u128>(), want.reserved[0]);
        assert_eq!(pool.claimable_of(MARIA), U256::ZERO);
        let heir = pool.estate_of(ana).0.to::<u128>();
        assert!(heir > 4_990_000 && heir <= 5_000_000, "heir {heir}");
        assert_eq!(pool.run_state().3, U256::from(2u8), "both queue entries consumed");

        // A paying member can't give notice: the pool priced her income for life.
        let (vm2, mut pool2) = setup();
        mock_join(&vm2, T0, 1966, 0, MOTHER_PAYOUT);
        let mom = pool2.join(key(1966), 55, 0, false, MOTHER_PAYOUT, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        vm2.set_sender(MOTHER_PAYOUT);
        assert!(matches!(pool2.request_exit(mom), Err(PoolError::NotEligible(_))));
    }

    /// Skeptic 2: a mutation letting `mark_dead` queue a living member survived the whole suite.
    /// Only a death the registry has made final (reported or presumed) can be queued, once.
    #[test]
    fn only_a_death_the_registry_made_final_can_be_queued() {
        let vm = TestVM::new();
        let (mut pool, ana, _bea) = two_savers(&vm);
        for s in 0u8..5 {
            // none, active, due, lapsed, death reported (still open to challenge)
            vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: ana }.abi_encode(), Ok(U256::from(s).abi_encode()));
            assert!(matches!(pool.mark_dead(ana), Err(PoolError::NotEligible(_))), "status {s}");
        }
        assert_eq!(pool.member(ana).6.to::<u64>() & FLAG_QUEUED_DEAD, 0);
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: ana }.abi_encode(), Ok(U256::from(STATUS_PRESUMED).abi_encode()));
        // Skeptic 4: nobody pays into the account of a member the registry says has died.
        assert!(matches!(pool.contribute(ana, U256::from(1_000_000u64)), Err(PoolError::NotEligible(_))));
        pool.mark_dead(ana).unwrap();
        assert!(matches!(pool.mark_dead(ana), Err(PoolError::Released(_))), "queued once");
    }

    #[test]
    fn the_guardian_can_stop_new_business_but_not_money_already_owed() {
        let vm = TestVM::new();
        let (mut pool, ana, bea) = two_savers(&vm);
        const GUARDIAN: Address = Address::new([0x9a; 20]);
        pool.set_guardian(GUARDIAN).unwrap();
        vm.set_sender(BEA);
        assert!(matches!(pool.pause(), Err(PoolError::Unauthorized(_))));
        let month = T0 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(month);
        vm.set_sender(GUARDIAN);
        pool.pause().unwrap();
        assert!(pool.is_paused());
        // New business stops.
        assert!(matches!(pool.contribute(bea, U256::from(1_000_000u64)), Err(PoolError::Paused(_))));
        assert!(matches!(pool.join(key(1990), 60, 0, false, BEA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]), Err(PoolError::Paused(_))));
        vm.set_sender(MARIA);
        vm.set_block_timestamp(month + 6 * DAY as u64);
        assert!(matches!(pool.request_exit(ana), Err(PoolError::Paused(_))));
        assert!(matches!(pool.settle_steps(1), Err(PoolError::Paused(_))));
        assert!(matches!(pool.rebalance_steps(1), Err(PoolError::Paused(_))));
        // Money owed still moves, and the guardian can't unpause: only the owner (the timelock).
        assert_eq!(pool.withdraw().unwrap(), U256::ZERO);
        vm.set_sender(GUARDIAN);
        assert!(matches!(pool.unpause(), Err(PoolError::Unauthorized(_))));
        // Skeptic 2: one key must not freeze income. After a week, settlements and exits go on even
        // though new business is still stopped, and the guardian can't simply pause again.
        vm.set_block_timestamp(month + 7 * DAY as u64);
        assert!(pool.is_paused());
        assert!(matches!(pool.contribute(bea, U256::from(1_000_000u64)), Err(PoolError::Paused(_))));
        assert!(!matches!(pool.settle_steps(0), Err(PoolError::Paused(_))), "a week later the month is settled");
        pool.pause().unwrap(); // pausing again while paused changes nothing
        assert!(!matches!(pool.settle_steps(0), Err(PoolError::Paused(_))), "still counted from the first pause");
        vm.set_sender(MARIA); // the owner in this test
        pool.unpause().unwrap();
        assert!(!pool.is_paused());
    }

    #[test]
    fn the_ghost_detector_flags_a_group_with_no_deaths_and_holds_it_after_120_days_until_strong_proofs() {
        let (vm, mut pool) = setup();
        pool.set_params(U256::from((YEAR_SECONDS / 12) as u64), U256::from(25_000_000u64)).unwrap();
        let p = prices();
        vm.mock_static_call(TREASURY, Abi::pricesCall {}.abi_encode(), Ok(p.abi_encode()));
        let fx_prices = [fx_of_wad(p[0]).unwrap(), fx_of_wad(p[1]).unwrap(), fx_of_wad(p[2]).unwrap()];
        // Ten members of one group (Philippines, born 1985), all at risk.
        for i in 0..10u64 {
            mock_join(&vm, T0, 1985, i, MARIA);
            pool.join(key(1985), 65, 0, false, MARIA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
            mock_contribute(&vm, MARIA, 1_000_000);
            pool.contribute(U256::from(i), U256::from(1_000_000u64)).unwrap();
        }
        vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(10_000_000u64) }.abi_encode(), U256::ZERO, Ok(U256::from(9_920_634_920_634_920_634u128).abi_encode()));
        pool.settle().unwrap();
        let g = pool.group_of_member(U256::ZERO);
        assert_eq!(g, U256::from(608u64 * 1000 + 198), "country × birth decade");

        // Every month the model expects 10 × 0.99 = 9.9 deaths, and none are ever made final. Deaths
        // take about five months to become final, so each month is tested against the deaths
        // expected five epochs earlier: nothing until epoch 7, then the log-likelihood grows by
        // 0.3 × 9.9 a month and passes ln(0.95/0.001) = 6.86 in the third month of testing.
        let q = U256::from(990_000_000_000_000_000u128);
        for month in 1..=8u64 {
            let t = T0 + month * (YEAR_SECONDS / 12) as u64;
            vm.set_block_timestamp(t);
            let (year, age) = year_age(t, 1985);
            vm.mock_static_call(ACTUARY, Abi::qMonthCall { key: key(1985), age, year }.abi_encode(), Ok(q.abi_encode()));
            // The only sale is the month's fee on the tontine cohort (cash only).
            let mut c = load_cohort(&pool, 0, false);
            let mut fees = [0u128; SLEEVES];
            step::fee(&mut c, Fx::from_ratio(30, 120_000).unwrap(), &mut fees).unwrap();
            let usdg = fees[0] * 1_008 / 1_000 / 1_000_000_000_000;
            vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(fees[0]), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(usdg).abi_encode()));
            for leftover in 0u64..8 {
                vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(leftover) }.abi_encode(), U256::ZERO, Ok(U256::from(leftover).abi_encode()));
            }
            pool.settle().unwrap();
            let _ = fx_prices;
            let flagged = pool.group_state(g).2;
            if month < 8 {
                assert_eq!(flagged, U256::ZERO, "month {month}: deaths aren't final yet, or too little evidence");
            } else {
                assert_eq!(flagged, U256::from(t), "three tested months of 9.9 expected deaths and none final");
            }
        }
        // Members get 120 days to renew their identity; after that the group is held until each
        // of them proves who they are again.
        let flag_at = pool.group_state(g).2.to::<u64>();
        vm.mock_static_call(REGISTRY, Abi::canReceiveIncomeCall { memberId: U256::from(3u8) }.abi_encode(), Ok(true.abi_encode()));
        let last = Abi::lastStrongCall { memberId: U256::from(3u8) }.abi_encode();
        vm.mock_static_call(REGISTRY, last.clone(), Ok(U256::from(flag_at - 1).abi_encode()));
        vm.set_block_timestamp(flag_at + 119 * 86_400);
        assert_eq!(pool.claim(U256::from(3u8)).unwrap(), U256::ZERO, "within the grace period income still flows");
        vm.set_block_timestamp(flag_at + 120 * 86_400);
        assert!(matches!(pool.claim(U256::from(3u8)), Err(PoolError::NotEligible(_))), "held");
        vm.mock_static_call(REGISTRY, last, Ok(U256::from(flag_at + 60).abi_encode()));
        assert_eq!(pool.claim(U256::from(3u8)).unwrap(), U256::ZERO, "released by a fresh strong proof");
    }

    #[test]
    fn a_reported_death_pays_no_bounty_funds_the_reserve_and_can_be_undone() {
        const REPORTER: Address = Address::new([0xbb; 20]);
        let (vm, mut pool) = setup();
        let p = prices();
        let fxp = [fx_of_wad(p[0]).unwrap(), fx_of_wad(p[1]).unwrap(), fx_of_wad(p[2]).unwrap()];
        vm.mock_static_call(TREASURY, Abi::pricesCall {}.abi_encode(), Ok(p.abi_encode()));
        // A mother and an aunt, both born 1966 and drawing income from 55, all at risk: one cohort.
        mock_join(&vm, T0, 1966, 0, MOTHER_PAYOUT);
        pool.join(key(1966), 55, 0, false, MOTHER_PAYOUT, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        mock_join(&vm, T0, 1966, 1, AUNT);
        pool.join(key(1966), 55, 0, false, AUNT, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        for m in 0..2u64 {
            mock_contribute(&vm, MARIA, 25_000_000);
            pool.contribute(U256::from(m), U256::from(25_000_000u64)).unwrap();
        }
        vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(50_000_000u64) }.abi_encode(), U256::ZERO, Ok(U256::from(49_603_174_603_174_603_174u128).abi_encode()));
        pool.settle().unwrap();
        for leftover in 0u64..16 {
            vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(leftover) }.abi_encode(), U256::ZERO, Ok(U256::from(leftover).abi_encode()));
        }
        let (q, pay) = (U256::from(600_000_000_000_000u128), U256::from(4_000_000_000_000_000u128));
        let wadfx = |w: U256| Fx::from_wad(w.to::<u128>() as i128).unwrap();
        let mock_rates = |vm: &TestVM, t: u64| {
            let (year, age) = year_age(t, 1966);
            vm.mock_static_call(ACTUARY, Abi::qMonthCall { key: key(1966), age, year }.abi_encode(), Ok(q.abi_encode()));
            vm.mock_static_call(ACTUARY, Abi::payoutFractionCall { key: key(1966), age, year, escalating: false }.abi_encode(), Ok(pay.abi_encode()));
        };
        let rates = |_: usize, c: &Cohort| {
            let live = c.units > 0;
            (if c.class == Class::Tontine && live { wadfx(q) } else { Fx::ZERO }, if c.class == Class::Tontine && live { wadfx(pay) } else { Fx::ZERO })
        };
        let settle = |vm: &TestVM, pool: &mut TontiPool, want: &Month| {
            let sold = want.sale()[0];
            vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(sold), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(sold * 1_008 / 1_000 / 1_000_000_000_000).abi_encode()));
            pool.settle().unwrap();
            sold * 1_008 / 1_000 / 1_000_000_000_000
        };

        // Month 1: both are paid.
        let t1 = T0 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t1);
        mock_rates(&vm, t1);
        let want = ledger_month(&pool, carry_of(&pool), &rates, &[], [0; SLEEVES]);
        settle(&vm, &mut pool, &want);

        // Month 2: a neighbour reported the aunt dead a week into month 1, and nobody answered.
        let t2 = t1 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t2);
        mock_rates(&vm, t2);
        let aunt = U256::from(1u8);
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: aunt }.abi_encode(), Ok(U256::from(5u8).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::dateOfDeathCall { memberId: aunt }.abi_encode(), Ok(U256::from(t1 + 7 * 86_400).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::reporterOfCall { memberId: aunt }.abi_encode(), Ok(REPORTER.abi_encode()));
        pool.mark_dead(aunt).unwrap();
        let want = ledger_month(&pool, carry_of(&pool), &rates, &[(aunt, Out::Death)], [0; SLEEVES]);
        settle(&vm, &mut pool, &want);
        // No bounty: a false report can't pay. 5% waits in the reserve; the mother gets the rest.
        assert_eq!(pool.claimable_of(REPORTER), U256::ZERO);
        assert!(want.reserved[0] > 0);
        assert_eq!(pool.revival_reserve().0.to::<u128>(), want.reserved[0]);
        assert_eq!(load_cohort(&pool, 0, false).shares, want.cohorts[0].shares, "mother's cohort matches the ledger");
        let flags = pool.member(aunt).6.to::<u64>();
        assert!(flags & FLAG_RELEASED != 0 && flags & FLAG_PRESUMED == 0, "released as a reported death");
        // Her unclaimed month-1 income, earned before she died, is her estate: it is held for her
        // beneficiary (a year, for a reported death), not paid to her own wallet.
        let (estate, _, post) = pool.estate_of(aunt);
        let (estate, post) = (estate.to::<u128>(), post.to::<u128>());
        assert!(estate > 0, "the estate is held for the beneficiary");
        assert_eq!(pool.claimable_of(HEIR), U256::ZERO, "not paid while a revival is possible");
        assert_eq!(pool.claimable_of(AUNT), U256::ZERO, "and nothing to her payout address");

        // The report was false: she proves she's alive, and the next settlement repays her from the
        // reserve (after its monthly 1/60 to the survivors).
        let t3 = t2 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t3);
        mock_rates(&vm, t3);
        // Skeptic 2: a revival on record while the registry says she is dead (one left over from an
        // earlier, false death) repays nothing.
        vm.mock_static_call(REGISTRY, Abi::revivedAtCall { memberId: aunt }.abi_encode(), Ok(U256::from(t1).abi_encode()));
        assert!(matches!(pool.restore(aunt), Err(PoolError::NotEligible(_))), "not while the registry says she is dead");
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: aunt }.abi_encode(), Ok(U256::from(1u8).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::revivedAtCall { memberId: aunt }.abi_encode(), Ok(U256::from(t3 - 1).abi_encode()));
        pool.restore(aunt).unwrap();
        assert!(pool.restore(aunt).is_err(), "queued once");
        let decay = want.reserved[0] / 60;
        let restore: Shares = [want.reserved[0] - decay, 0, 0];
        let mut start = carry_of(&pool);
        start[0] += decay;
        let want = ledger_month(&pool, start, &rates, &[], restore);
        let before = pool.claimable_of(AUNT).to::<u128>();
        let usdg = settle(&vm, &mut pool, &want);
        let expected = usdg_raw(Fx(mul_div(usdg_fx(usdg).unwrap().0 as u128, value(&restore, &fxp).unwrap().0 as u128, value(&want.sale(), &fxp).unwrap().0 as u128).unwrap() as i128));
        // Skeptic 2: repaid in cash, an undone death was a way out after income had started. The
        // repayment goes back into her own account instead.
        assert_eq!(pool.claimable_of(AUNT).to::<u128>(), before, "nothing leaves the pool");
        let (_, _, tu, _, pending, _, flags) = pool.member(aunt);
        // With them, what her false death held: her estate and the income dated after it (skeptic 4).
        assert_eq!((tu, pending.to::<u128>()), (U256::ZERO, expected + estate + post), "the reserve's shares at the sale price, and her held estate, waiting to be invested for her");
        assert_eq!(pool.pending_total.get().to::<u128>(), expected + estate + post);
        let flags = flags.to::<u64>();
        assert!(flags & (FLAG_RELEASED | FLAG_RESTORED | FLAG_QUEUED_DEAD) == 0 && flags & FLAG_QUEUED_DEPOSIT != 0, "a member again, deposit queued");
        // The reserve covered only part of her release: the rest stays owed for a later restore.
        assert!(flags & FLAG_OWED != 0 && pool.m_owed.get(aunt * U256::from(4u8)) > U256::ZERO, "the rest is still owed");
        // The false report paid her family nothing, and nothing is left held.
        assert_eq!(pool.claimable_of(HEIR), U256::ZERO);
        assert_eq!(pool.estate_of(aunt), (U256::ZERO, pool.estate_of(aunt).1, U256::ZERO));
        assert!(matches!(pool.pay_estate(aunt), Err(PoolError::BadInput(_))));
        let expected = expected + estate + post;

        // Month 4: invested back. She holds units in her cohort again, and is paid from them.
        let t4 = t3 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t4);
        mock_rates(&vm, t4);
        for leftover in 0u128..16 {
            let usdg = expected + leftover;
            vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(usdg) }.abi_encode(), U256::ZERO, Ok(U256::from(usdg * 1_000_000_000_000).abi_encode()));
        }
        let want = ledger_month(&pool, carry_of(&pool), &rates, &[], [0; SLEEVES]);
        settle(&vm, &mut pool, &want);
        let (t, _, tu, _, pending, _, flags) = pool.member(aunt);
        assert!(tu > U256::ZERO && pending == U256::ZERO, "units minted for her");
        assert!(flags.to::<u64>() & FLAG_COUNTED != 0);
        assert_eq!(pool.c_members.get(t), U256::from(2u8), "counted among the living of her cohort again");
    }

    // ---------------------------------------------------------------- regressions (skeptic, 2026-09-27)

    /// A whole counterparty world for multi-month scenarios: a Treasury that tracks its USDG and
    /// refuses to pay more than it holds, a registry that remembers each enrolment, and fixed rates.
    /// Installed as the TestVM's fallback, so anything it doesn't model still fails the test.
    #[derive(Default)]
    struct World {
        usdg: i128,
        paid: std::collections::HashMap<Address, u128>,
        payout: std::collections::HashMap<u64, Address>,
        heir: std::collections::HashMap<u64, Address>,
        q: u128,
        pay: u128,
        prices: [u128; 3],
    }

    fn world() -> std::rc::Rc<core::cell::RefCell<World>> {
        let w = std::rc::Rc::new(core::cell::RefCell::new(World {
            q: 600_000_000_000_000,
            pay: 4_000_000_000_000_000,
            prices: [1_008_000_000_000_000_000, 101_170_000_000_000_000_000, 772_330_000_000_000_000_000],
            ..Default::default()
        }));
        let ww = w.clone();
        set_fallback(Some(Box::new(move |to: Address, data: &[u8], _value: U256| {
            let mut w = ww.borrow_mut();
            let sel: [u8; 4] = data[..4].try_into().unwrap();
            let e = 1_000_000_000_000u128;
            let id = |d: &[u8]| U256::from_be_slice(&d[4..36]).to::<u64>();
            if to == TREASURY {
                if sel == Abi::pricesCall::SELECTOR {
                    return Some(Ok(w.prices.map(U256::from).abi_encode()));
                }
                if sel == Abi::depositCashCall::SELECTOR {
                    let x = Abi::depositCashCall::abi_decode(data).unwrap().usdgAmount.to::<u128>();
                    w.usdg -= x as i128; // moved into the vault
                    return Some(Ok(U256::from(mul_div(x * e, WAD, w.prices[0]).unwrap()).abi_encode()));
                }
                if sel == Abi::sellCall::SELECTOR {
                    let c = Abi::sellCall::abi_decode(data).unwrap();
                    let out: u128 = (0..3).map(|a| mul_div(c.shares[a].to::<u128>(), w.prices[a], WAD).unwrap() / e).sum();
                    w.usdg += out as i128;
                    return Some(Ok(U256::from(out).abi_encode()));
                }
                if sel == Abi::buyCall::SELECTOR {
                    let c = Abi::buyCall::abi_decode(data).unwrap();
                    let x = c.usdgIn.to::<u128>();
                    w.usdg -= x as i128;
                    return Some(Ok(U256::from(mul_div(x * e, WAD, w.prices[c.sleeve as usize]).unwrap()).abi_encode()));
                }
                if sel == Abi::payCall::SELECTOR {
                    let c = Abi::payCall::abi_decode(data).unwrap();
                    let x = c.usdgAmount.to::<u128>();
                    if x as i128 > w.usdg {
                        return Some(Err(b"transfer amount exceeds balance".to_vec()));
                    }
                    w.usdg -= x as i128;
                    *w.paid.entry(c.to).or_default() += x;
                    return Some(Ok(Vec::new()));
                }
            }
            if to == REGISTRY {
                if sel == Abi::enrollCall::SELECTOR {
                    let c = Abi::enrollCall::abi_decode(data).unwrap();
                    let m = c.memberId.to::<u64>();
                    w.payout.insert(m, c.payout);
                    w.heir.insert(m, c.beneficiary);
                    return Some(Ok(Vec::new()));
                }
                if sel == Abi::payoutOfCall::SELECTOR {
                    return Some(Ok(w.payout[&id(data)].abi_encode()));
                }
                if sel == Abi::beneficiaryOfCall::SELECTOR {
                    return Some(Ok(w.heir[&id(data)].abi_encode()));
                }
                if sel == Abi::canReceiveIncomeCall::SELECTOR || sel == Abi::identifiedCall::SELECTOR {
                    return Some(Ok(true.abi_encode()));
                }
                if sel == Abi::statusCall::SELECTOR {
                    return Some(Ok(U256::from(1u8).abi_encode())); // Active, unless a test mocks otherwise
                }
                if sel == Abi::lastStrongCall::SELECTOR {
                    return Some(Ok(U256::from(T0).abi_encode()));
                }
            }
            if to == ACTUARY {
                if sel == Abi::qMonthCall::SELECTOR {
                    return Some(Ok(U256::from(w.q).abi_encode()));
                }
                if sel == Abi::payoutFractionCall::SELECTOR {
                    return Some(Ok(U256::from(w.pay).abi_encode()));
                }
            }
            if to == USDG && sel == Abi::transferFromCall::SELECTOR {
                let c = Abi::transferFromCall::abi_decode(data).unwrap();
                assert_eq!(c.to, TREASURY, "money only ever goes to the Treasury");
                w.usdg += c.amount.to::<u128>() as i128;
                return Some(Ok(true.abi_encode()));
            }
            None
        })));
        w
    }

    /// Two identical paying members leave 12 months of income unclaimed; one tops up. Before the
    /// fix the new units inherited the old snapshot, were credited all 13 months, and the other
    /// member's claim then bounced off an empty Treasury (skeptic, test `topup_inherits_…`).
    #[test]
    fn a_top_up_keeps_income_already_owed_and_earns_only_from_then() {
        const PA: Address = Address::new([0xa1; 20]);
        const PB: Address = Address::new([0xb1; 20]);
        let vm = TestVM::new();
        vm.set_block_timestamp(T0);
        vm.set_sender(MARIA);
        let w = world();
        let mut pool = TontiPool::from(&vm);
        pool.init(TREASURY, REGISTRY, ACTUARY, USDG).unwrap();
        pool.set_params(U256::from((YEAR_SECONDS / 12) as u64), U256::from(10_000u128 * USDG_UNIT)).unwrap();
        let a = pool.join(key(1966), 55, 0, false, PA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        let b = pool.join(key(1966), 55, 0, false, PB, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        for (m, payer) in [(a, PA), (b, PB)] {
            vm.set_sender(payer);
            pool.contribute(m, U256::from(100 * USDG_UNIT)).unwrap();
        }
        let mut now = T0 + 1;
        vm.set_block_timestamp(now);
        pool.settle().unwrap();
        let month = (YEAR_SECONDS / 12) as u64;
        for _ in 0..12 {
            now += month;
            vm.set_block_timestamp(now);
            pool.settle().unwrap();
        }
        let (oa0, ob0) = (pool.owed(a).unwrap(), pool.owed(b).unwrap());
        assert_eq!(oa0, ob0);
        assert!(ob0 > U256::from(4 * USDG_UNIT), "a year of income is owed: {ob0}");

        // A tops up 100 USDG. The settlement books the month, then mints her new units after it.
        vm.set_sender(PA);
        pool.contribute(a, U256::from(100 * USDG_UNIT)).unwrap();
        now += month;
        vm.set_block_timestamp(now);
        pool.settle().unwrap();
        let (oa1, ob1) = (pool.owed(a).unwrap().to::<u128>(), pool.owed(b).unwrap().to::<u128>());
        assert!(oa1 <= ob1 && ob1 - oa1 <= 1, "the top-up changed nothing already owed: A {oa1}, B {ob1}");

        // From the next month A's income follows her units: her new 100 USDG bought units at the
        // cohort's current value per unit, which 13 months of income payments have lowered.
        now += month;
        vm.set_block_timestamp(now);
        pool.settle().unwrap();
        let (oa2, ob2) = (pool.owed(a).unwrap().to::<u128>(), pool.owed(b).unwrap().to::<u128>());
        let (da, db) = (oa2 - oa1, ob2 - ob1);
        let (ua, ub) = (pool.member(a).2.to::<u128>() as f64, pool.member(b).2.to::<u128>() as f64);
        assert!(ua > 2.0 * ub, "cheaper units after 13 months of payouts");
        assert!((da as f64 / db as f64 - ua / ub).abs() < 1e-4, "income per unit is equal: {da}/{db} vs units {ua}/{ub}");

        // Both are paid in full, and the Treasury never goes short.
        assert_eq!(pool.claim(a).unwrap().to::<u128>(), oa2);
        assert_eq!(pool.claim(b).unwrap().to::<u128>(), ob2);
        assert!(w.borrow().usdg >= 0);
        assert_eq!((w.borrow().paid[&PA], w.borrow().paid[&PB]), (oa2, ob2), "each to their own payout address");
        set_fallback(None);
    }

    /// `topped_up_snap` over random holdings, snapshots and indexes: the held units' accrued income
    /// is kept to within one base unit of the index's precision, and never grows.
    #[test]
    fn topped_up_snapshots_never_create_income() {
        let mut x = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        // Realistic ranges: holdings up to ~2^94 WAD units (20 billion USDG), income indexes (Q64.64
        // USDG per unit) up to 2^80, a gap of up to 2^72 (256 USDG a unit).
        for _ in 0..20_000 {
            let held = 1 + ((next() as u128) << (next() % 31));
            let added = 1 + ((next() as u128) << (next() % 31));
            let snap = (next() as u128) << (next() % 17);
            let ipu = snap + ((next() as u128) >> (next() % 64) << 8);
            let s = topped_up_snap(held, snap, added, ipu).unwrap();
            assert!(s <= ipu);
            let before = mul_div(held, ipu - snap, WAD).unwrap();
            let after = mul_div(held + added, ipu - s, WAD).unwrap();
            assert!(after <= before, "income created: {after} > {before}");
            // Lost at most the rounding of one gap step: (held + added)/WAD + 1.
            assert!(before - after <= (held + added) / WAD + 1, "income lost: {before} → {after}");
        }
        assert_eq!(topped_up_snap(0, 0, 5, 77).unwrap(), 77, "a first deposit starts at the index");
    }

    /// A notice must be used within 90 days of maturing, and never after income has started
    /// (skeptic, test `a_stale_notice_lets_a_paying_member_exit_whenever_they_like`).
    #[test]
    fn an_exit_notice_lapses_after_150_days_and_never_outlives_the_start_of_income() {
        let (vm, mut pool) = setup();
        let can = |vm: &TestVM, id: u64| vm.mock_static_call(REGISTRY, Abi::canReceiveIncomeCall { memberId: U256::from(id) }.abi_encode(), Ok(true.abi_encode()));
        // Two savers born 1980, income from 50 (they are about 46 now).
        mock_join(&vm, T0, 1980, 0, MARIA);
        let early = pool.join(key(1980), 50, 0, false, MARIA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        mock_join(&vm, T0, 1980, 1, BEA);
        let late = pool.join(key(1980), 50, 0, false, BEA, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        can(&vm, 0);
        can(&vm, 1);
        vm.set_sender(MARIA);
        let at = pool.request_exit(early).unwrap().to::<u64>();
        // 151 days after it matured the notice has lapsed; she must cancel and give notice again.
        vm.set_block_timestamp(at + 151 * 86_400);
        assert!(matches!(pool.exit(early), Err(PoolError::NotEligible(_))));
        pool.cancel_exit(early).unwrap();
        let again = pool.request_exit(early).unwrap().to::<u64>();
        vm.set_block_timestamp(again + 10 * 86_400);
        pool.exit(early).unwrap();

        // Bea gives notice at 48.9, so it matures at 49.9, just before her income starts at 50.
        let t = T0 + (2.66 * YEAR_SECONDS as f64) as u64;
        vm.set_block_timestamp(t);
        vm.set_sender(BEA);
        let at = pool.request_exit(late).unwrap().to::<u64>();
        // Inside the 90-day window but past her 50th: income has started, so no exit.
        vm.set_block_timestamp(at + 60 * 86_400);
        let age = age_wad_at(key(1980), (at + 60 * 86_400) as u128).unwrap();
        assert!(age >= 50 * WAD, "she is past 50: {}", age as f64 / 1e18);
        assert!(matches!(pool.exit(late), Err(PoolError::NotEligible(_))));
    }

    #[test]
    fn only_the_deployer_can_initialise() {
        set_fallback(None);
        let vm = TestVM::new();
        vm.set_sender(Address::new([0xee; 20]));
        let mut pool = TontiPool::from(&vm);
        assert!(matches!(pool.init(TREASURY, REGISTRY, ACTUARY, USDG), Err(PoolError::Unauthorized(_))), "a front-runner can't take the pool");
        vm.set_sender(DEPLOYER);
        pool.init(TREASURY, REGISTRY, ACTUARY, USDG).unwrap();
        assert_eq!(pool.owner(), DEPLOYER);
        assert!(matches!(pool.init(TREASURY, REGISTRY, ACTUARY, USDG), Err(PoolError::Unauthorized(_))), "once");
    }

    /// A rebalance whose trade keeps failing (a paused token, a dry pool) can be abandoned after a
    /// day, so it can't stop everyone's income.
    #[test]
    fn a_stuck_rebalance_can_be_abandoned_after_a_day() {
        let vm = TestVM::new();
        let (mut pool, _ana, _bea) = two_savers(&vm);
        let n = pool.counts().1.to::<u32>();
        // Plan every cohort, then the trade fails: the Treasury's sale is not mocked, so it reverts.
        assert!(!pool.rebalance_steps(n).unwrap());
        assert!(matches!(pool.rebalance_steps(1), Err(PoolError::External(_))));
        assert_eq!(pool.run_state().0, U256::from(RB_PLAN));
        assert!(matches!(pool.settle_steps(1), Err(PoolError::Busy(_))), "the stuck run blocks settlement");
        assert!(matches!(pool.abort_rebalance(), Err(PoolError::TooEarly(_))), "not within a day");
        vm.set_block_timestamp(T0 + 86_400);
        vm.set_sender(Address::new([0xee; 20]));
        pool.abort_rebalance().unwrap();
        assert_eq!(pool.run_state().0, U256::ZERO);
        assert!(matches!(pool.rebalance_steps(1), Err(PoolError::TooEarly(_))), "this epoch counts as rebalanced");
        assert!(matches!(pool.abort_rebalance(), Err(PoolError::BadInput(_))), "nothing to abort");
        // The next settlement can start.
        vm.set_block_timestamp(T0 + (YEAR_SECONDS / 12) as u64);
        pool.settle_steps(0).unwrap();
        assert_eq!(pool.run_state().0, U256::from(PH_RELEASE));
    }

    #[test]
    fn a_presumed_death_funds_the_revival_reserve_and_a_revived_member_is_repaid() {
        let vm = TestVM::new();
        let (mut pool, ana, bea) = two_savers(&vm);
        let p = prices();
        let fxp = [fx_of_wad(p[0]).unwrap(), fx_of_wad(p[1]).unwrap(), fx_of_wad(p[2]).unwrap()];
        let fee = Fx::from_ratio(30, 120_000).unwrap();
        let q = U256::from(300_000_000_000_000u128);
        let wadfx = |w: U256| Fx::from_wad(w.to::<u128>() as i128).unwrap();
        let rates = |vm: &TestVM, t: u64| {
            for birth in [1985u64, 1980] {
                let (year, age) = year_age(t, birth);
                vm.mock_static_call(ACTUARY, Abi::qMonthCall { key: key(birth), age, year }.abi_encode(), Ok(q.abi_encode()));
            }
        };
        for leftover in 0u64..16 {
            vm.mock_call(TREASURY, Abi::depositCashCall { usdgAmount: U256::from(leftover) }.abi_encode(), U256::ZERO, Ok(U256::from(leftover).abi_encode()));
        }
        // The ledger's steps for one month: fees, an optional release, the reserve, credits, sale.
        let expect = |pool: &TontiPool, start: Shares, release: Option<(usize, u128, usize, u128)>, restore: Shares| {
            let n = pool.counts().1.to::<u64>();
            let mut c: Vec<Cohort> = (0..n).map(|i| load_cohort(pool, i, i % 2 == 1)).collect();
            let (mut fees, mut pl, mut claims) = ([0u128; SLEEVES], start, restore);
            for x in c.iter_mut() {
                step::fee(x, fee, &mut fees).unwrap();
            }
            let mut reserved = [0u128; SLEEVES];
            if let Some((t, tu, b, bu)) = release {
                let before = pl;
                step::release(&mut c[t], tu, &mut pl, &mut claims).unwrap();
                for a in 0..SLEEVES {
                    reserved[a] = mul_div(pl[a] - before[a], 500, 10_000).unwrap();
                    pl[a] -= reserved[a];
                }
                step::release(&mut c[b], bu, &mut pl, &mut claims).unwrap();
            }
            let (mut total, mut sq) = (0u128, SumSq::default());
            let ws: Vec<u128> = c.iter().map(|x| step::weight(x, if x.class == Class::Tontine { wadfx(q) } else { Fx::ZERO }, &fxp).unwrap()).collect();
            ws.iter().for_each(|&w| {
                total += w;
                sq.add(w).unwrap();
            });
            let (mut credited, mut to_sell) = ([0u128; SLEEVES], [0u128; SLEEVES]);
            for (k, x) in c.iter_mut().enumerate() {
                step::credit_and_sell(x, ws[k], &pl, total, sq.share(total).unwrap(), Fx::ZERO, &mut credited, &mut to_sell).unwrap();
            }
            let sale: Shares = core::array::from_fn(|a| to_sell[a] + claims[a] + fees[a]);
            (c, sale, reserved)
        };
        let carry = |pool: &TontiPool| -> Shares { [pool.carry0.get().to::<u128>(), pool.carry1.get().to::<u128>(), pool.carry2.get().to::<u128>()] };

        // Month 1: nobody reported Ana's death; the registry presumed it.
        let t1 = T0 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t1);
        rates(&vm, t1);
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: ana }.abi_encode(), Ok(U256::from(6u8).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::dateOfDeathCall { memberId: ana }.abi_encode(), Ok(U256::from(T0).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::reporterOfCall { memberId: ana }.abi_encode(), Ok(Address::ZERO.abi_encode()));
        pool.mark_dead(ana).unwrap();
        let (a_t, a_b, a_tu, a_bu, ..) = pool.member(ana);
        let (c1, sale1, reserved) = expect(&pool, carry(&pool), Some((a_t.to::<usize>(), a_tu.to::<u128>(), a_b.to::<usize>(), a_bu.to::<u128>())), [0; SLEEVES]);
        vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(sale1[0]), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(sale1[0] * 1_008 / 1_000 / 1_000_000_000_000).abi_encode()));
        pool.settle().unwrap();
        assert!(reserved[0] > 0);
        // A presumed death (two years of silence) pays its estate, her 20% bequest, to her
        // beneficiary at once, and nothing to her own payout address.
        assert!(pool.claimable_of(HEIR) > U256::ZERO && pool.estate_of(ana).0 == U256::ZERO, "the beneficiary is paid at once");
        assert_eq!(pool.claimable_of(MARIA), U256::ZERO);
        assert_eq!(pool.revival_reserve().0.to::<u128>(), reserved[0], "5% of her at-risk release is held");
        let b_t = pool.member(bea).0.to::<usize>();
        assert_eq!(load_cohort(&pool, b_t as u64, false).shares, c1[b_t].shares, "Bea gets the other 95%");
        vm.mock_static_call(REGISTRY, Abi::revivedAtCall { memberId: ana }.abi_encode(), Ok(U256::ZERO.abi_encode()));
        assert!(matches!(pool.restore(ana), Err(PoolError::NotEligible(_))), "not before she proves she's alive");
        vm.mock_static_call(REGISTRY, Abi::revivedAtCall { memberId: bea }.abi_encode(), Ok(U256::from(t1).abi_encode()));
        assert!(matches!(pool.restore(bea), Err(PoolError::BadInput(_))), "only members released by a death are repaid");

        // Month 2: Ana was alive after all. The reserve first pays its monthly 1/60 to the
        // survivors, then repays her from what's left.
        let t2 = t1 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(t2);
        rates(&vm, t2);
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: ana }.abi_encode(), Ok(U256::from(1u8).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::revivedAtCall { memberId: ana }.abi_encode(), Ok(U256::from(t2 - 1).abi_encode()));
        pool.restore(ana).unwrap();
        let decay = reserved[0] / 60;
        let restore: Shares = [reserved[0] - decay, 0, 0];
        let mut start = carry(&pool);
        start[0] += decay;
        let (c2, sale2, _) = expect(&pool, start, None, restore);
        let usdg2 = sale2[0] * 1_008 / 1_000 / 1_000_000_000_000;
        vm.mock_call(TREASURY, Abi::sellCall { shares: [U256::from(sale2[0]), U256::ZERO, U256::ZERO] }.abi_encode(), U256::ZERO, Ok(U256::from(usdg2).abi_encode()));
        let before = pool.claimable_of(MARIA).to::<u128>();
        pool.settle().unwrap();
        assert_eq!(pool.revival_reserve().0, U256::ZERO);
        assert_eq!(load_cohort(&pool, b_t as u64, false).shares, c2[b_t].shares, "Bea got the month's 1/60");
        let want = usdg_raw(Fx(mul_div(usdg_fx(usdg2).unwrap().0 as u128, value(&restore, &fxp).unwrap().0 as u128, value(&sale2, &fxp).unwrap().0 as u128).unwrap() as i128));
        assert_eq!(pool.claimable_of(MARIA).to::<u128>(), before, "nothing is paid out");
        assert_eq!(pool.member(ana).4.to::<u128>(), want, "repaid at the sale price, into her own account");
        assert!(pool.member(ana).6.to::<u64>() & FLAG_RELEASED == 0);
        assert!(pool.member(ana).6.to::<u64>() & FLAG_OWED != 0, "the rest of her release is still owed");
    }

    #[test]
    fn short_epochs_scale_death_and_payout_rates_consistently() {
        let (_vm, mut pool) = setup();
        let monthly = Fx::from_ratio(4, 1000).unwrap(); // 0.4% a month
        // Default epoch is one Julian month: exact identity.
        assert_eq!(pool.per_epoch(monthly).unwrap(), monthly);
        // 28-day epochs: 30.4375/28 of them must compound back to the monthly rate.
        pool.set_params(U256::from(28 * 86_400u64), U256::from(25_000_000u64)).unwrap();
        let short = pool.per_epoch(monthly).unwrap();
        let n = Fx::from_ratio(304_375, 280_000).unwrap();
        let back = Fx::ONE.sub(Fx::ONE.sub(short).unwrap().pow(n).unwrap()).unwrap();
        let err = (back.0 - monthly.0).abs() as f64 / monthly.0 as f64;
        assert!(err < 1e-12, "compounding error {err}");
        assert!(short.0 < monthly.0, "a shorter epoch pays out less");
        // Bounds (skeptic 2): the detector's lag ring holds 7 epochs, so epochs shorter than 28 days
        // would shrink its 150-day reporting lag; they're refused, as are ones over 31 days.
        assert!(pool.set_params(U256::from(86_400u64), U256::from(25_000_000u64)).is_err());
        assert!(pool.set_params(U256::from(27 * 86_400u64), U256::from(25_000_000u64)).is_err());
        assert!(pool.set_params(U256::from(32 * 86_400u64), U256::from(25_000_000u64)).is_err());
    }

    // ---------------------------------------------------------------- regressions (skeptic 3, 2026-09-27)

    /// Two members of 1,000 USDG drawing income from 55, run for six months on the World.
    fn six_months_of_two(vm: &TestVM, heir_a: Address) -> (TontiPool, U256, U256, u64, std::rc::Rc<core::cell::RefCell<World>>) {
        const PA: Address = Address::new([0xa1; 20]);
        const PB: Address = Address::new([0xb1; 20]);
        vm.set_block_timestamp(T0);
        vm.set_sender(MARIA);
        let w = world();
        let mut pool = TontiPool::from(vm);
        pool.init(TREASURY, REGISTRY, ACTUARY, USDG).unwrap();
        pool.set_params(U256::from((YEAR_SECONDS / 12) as u64), U256::from(10_000u128 * USDG_UNIT)).unwrap();
        let a = pool.join(key(1966), 55, 2_000, false, PA, heir_a, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        let b = pool.join(key(1966), 55, 0, false, PB, HEIR, FixedBytes::ZERO, FixedBytes::ZERO, [Address::ZERO; 3]).unwrap();
        for (m, payer) in [(a, PA), (b, PB)] {
            vm.set_sender(payer);
            pool.contribute(m, U256::from(1_000 * USDG_UNIT)).unwrap();
        }
        vm.set_sender(MARIA);
        let mut now = T0 + 1;
        vm.set_block_timestamp(now);
        pool.settle().unwrap();
        for _ in 0..6 {
            now += (YEAR_SECONDS / 12) as u64;
            vm.set_block_timestamp(now);
            pool.settle().unwrap();
        }
        (pool, a, b, now, w)
    }

    fn reported_dead(vm: &TestVM, m: U256, died: u64) {
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: m }.abi_encode(), Ok(U256::from(STATUS_DECEASED).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::dateOfDeathCall { memberId: m }.abi_encode(), Ok(U256::from(died).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::reporterOfCall { memberId: m }.abi_encode(), Ok(Address::new([0xbb; 20]).abi_encode()));
    }

    fn revived(vm: &TestVM, m: U256, at: u64) {
        revived_as(vm, m, at, 1);
    }

    /// Revived, and now in registry status `status` (1 Active, 2 Due, 3 Lapsed).
    fn revived_as(vm: &TestVM, m: U256, at: u64, status: u8) {
        vm.mock_static_call(REGISTRY, Abi::statusCall { memberId: m }.abi_encode(), Ok(U256::from(status).abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::dateOfDeathCall { memberId: m }.abi_encode(), Ok(U256::ZERO.abi_encode()));
        vm.mock_static_call(REGISTRY, Abi::revivedAtCall { memberId: m }.abi_encode(), Ok(U256::from(at).abi_encode()));
    }

    /// Skeptic 3: queued dead, then revived before the settlement: she was released anyway, her
    /// unclaimed income lost (a zero date of death) and her bequest paid out. Now she is untouched.
    #[test]
    fn a_member_revived_before_the_settlement_is_not_released() {
        const HEIR_A: Address = Address::new([0xa2; 20]);
        let vm = TestVM::new();
        let (mut pool, a, _b, mut now, _w) = six_months_of_two(&vm, HEIR_A);
        let owed = pool.owed(a).unwrap();
        let (_, _, tu, bu, ..) = pool.member(a);
        reported_dead(&vm, a, now - 10 * 86_400);
        pool.mark_dead(a).unwrap();
        // Revived, and by settlement time already Lapsed again: still alive, still not released.
        revived_as(&vm, a, now, 3);
        now += (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(now);
        pool.settle().unwrap();
        let (_, _, tu2, bu2, _, _, flags) = pool.member(a);
        assert_eq!((tu2, bu2), (tu, bu), "her units are all still hers");
        assert_eq!(flags.to::<u64>() & (FLAG_RELEASED | FLAG_QUEUED_DEAD), 0, "not released, no longer queued");
        assert!(pool.owed(a).unwrap() > owed, "her income kept accruing");
        assert_eq!((pool.claimable_of(HEIR_A), pool.estate_of(a).0), (U256::ZERO, U256::ZERO), "nothing to her heir");
        assert_eq!(pool.revival_reserve().0, U256::ZERO, "nothing taken for the reserve");
        set_fallback(None);
    }

    /// Skeptic 3: a revived member got back only what the reserve held (~4% in a young pool) and the
    /// rest was erased. The rest stays owed, and later restores repay it as the reserve refills; her
    /// held estate (a reported death) goes back to her too.
    #[test]
    fn a_revival_the_reserve_cannot_cover_yet_stays_owed_and_is_repaid_later() {
        const HEIR_A: Address = Address::new([0xa2; 20]);
        let vm = TestVM::new();
        let (mut pool, a, _b, mut now, _w) = six_months_of_two(&vm, HEIR_A);
        let month = (YEAR_SECONDS / 12) as u64;
        reported_dead(&vm, a, now - 10 * 86_400);
        pool.mark_dead(a).unwrap();
        now += month;
        vm.set_block_timestamp(now);
        pool.settle().unwrap();
        assert!(pool.member(a).6.to::<u64>() & FLAG_RELEASED != 0);
        let released = pool.m_owed.get(a * U256::from(4u8)).to::<u128>();
        let estate = pool.estate_of(a).0;
        assert!(released > 0 && estate > U256::ZERO);
        // She proves she is alive. The young pool's reserve holds 5% of her release.
        revived(&vm, a, now);
        pool.restore(a).unwrap();
        now += month;
        vm.set_block_timestamp(now);
        pool.settle().unwrap();
        let flags = pool.member(a).6.to::<u64>();
        assert!(flags & FLAG_OWED != 0 && flags & FLAG_RELEASED == 0, "a member again, still owed");
        let left = pool.m_owed.get(a * U256::from(4u8)).to::<u128>();
        assert!(left > 0 && left < released, "part repaid, the rest owed: {left} of {released}");
        // Her held estate came back with the restore: the false report paid her family nothing.
        assert_eq!(pool.estate_of(a).0, U256::ZERO);
        assert_eq!(pool.claimable_of(HEIR_A), U256::ZERO, "the false report paid her family nothing");
        // The reserve refills (later deaths; ample here, since it first pays survivors its monthly
        // 1/60); the next restore repays the rest.
        set3(&mut pool.reserve, &[2 * left, 0, 0]);
        let before = pool.member(a).4.to::<u128>() + pool.member(a).2.to::<u128>();
        pool.restore(a).unwrap();
        assert!(pool.restore(a).is_err(), "queued once");
        now += month;
        vm.set_block_timestamp(now);
        pool.settle().unwrap();
        let flags = pool.member(a).6.to::<u64>();
        assert_eq!(flags & FLAG_OWED, 0, "repaid in full");
        assert_eq!(pool.m_owed.get(a * U256::from(4u8)), U256::ZERO);
        assert!(pool.member(a).4.to::<u128>() + pool.member(a).2.to::<u128>() > before, "more of her money back in her account");
        assert!(matches!(pool.restore(a), Err(PoolError::BadInput(_))), "nothing more owed");
        set_fallback(None);
    }

    /// Skeptic 3: the owner could re-pause every few days and hold settlements forever. A pause
    /// while paused changes nothing, and nobody can pause twice in 30 days.
    #[test]
    fn nobody_can_stretch_a_pause_past_seven_days() {
        let vm = TestVM::new();
        let (mut pool, _ana, _bea) = two_savers(&vm);
        let month = T0 + (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(month);
        pool.pause().unwrap(); // the owner (MARIA)
        vm.set_block_timestamp(month + 5 * DAY as u64);
        pool.pause().unwrap(); // no-op
        assert!(matches!(pool.settle_steps(0), Err(PoolError::Paused(_))));
        vm.set_block_timestamp(month + 7 * DAY as u64);
        assert!(!matches!(pool.settle_steps(0), Err(PoolError::Paused(_))), "seven days from the first pause");
        pool.unpause().unwrap();
        vm.set_block_timestamp(month + 8 * DAY as u64);
        assert!(matches!(pool.pause(), Err(PoolError::TooEarly(_))), "not even the owner, within 30 days");
        vm.set_block_timestamp(month + 29 * DAY as u64);
        assert!(matches!(pool.pause(), Err(PoolError::TooEarly(_))), "29 days is still too soon");
        vm.set_block_timestamp(month + 30 * DAY as u64);
        pool.pause().unwrap();
    }

    /// Skeptic 3 (a surviving mutation): `exit()` itself wasn't tested under a pause.
    #[test]
    fn a_pause_holds_a_matured_exit_for_at_most_seven_days() {
        let vm = TestVM::new();
        let (mut pool, ana, _bea) = two_savers(&vm);
        const GUARDIAN: Address = Address::new([0x9a; 20]);
        pool.set_guardian(GUARDIAN).unwrap();
        let at = pool.request_exit(ana).unwrap().to::<u64>();
        vm.mock_static_call(REGISTRY, Abi::canReceiveIncomeCall { memberId: ana }.abi_encode(), Ok(true.abi_encode()));
        vm.set_block_timestamp(at);
        vm.set_sender(GUARDIAN);
        pool.pause().unwrap();
        vm.set_block_timestamp(at + 6 * DAY as u64);
        assert!(matches!(pool.exit(ana), Err(PoolError::Paused(_))));
        vm.set_block_timestamp(at + 7 * DAY as u64);
        pool.exit(ana).unwrap();
    }

    // ---------------------------------------------------------------- regressions (skeptic 4, 2026-09-27)

    const PA4: Address = Address::new([0xa1; 20]);
    const HEIR4: Address = Address::new([0xa2; 20]);

    fn next_month(vm: &TestVM, pool: &mut TontiPool, now: &mut u64) {
        *now += (YEAR_SECONDS / 12) as u64;
        vm.set_block_timestamp(*now);
        pool.settle().unwrap();
    }

    fn owed_of(pool: &TontiPool, m: U256) -> u128 {
        pool.m_owed.get(m * U256::from(4u8)).to::<u128>()
    }

    /// A false death, a revival, a partial repayment (the young pool's reserve is thin), and the
    /// repayment invested: she holds units again and is still owed the rest.
    fn falsely_killed_and_partly_repaid(vm: &TestVM) -> (TontiPool, U256, u64, std::rc::Rc<core::cell::RefCell<World>>) {
        let (mut pool, a, _b, mut now, w) = six_months_of_two(vm, HEIR4);
        reported_dead(vm, a, now - 10 * 86_400);
        pool.mark_dead(a).unwrap();
        next_month(vm, &mut pool, &mut now);
        revived(vm, a, now);
        pool.restore(a).unwrap();
        next_month(vm, &mut pool, &mut now); // repaid in part, queued as her deposit
        next_month(vm, &mut pool, &mut now); // invested
        assert!(owed_of(&pool, a) > 0 && pool.member(a).2 > U256::ZERO && pool.member(a).6.to::<u64>() & FLAG_OWED != 0);
        (pool, a, now, w)
    }

    /// Skeptic 4 (HIGH): a second false death wrote over what the first still owed.
    #[test]
    fn a_second_false_death_adds_to_what_the_first_still_owes() {
        let vm = TestVM::new();
        let (mut pool, a, mut now, _w) = falsely_killed_and_partly_repaid(&vm);
        let left = owed_of(&pool, a);
        reported_dead(&vm, a, now - 86_400);
        pool.mark_dead(a).unwrap();
        next_month(&vm, &mut pool, &mut now);
        assert!(owed_of(&pool, a) > left, "the second release is added to what the first still owes");
        set_fallback(None);
    }

    /// Skeptic 4: a restore and her real death settled in the same run left her unreleased, open
    /// to deposits, holding units. She stays released, and the repayment is her estate.
    #[test]
    fn a_restore_that_meets_a_real_death_keeps_her_released() {
        let vm = TestVM::new();
        let (mut pool, a, mut now, _w) = falsely_killed_and_partly_repaid(&vm);
        let before = pool.estate_of(a).0;
        let refill = 2 * owed_of(&pool, a);
        set3(&mut pool.reserve, &[refill, 0, 0]); // the reserve has refilled
        pool.restore(a).unwrap();
        reported_dead(&vm, a, now - 86_400); // and then her death is final
        pool.mark_dead(a).unwrap();
        next_month(&vm, &mut pool, &mut now);
        let (t, _, tu, bu, pending, _, flags) = pool.member(a);
        let flags = flags.to::<u64>();
        assert!(flags & FLAG_RELEASED != 0 && flags & (FLAG_RESTORED | FLAG_COUNTED) == 0, "released, and not counted among the living");
        assert_eq!((tu, bu, pending), (U256::ZERO, U256::ZERO, U256::ZERO));
        assert_eq!(pool.c_members.get(t), U256::from(1u8));
        assert!(pool.estate_of(a).0 > before, "the repayment is part of her estate");
        vm.set_sender(PA4);
        assert!(pool.contribute(a, U256::from(10 * USDG_UNIT)).is_err(), "nobody pays in for her");
        set_fallback(None);
    }

    /// Skeptic 4: a repayment added to a deposit already queued this month was minted from this
    /// month's deposit, which never included it (shares out of thin air). It now waits a month.
    #[test]
    fn a_repayment_for_a_member_already_queued_waits_for_next_month() {
        let vm = TestVM::new();
        let (mut pool, a, mut now, _w) = falsely_killed_and_partly_repaid(&vm);
        vm.set_sender(PA4);
        pool.contribute(a, U256::from(10 * USDG_UNIT)).unwrap(); // queued this month
        vm.set_sender(MARIA);
        let refill = 2 * owed_of(&pool, a);
        set3(&mut pool.reserve, &[refill, 0, 0]);
        pool.restore(a).unwrap();
        next_month(&vm, &mut pool, &mut now);
        let (_, _, _, _, pending, _, flags) = pool.member(a);
        assert!(pending > U256::ZERO && flags.to::<u64>() & FLAG_QUEUED_DEPOSIT != 0, "the repayment waits, queued for next month");
        assert_eq!(pool.pending_total.get(), pending, "and next month invests exactly it");
        assert_eq!(pool.m_restored.get(a), U256::ZERO);
        next_month(&vm, &mut pool, &mut now);
        assert_eq!(pool.member(a).4, U256::ZERO, "invested");
        set_fallback(None);
    }

    /// Skeptic 4: a member who had left the pool, still owed, was re-enrolled at risk by a later
    /// restore. She is repaid in cash.
    #[test]
    fn a_member_who_left_is_repaid_what_she_is_owed_in_cash() {
        let vm = TestVM::new();
        let (mut pool, a, mut now, _w) = falsely_killed_and_partly_repaid(&vm);
        let f = pool.member(a).6.to::<u64>();
        pool.m_flags.insert(a, U256::from(f | FLAG_EXITED | FLAG_RELEASED)); // she has since left
        let refill = 2 * owed_of(&pool, a);
        set3(&mut pool.reserve, &[refill, 0, 0]);
        let before = pool.claimable_of(PA4);
        pool.restore(a).unwrap();
        next_month(&vm, &mut pool, &mut now);
        assert!(pool.claimable_of(PA4) > before, "paid to her own wallet");
        let f = pool.member(a).6.to::<u64>();
        assert!(f & FLAG_RELEASED != 0 && f & FLAG_EXITED != 0 && f & FLAG_QUEUED_DEPOSIT == 0, "not re-enrolled");
        assert_eq!(pool.member(a).4, U256::ZERO);
        set_fallback(None);
    }

    /// Skeptic 4 (surviving mutations): a held estate is settled only between settlements, and a
    /// revived member's only after her restore.
    #[test]
    fn a_held_estate_moves_only_between_settlements_and_after_the_restore() {
        let vm = TestVM::new();
        let (mut pool, a, _b, mut now, _w) = six_months_of_two(&vm, HEIR4);
        reported_dead(&vm, a, now - 10 * 86_400);
        pool.mark_dead(a).unwrap();
        next_month(&vm, &mut pool, &mut now);
        assert!(pool.estate_of(a).0 > U256::ZERO);
        now += (YEAR_SECONDS / 12) as u64 + 365 * 86_400;
        vm.set_block_timestamp(now);
        assert!(!pool.settle_steps(1).unwrap());
        assert!(matches!(pool.pay_estate(a), Err(PoolError::Busy(_))), "not mid-settlement");
        while !pool.settle_steps(100).unwrap() {}
        revived(&vm, a, now);
        assert!(matches!(pool.pay_estate(a), Err(PoolError::NotEligible(_))), "a revived member's estate comes back with her restore");
        set_fallback(None);
    }

    /// Skeptic 4: a false report took the income dated after it for good. It now waits with the
    /// estate: back to her if revived, to the survivors after the year if not.
    #[test]
    fn a_reported_deaths_income_after_its_date_waits_then_goes_to_the_survivors() {
        let vm = TestVM::new();
        let (mut pool, a, _b, mut now, _w) = six_months_of_two(&vm, HEIR4);
        reported_dead(&vm, a, now - 60 * 86_400);
        pool.mark_dead(a).unwrap();
        next_month(&vm, &mut pool, &mut now);
        let (estate, since, post) = pool.estate_of(a);
        assert!(post > U256::ZERO && estate > U256::ZERO, "two months of her income are dated after the report's date");
        let before = pool.unallocated_usdg.get();
        vm.set_block_timestamp(since.to::<u64>() + 365 * 86_400);
        pool.pay_estate(a).unwrap();
        assert_eq!(pool.unallocated_usdg.get(), before + post, "credited to the survivors at the next settlement");
        assert_eq!(pool.claimable_of(HEIR4), estate, "the estate to her beneficiary");
        set_fallback(None);
    }
}
