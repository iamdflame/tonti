//! facade-ok: the Solidity Treasury, LifeRegistry and USDG are stand-ins here, charged their Foundry
//! mainnet-fork gas. They can't all run for real in one month on a frozen fork: its Chainlink feeds go
//! stale past the Treasury's 96 h bound while a death needs ≥14 days to finalise. Disclosed in the contract.
//!
//! Gas of Tonti's Stylus contracts, measured before deployment.
//!
//!   ink-meter [members] [deaths] [page]
//!
//! Loads the contracts' deployable WASM (build them first with `cargo stylus check`, which leaves
//! them in $CARGO_TARGET_DIR/wasm32-unknown-unknown/release), instruments it exactly as Stylus v3
//! meters it (`meter`), and runs real calls against the Stylus host functions (`host`):
//!   1. the Actuary's on-chain quote at 16 to 1,024 market paths, against the 32M-gas cap;
//!   2. a TontiPool month with `members` members across many cohorts and `deaths` deaths,
//!      settled and rebalanced one item per transaction, for the gas of each kind of item.
//! Program calls between the pool and the Actuary execute for real. The Solidity Treasury,
//! LifeRegistry and USDG are stand-ins returning plausible values, charged the gas Foundry
//! measured for them against Robinhood Chain mainnet state (`forge test --gas-report`).
//!
//! Figures are lower bounds by each program's activation-specific init gas (see `host`).

mod host;
mod meter;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use alloy_primitives::{Address, FixedBytes, I256, U256};
use alloy_sol_types::{SolCall, SolValue, sol};
use host::{Outcome, World, call, intrinsic_gas};

sol! {
    interface Actuary {
        function init() external;
        function setMortalityBatch(uint256[] keys, int256[] a, int256[] b, int256[] theta, int256[] kappa) external;
        function setMarket(int256 spyReturn, int256 spyVol, int256 safeRate, int256 valuationRate) external;
        function quote(uint256 key, uint256 age, uint256 year, uint256 startAge, uint256 lumpSum, uint256 monthlyContribution, uint32 paths, bool escalating) external view returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256, uint256);
        function qMonth(uint256 key, uint256 age, uint256 year) external view returns (uint256);
        function payoutFraction(uint256 key, uint256 age, uint256 year, bool escalating) external view returns (uint256);
    }
    interface Pool {
        function init(address treasury, address registry, address actuary, address usdg) external;
        function setParams(uint256 epochLength, uint256 memberCap) external;
        function join(uint256 key, uint64 startAge, uint64 betaBps, bool escalating, address payout, address beneficiary, bytes32 qx, bytes32 qy, address[3] guardians) external returns (uint256);
        function contribute(uint256 memberId, uint256 amount) external;
        function markDead(uint256 memberId) external;
        function settleSteps(uint32 budget) external returns (bool);
        function rebalanceSteps(uint32 budget) external returns (bool);
        function runState() external view returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256);
        function counts() external view returns (uint256, uint256);
        function claim(uint256 memberId) external returns (uint256);
    }
    interface Mocked {
        function prices() external view returns (uint256[3]);
        function depositCash(uint256 usdgAmount) external returns (uint256);
        function sell(uint256[3] shares) external returns (uint256);
        function buy(uint8 sleeve, uint256 usdgIn) external returns (uint256);
        function pay(address to, uint256 usdgAmount) external;
        function enroll(uint256 memberId, uint256 key, bytes32 qx, bytes32 qy, address payout, address[3] guardians) external;
        function status(uint256 memberId) external view returns (uint8);
        function canReceiveIncome(uint256 memberId) external view returns (bool);
        function dateOfDeath(uint256 memberId) external view returns (uint64);
        function lastStrong(uint256 memberId) external view returns (uint64);
        function reporterOf(uint256 memberId) external view returns (address);
        function identified(uint256 memberId) external view returns (bool);
        function revivedAt(uint256 memberId) external view returns (uint64);
        function transferFrom(address from, address to, uint256 amount) external returns (bool);
    }
}

const OWNER: Address = Address::new([0xaa; 20]);
const ACTUARY: Address = Address::new([0x33; 20]);
const POOL: Address = Address::new([0x44; 20]);
const TREASURY: Address = Address::new([0x11; 20]);
const REGISTRY: Address = Address::new([0x22; 20]);
const USDG: Address = Address::new([0x55; 20]);
const WAD: u128 = 1_000_000_000_000_000_000;
const TX_GAS_CAP: u64 = 32_000_000; // ArbGasInfo.getGasAccountingParams() on Robinhood Chain
const GAS_PRICE_WEI: u128 = 27_280_000; // eth_gasPrice, 2026-09-26
const YEAR: u64 = 31_557_600;
const T0: u64 = 1_790_460_000; // 2026-09-26

/// Foundry gas report against mainnet state (2026-09-26): average gas per Solidity call.
fn solidity_gas(selector: [u8; 4]) -> u64 {
    match selector {
        s if s == Mocked::pricesCall::SELECTOR => 194_421,
        s if s == Mocked::depositCashCall::SELECTOR => 352_609,
        s if s == Mocked::sellCall::SELECTOR => 329_206,
        s if s == Mocked::buyCall::SELECTOR => 349_059,
        s if s == Mocked::payCall::SELECTOR => 24_185,
        s if s == Mocked::enrollCall::SELECTOR => 183_573,
        s if s == Mocked::statusCall::SELECTOR => 9_322,
        s if s == Mocked::canReceiveIncomeCall::SELECTOR => 10_585,
        s if s == Mocked::dateOfDeathCall::SELECTOR => 9_322, // same shape as `status`
        s if s == Mocked::lastStrongCall::SELECTOR => 9_322,
        s if s == Mocked::reporterOfCall::SELECTOR => 9_322,
        s if s == Mocked::identifiedCall::SELECTOR => 9_322,
        s if s == Mocked::transferFromCall::SELECTOR => 60_000, // USDG proxy, cold: an estimate
        _ => 0,
    }
}

fn prices() -> [U256; 3] {
    [U256::from(1_008u128 * WAD / 1000), U256::from(10_117u128 * WAD / 100), U256::from(77_233u128 * WAD / 100)]
}

fn q64(v: &serde_json::Value) -> I256 {
    I256::try_from(v.as_str().expect("q64 string").parse::<i128>().expect("q64")).unwrap()
}

fn fx(num: i128, den: i128) -> I256 {
    I256::try_from((num << 64) / den).unwrap()
}

fn tx(world: &Rc<RefCell<World>>, from: Address, to: Address, data: Vec<u8>) -> Outcome {
    world.borrow_mut().begin_tx();
    let out = call(world, from, to, &data);
    if !out.ok {
        panic!("call to {to} reverted: 0x{}", alloy_primitives::hex::encode(&out.output));
    }
    out
}

fn load(name: &str) -> Vec<u8> {
    let dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
    let path = format!("{dir}/wasm32-unknown-unknown/release/{name}.wasm");
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e} (build with `cargo stylus check` first)"))
}

fn deploy(world: &Rc<RefCell<World>>, at: Address, name: &str) -> meter::Stats {
    let wasm = load(name);
    let (instrumented, stats) = meter::instrument(&wasm).expect("instrument");
    world.borrow_mut().deploy(at, &instrumented, host::footprint(&wasm)).expect("deploy");
    stats
}

/// Mortality for every cohort in `keys`, from config/mortality.json.
fn load_mortality(world: &Rc<RefCell<World>>, keys: &[u64]) {
    let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/mortality.json")).unwrap()).unwrap();
    let params = &cfg["params"];
    for chunk in keys.chunks(40) {
        let (mut k, mut a, mut b, mut t, mut kp) = (vec![], vec![], vec![], vec![], vec![]);
        for key in chunk {
            let p = &params[key.to_string()]["q64"];
            k.push(U256::from(*key));
            a.push(q64(&p["A"]));
            b.push(q64(&p["B"]));
            t.push(q64(&p["theta"]));
            kp.push(q64(&p["kappa"]));
        }
        tx(world, OWNER, ACTUARY, Actuary::setMortalityBatchCall { keys: k, a, b, theta: t, kappa: kp }.abi_encode());
    }
}

fn setup_actuary(world: &Rc<RefCell<World>>, keys: &[u64]) {
    tx(world, OWNER, ACTUARY, Actuary::initCall {}.abi_encode());
    load_mortality(world, keys);
    tx(world, OWNER, ACTUARY, Actuary::setMarketCall { spyReturn: fx(6, 100), spyVol: fx(16, 100), safeRate: fx(35, 1000), valuationRate: fx(35, 1000) }.abi_encode());
}

fn key(iso: u64, sex: u64, birth: u64) -> u64 {
    (iso * 2 + sex) * 10_000 + birth
}

fn year_age(ts: u64, birth: u64) -> (U256, U256) {
    let year = 1970 * WAD + (ts as u128) * WAD / YEAR as u128;
    (U256::from(year), U256::from(year - (birth as u128 * WAD + WAD / 2)))
}

fn quotes(report: &mut serde_json::Map<String, serde_json::Value>) {
    let world = World::new();
    world.borrow_mut().timestamp = T0;
    let stats = deploy(&world, ACTUARY, "actuary_stylus");
    let mom = key(608, 0, 1966);
    setup_actuary(&world, &[mom, key(608, 0, 1990)]);
    println!("Actuary: {} functions, {} basic blocks metered", stats.functions, stats.blocks);
    println!("\n  on-chain quote (Maria's mother: 60, $3,000 + $30/mo, income from 62), gas as an eth_call:");
    let mut rows = Vec::new();
    for paths in [16u32, 64, 128, 256, 512] {
        for esc in [false, true] {
            let data = Actuary::quoteCall {
                key: U256::from(mom),
                age: U256::from(60 * WAD),
                year: U256::from(2026 * WAD + 3 * WAD / 4),
                startAge: U256::from(62 * WAD),
                lumpSum: U256::from(3_000_000_000u64),
                monthlyContribution: U256::from(30_000_000u64),
                paths,
                escalating: esc,
            }
            .abi_encode();
            let out = tx(&world, OWNER, ACTUARY, data.clone());
            let gas = out.cost.gas() + intrinsic_gas(&data);
            let r = Actuary::quoteCall::abi_decode_returns(&out.output).unwrap();
            let p50 = r._1.to::<u128>() as f64 / 1e6;
            if !esc {
                println!(
                    "    {paths:>5} paths: {:>12} gas ({:>5.1}% of the 32M cap)   P50 start ${p50:.2}   [{} Minstr-ink]",
                    gas,
                    gas as f64 / TX_GAS_CAP as f64 * 100.0,
                    out.cost.opcode_ink / 1_000_000
                );
            }
            rows.push(serde_json::json!({"paths": paths, "escalating": esc, "gas": gas, "opcode_ink": out.cost.opcode_ink, "hostio_ink": out.cost.hostio_ink, "evm_gas": out.cost.evm_gas, "p50_start_usdg": p50}));
        }
    }
    let (year, age) = year_age(T0, 1966);
    let q = tx(&world, OWNER, ACTUARY, Actuary::qMonthCall { key: U256::from(mom), age, year }.abi_encode());
    let p = tx(&world, OWNER, ACTUARY, Actuary::payoutFractionCall { key: U256::from(mom), age, year, escalating: false }.abi_encode());
    println!("  qMonth: {} gas, payoutFraction: {} gas (execution, as called by the pool)", q.cost.gas(), p.cost.gas());
    report.insert("quote".into(), serde_json::json!(rows));
    report.insert("qMonth_gas".into(), q.cost.gas().into());
    report.insert("payoutFraction_gas".into(), p.cost.gas().into());
}

/// Solidity stand-ins with the gas Foundry measured on mainnet state.
fn mocks(world: &Rc<RefCell<World>>, dead: Rc<RefCell<HashSet<u64>>>) {
    world.borrow_mut().mock = Box::new(move |to, data| {
        let sel: [u8; 4] = data[..4].try_into().ok()?;
        let gas = solidity_gas(sel);
        let p = prices();
        let out = match (to, sel) {
            (TREASURY, s) if s == Mocked::pricesCall::SELECTOR => p.abi_encode(),
            (TREASURY, s) if s == Mocked::depositCashCall::SELECTOR => {
                let c = Mocked::depositCashCall::abi_decode(data).ok()?;
                (c.usdgAmount * U256::from(1_000_000_000_000u64) * U256::from(1000u64) / U256::from(1008u64)).abi_encode()
            }
            (TREASURY, s) if s == Mocked::sellCall::SELECTOR => {
                let c = Mocked::sellCall::abi_decode(data).ok()?;
                let v: U256 = (0..3).map(|a| c.shares[a] * p[a] / U256::from(WAD)).fold(U256::ZERO, |x, y| x + y);
                (v / U256::from(1_000_000_000_000u64) * U256::from(999u64) / U256::from(1000u64)).abi_encode()
            }
            (TREASURY, s) if s == Mocked::buyCall::SELECTOR => {
                let c = Mocked::buyCall::abi_decode(data).ok()?;
                (c.usdgIn * U256::from(1_000_000_000_000u64) * U256::from(WAD) / p[c.sleeve as usize]).abi_encode()
            }
            (TREASURY, s) if s == Mocked::payCall::SELECTOR => Vec::new(),
            (REGISTRY, s) if s == Mocked::enrollCall::SELECTOR => Vec::new(),
            (REGISTRY, s) if s == Mocked::statusCall::SELECTOR => {
                let c = Mocked::statusCall::abi_decode(data).ok()?;
                U256::from(if dead.borrow().contains(&c.memberId.to::<u64>()) { 5u8 } else { 1u8 }).abi_encode()
            }
            (REGISTRY, s) if s == Mocked::canReceiveIncomeCall::SELECTOR => {
                let c = Mocked::canReceiveIncomeCall::abi_decode(data).ok()?;
                (!dead.borrow().contains(&c.memberId.to::<u64>())).abi_encode()
            }
            (REGISTRY, s) if s == Mocked::dateOfDeathCall::SELECTOR => U256::from(T0 + 10 * 86_400).abi_encode(),
            (REGISTRY, s) if s == Mocked::lastStrongCall::SELECTOR => U256::from(T0).abi_encode(),
            (REGISTRY, s) if s == Mocked::identifiedCall::SELECTOR => true.abi_encode(),
            (REGISTRY, s) if s == Mocked::revivedAtCall::SELECTOR => U256::ZERO.abi_encode(),
            // Half the deaths were reported (bounty paid), half presumed (revival reserve).
            (REGISTRY, s) if s == Mocked::reporterOfCall::SELECTOR => {
                let c = Mocked::reporterOfCall::abi_decode(data).ok()?;
                (if c.memberId.to::<u64>() % 2 == 0 { Address::with_last_byte(0xbb) } else { Address::ZERO }).abi_encode()
            }
            (USDG, s) if s == Mocked::transferFromCall::SELECTOR => true.abi_encode(),
            _ => return None,
        };
        Some((out, gas))
    });
}

#[derive(Default)]
struct Phase {
    items: u64,
    gas: u64,
    max: u64,
}

/// Pushes `sig` one item per transaction until the pool is idle; gas by the phase each item was in.
fn one_by_one(world: &Rc<RefCell<World>>, rebalance: bool, phases: &mut BTreeMap<u64, Phase>) -> u64 {
    let mut total = 0;
    loop {
        let state = call(world, OWNER, POOL, &Pool::runStateCall {}.abi_encode());
        let r = Pool::runStateCall::abi_decode_returns(&state.output).unwrap();
        let before = r._0.to::<u64>();
        let data = if rebalance { Pool::rebalanceStepsCall { budget: 1 }.abi_encode() } else { Pool::settleStepsCall { budget: 1 }.abi_encode() };
        let out = tx(world, OWNER, POOL, data.clone());
        let gas = out.cost.gas() + intrinsic_gas(&data);
        total += gas;
        let state = call(world, OWNER, POOL, &Pool::runStateCall {}.abi_encode());
        let after = Pool::runStateCall::abi_decode_returns(&state.output).unwrap()._0.to::<u64>();
        // The first call of a run also begins it; attribute that call to "begin".
        let key = if before == 0 { if rebalance { 90 } else { 0 } } else { before };
        let e = phases.entry(key).or_default();
        e.items += 1;
        e.gas += gas;
        e.max = e.max.max(gas);
        if after == 0 && Pool::settleStepsCall::abi_decode_returns(&out.output).unwrap_or(false) {
            return total;
        }
    }
}

fn phase_name(p: u64) -> &'static str {
    match p {
        0 => "begin + first release",
        1 => "release (death/exit)",
        2 => "weigh (Actuary q, payout)",
        3 => "credit + income sale",
        4 => "sell (one Treasury sale)",
        5 => "pay beneficiary",
        6 => "book income",
        7 => "invest (one deposit)",
        8 => "mint units",
        90 => "rebalance begin",
        10 => "rebalance plan",
        11 => "rebalance trade",
        12 => "rebalance apply",
        _ => "?",
    }
}

/// A pool with `members` members, invested in month 1, with `deaths` deaths queued and a top-up
/// waiting for month 2. Returns the world, the cohort count and average join / contribute gas.
fn busy_pool(members: u64, deaths: u64, quiet: bool) -> (Rc<RefCell<World>>, u64, (u64, u64)) {
    let world = World::new();
    world.borrow_mut().timestamp = T0;
    let dead = Rc::new(RefCell::new(HashSet::new()));
    mocks(&world, dead.clone());
    let stats = deploy(&world, POOL, "pool_stylus");
    deploy(&world, ACTUARY, "actuary_stylus");
    if !quiet {
        println!("\nTontiPool: {} functions, {} basic blocks metered", stats.functions, stats.blocks);
    }

    // Members spread over 3 countries × 2 sexes × birth years 1950–1999, two plans, two start ages:
    // realistic diversity, so most members open their own cohorts.
    let countries = [608u64, 360, 356]; // Philippines, Indonesia, India
    let people: Vec<(u64, u64, u64, bool, u64)> = (0..members)
        .map(|i| {
            let birth = 1950 + (i * 7) % 50;
            let (iso, sex) = (countries[(i % 3) as usize], (i / 3) % 2);
            let start = if birth < 1966 { 60 } else { 65 };
            (key(iso, sex, birth), start, if i % 4 == 0 { 2_000 } else { 0 }, i % 5 == 0, birth)
        })
        .collect();
    let mut keys: Vec<u64> = people.iter().map(|p| p.0).collect();
    keys.sort();
    keys.dedup();
    setup_actuary(&world, &keys);
    tx(&world, OWNER, POOL, Pool::initCall { treasury: TREASURY, registry: REGISTRY, actuary: ACTUARY, usdg: USDG }.abi_encode());
    tx(&world, OWNER, POOL, Pool::setParamsCall { epochLength: U256::from(YEAR / 12), memberCap: U256::from(100_000_000u64) }.abi_encode());

    let mut join_gas = 0;
    let mut contribute_gas = 0;
    for (i, (k, start, beta, esc, _)) in people.iter().enumerate() {
        let payout = Address::with_last_byte((i % 250) as u8 + 1);
        let data = Pool::joinCall { key: U256::from(*k), startAge: *start, betaBps: *beta, escalating: *esc, payout, beneficiary: payout, qx: FixedBytes::ZERO, qy: FixedBytes::ZERO, guardians: [Address::ZERO; 3] }.abi_encode();
        let out = tx(&world, OWNER, POOL, data.clone());
        join_gas += out.cost.gas() + intrinsic_gas(&data);
        let data = Pool::contributeCall { memberId: U256::from(i), amount: U256::from(25_000_000u64) }.abi_encode();
        let out = tx(&world, OWNER, POOL, data.clone());
        contribute_gas += out.cost.gas() + intrinsic_gas(&data);
    }
    let counts = Pool::countsCall::abi_decode_returns(&call(&world, OWNER, POOL, &Pool::countsCall {}.abi_encode()).output).unwrap();
    let cohorts = counts._1.to::<u64>();
    if !quiet {
        println!("  {members} members in {cohorts} cohorts; join {} gas, contribute {} gas (average)", join_gas / members, contribute_gas / members);
    }
    // Month 1: invest everyone.
    paged(&world, false, 1_000);
    // Month 2: deaths, income for the retired cohorts, a top-up.
    world.borrow_mut().timestamp = T0 + YEAR / 12;
    for m in 0..deaths {
        let id = m * (members / deaths.max(1));
        dead.borrow_mut().insert(id);
        tx(&world, OWNER, POOL, Pool::markDeadCall { memberId: U256::from(id) }.abi_encode());
    }
    tx(&world, OWNER, POOL, Pool::contributeCall { memberId: U256::from(members - 1), amount: U256::from(25_000_000u64) }.abi_encode());
    (world, cohorts, (join_gas / members, contribute_gas / members))
}

/// Settles (or rebalances) in pages of `budget`: (transactions, total gas, dearest transaction).
fn paged(world: &Rc<RefCell<World>>, rebalance: bool, budget: u32) -> (u64, u64, u64) {
    let (mut txs, mut total, mut max) = (0, 0, 0);
    loop {
        let data = if rebalance { Pool::rebalanceStepsCall { budget }.abi_encode() } else { Pool::settleStepsCall { budget }.abi_encode() };
        let out = tx(world, OWNER, POOL, data.clone());
        let gas = out.cost.gas() + intrinsic_gas(&data);
        txs += 1;
        total += gas;
        max = max.max(gas);
        if Pool::settleStepsCall::abi_decode_returns(&out.output).unwrap_or(false) {
            return (txs, total, max);
        }
    }
}

fn pool_month(members: u64, deaths: u64, page: u32, report: &mut serde_json::Map<String, serde_json::Value>) {
    let (world, cohorts, (join_gas, contribute_gas)) = busy_pool(members, deaths, false);
    let mut phases = BTreeMap::new();
    let month = one_by_one(&world, false, &mut phases);
    let mut rb = BTreeMap::new();
    let rebalance = one_by_one(&world, true, &mut rb);
    phases.extend(rb);

    println!("  month 2, one item per transaction ({deaths} deaths, {cohorts} cohorts):");
    let mut rows = serde_json::Map::new();
    let mut worst_item = 0u64;
    for (p, e) in &phases {
        println!("    {:<28} {:>5} items  avg {:>9} gas  max {:>9}", phase_name(*p), e.items, e.gas / e.items, e.max);
        rows.insert(phase_name(*p).into(), serde_json::json!({"items": e.items, "avg_gas": e.gas / e.items, "max_gas": e.max}));
        if matches!(*p, 1 | 2 | 3 | 5 | 6 | 8 | 10 | 12) {
            worst_item = worst_item.max(e.max);
        }
    }
    // A page of `budget` items costs about (one call's overhead) + budget × the dearest item.
    let _ = TX_GAS_CAP;
    println!("  one item per transaction: {} gas for the month and its rebalance", month + rebalance);
    println!("  dearest single item (with its transaction's overhead): {worst_item} gas");

    // The same month as a keeper runs it: pages of `page` items.
    let (world, ..) = busy_pool(members, deaths, true);
    let (s_tx, s_gas, s_max) = paged(&world, false, page);
    let (r_tx, r_gas, r_max) = paged(&world, true, page);
    let per_member = (s_gas + r_gas) as f64 / members as f64;
    let usd_per_eth = 4_000.0;
    let usd = per_member * GAS_PRICE_WEI as f64 / 1e18 * usd_per_eth;
    println!(
        "  in pages of {page}: settlement {s_tx} txs / {s_gas} gas (dearest {s_max}), rebalance {r_tx} txs / {r_gas} gas (dearest {r_max})"
    );
    println!(
        "  ⇒ {:.0} gas per member-month; at {:.4} gwei and $4,000/ETH ≈ ${usd:.4} per member-month",
        per_member,
        GAS_PRICE_WEI as f64 / 1e9
    );
    // Items scale linearly within a page, so the dearest page bounds the page size the cap allows.
    let dearest = s_max.max(r_max);
    let max_page = (page as u64 * (TX_GAS_CAP * 9 / 10) / dearest) as u32;
    println!("  the dearest page of {page} used {dearest} gas ⇒ pages up to ~{max_page} items stay under 90% of the 32M cap");
    report.insert(
        "pool_month".into(),
        serde_json::json!({
            "members": members, "cohorts": cohorts, "deaths": deaths,
            "join_avg_gas": join_gas, "contribute_avg_gas": contribute_gas,
            "phases_one_item_per_tx": rows, "one_item_month_gas": month + rebalance,
            "dearest_item_gas": worst_item,
            "page": page, "settle_txs": s_tx, "settle_gas": s_gas, "settle_dearest_tx": s_max,
            "rebalance_txs": r_tx, "rebalance_gas": r_gas, "rebalance_dearest_tx": r_max,
            "gas_per_member_month": per_member, "usd_per_member_month_at_4000": usd,
        }),
    );
}

/// `ink-meter profile <wasm> <paths>`: where a quote's ink goes, by function (needs an unstripped build).
fn profile(path: &str, paths: u32) {
    let wasm = std::fs::read(path).expect("wasm");
    let names = meter::function_names(&wasm);
    let (instrumented, _) = meter::instrument_with(&wasm, true).expect("instrument");
    let world = World::new();
    world.borrow_mut().timestamp = T0;
    world.borrow_mut().deploy(ACTUARY, &instrumented, host::footprint(&wasm)).expect("deploy");
    let mom = key(608, 0, 1966);
    setup_actuary(&world, &[mom]);
    let data = Actuary::quoteCall {
        key: U256::from(mom),
        age: U256::from(60 * WAD),
        year: U256::from(2026 * WAD + 3 * WAD / 4),
        startAge: U256::from(62 * WAD),
        lumpSum: U256::from(3_000_000_000u64),
        monthlyContribution: U256::from(30_000_000u64),
        paths,
        escalating: false,
    }
    .abi_encode();
    world.borrow_mut().begin_tx();
    let per = host::profile_call(&world, OWNER, ACTUARY, &data);
    let total: u64 = per.iter().map(|x| x.1).sum();
    println!("quote({paths} paths): {} Mink in opcodes; self ink by function:", total / 1_000_000);
    for (f, ink) in per.iter().take(25) {
        println!("  {:>6.2}%  {:>10} Mink  {}", *ink as f64 / total as f64 * 100.0, ink / 1_000_000, names.get(f).map(|s| s.as_str()).unwrap_or("?"));
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().map(|s| s.as_str()) == Some("profile") {
        return profile(&raw[1], raw.get(2).map(|p| p.parse().unwrap()).unwrap_or(64));
    }
    let args: Vec<u64> = raw.iter().map(|a| a.parse().expect("number")).collect();
    let members = args.first().copied().unwrap_or(300);
    let deaths = args.get(1).copied().unwrap_or(6);
    let page = args.get(2).copied().unwrap_or(20) as u32;
    let mut report = serde_json::Map::new();
    quotes(&mut report);
    pool_month(members, deaths, page, &mut report);
    // Same resolution as actuarial/paths.py: TONTI_RUNS, else the build machine's data drive, else runs/.
    let runs = std::env::var("TONTI_RUNS").unwrap_or_else(|_| {
        let uniq = "/media/dflame/UNIQ/arbit/runs";
        if std::path::Path::new(uniq).exists() { uniq.into() } else { concat!(env!("CARGO_MANIFEST_DIR"), "/../../runs").into() }
    });
    std::fs::create_dir_all(&runs).expect("runs dir");
    let out = format!("{runs}/gas.json");
    std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).expect("write report");
    println!("\nreport -> {out}");
}
