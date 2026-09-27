//! facade-ok: the Solidity Treasury, LifeRegistry and USDG are stand-ins here, charged their Foundry
//! mainnet-fork gas. They can't all run for real in one month on a frozen fork: its Chainlink feeds go
//! stale past the Treasury's 96 h bound while a death needs ≥14 days to finalise. Disclosed in the contract.
//!
//! A Stylus host for measuring: the `vm_hooks` a Stylus program imports, priced as nitro prices
//! them (Stylus version 3), over an in-memory EVM world.
//!
//! - Opcode ink comes from the instrumented module's `stylus_ink` counter (see `meter`).
//! - Hostio ink: nitro `crates/arbutil/src/pricing.rs` (base costs, per-byte read/write prices,
//!   keccak). Ink converts to gas at the chain's `ArbWasm.inkPrice()` = 10,000 ink per gas.
//! - EVM gas: storage per EIP-2929/2200 (cold 2,100, warm 100; SSTORE 20,000 / 2,900 / 100),
//!   logs 375 + 375 per topic + 8 per byte, account access 2,600 cold / 100 warm, and memory by
//!   nitro's `arbos/programs/memory.go` (2 free pages, 1,000 gas per page, exponential ramp).
//! - Calling a Stylus program costs at least `ArbWasm.minInitGas()` = 8,832 gas plus its memory
//!   footprint. The program-specific part of init gas is only known after activation, so every
//!   figure here is a lower bound by that amount per program call.
//! - Calls to other Stylus programs in the world execute for real; calls to anything else are
//!   answered by a mock, with the gas the caller supplies (e.g. from a Foundry gas report).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use alloy_primitives::{Address, B256};
use tiny_keccak::{Hasher, Keccak};
use wasmi::{Caller, Engine, Extern, Linker, Memory, Module, Store};

pub const INK_PER_GAS: u64 = 10_000;
pub const MIN_INIT_GAS: u64 = 8_832;
const FREE_PAGES: u16 = 2;
const PAGE_GAS: u64 = 1_000;

const HOSTIO_INK: u64 = 8_400;
const PTR_INK: u64 = 13_440 - HOSTIO_INK;
const EVM_API_INK: u64 = 59_673;

fn write_price(bytes: u64) -> u64 {
    5_040 + 30 * bytes.saturating_sub(32)
}
fn read_price(bytes: u64) -> u64 {
    16_381 + 55 * bytes.saturating_sub(32)
}
fn keccak_price(bytes: u64) -> u64 {
    let words = bytes.div_ceil(32).saturating_sub(2);
    121_800 + 21_000 * words
}

const MEMORY_EXPONENTS: [u64; 129] = [
    1, 1, 1, 1, 1, 1, 2, 2, 2, 3, 3, 4, 5, 5, 6, 7, 8, 9, 11, 12, 14, 17, 19, 22, 25, 29, 33, 38, 43, 50, 57, 65, 75, 85, 98, 112, 128, 147, 168, 193,
    221, 253, 289, 331, 379, 434, 497, 569, 651, 745, 853, 976, 1117, 1279, 1463, 1675, 1917, 2194, 2511, 2874, 3290, 3765, 4309, 4932, 5645, 6461,
    7395, 8464, 9687, 11087, 12689, 14523, 16621, 19024, 21773, 24919, 28521, 32642, 37359, 42758, 48938, 56010, 64104, 73368, 83971, 96106, 109994,
    125890, 144082, 164904, 188735, 216010, 247226, 282953, 323844, 370643, 424206, 485509, 555672, 635973, 727880, 833067, 953456, 1091243, 1248941,
    1429429, 1636000, 1872423, 2143012, 2452704, 2807151, 3212820, 3677113, 4208502, 4816684, 5512756, 6309419, 7221210, 8264766, 9459129, 10826093,
    12390601, 14181199, 16230562, 18576084, 21260563, 24332984, 27849408, 31873999,
];

/// nitro `MemoryModel.GasCost`.
fn memory_gas(new: u16, open: u16, ever: u16) -> u64 {
    let new_open = open.saturating_add(new);
    let new_ever = ever.max(new_open);
    if new_ever <= FREE_PAGES {
        return 0;
    }
    let sub_free = |p: u16| p.saturating_sub(FREE_PAGES);
    let adding = sub_free(new_open).saturating_sub(sub_free(open)) as u64;
    let exp = |p: u16| MEMORY_EXPONENTS.get(p as usize).copied().unwrap_or(u64::MAX);
    adding * PAGE_GAS + (exp(new_ever) - exp(ever))
}

fn sstore_gas(original: B256, current: B256, new: B256, cold: bool) -> u64 {
    let access = if cold { 2_100 } else { 0 };
    access
        + if current == new {
            100
        } else if original == current {
            if original == B256::ZERO { 20_000 } else { 2_900 }
        } else {
            100
        }
}

pub fn keccak(data: &[u8]) -> [u8; 32] {
    let mut k = Keccak::v256();
    let mut out = [0u8; 32];
    k.update(data);
    k.finalize(&mut out);
    out
}

/// Solidity stand-in: given (to, calldata), returns (return data, gas it costs), or None to revert.
pub type Mock = Box<dyn FnMut(Address, &[u8]) -> Option<(Vec<u8>, u64)>>;

pub struct Program {
    module: Module,
    footprint: u16,
    pub storage: HashMap<B256, B256>,
}

pub struct World {
    engine: Engine,
    pub programs: HashMap<Address, Program>,
    pub mock: Mock,
    pub timestamp: u64,
    // Per transaction.
    warm_slots: HashSet<(Address, B256)>,
    warm_accounts: HashSet<Address>,
    originals: HashMap<(Address, B256), B256>,
    open_pages: u16,
    ever_pages: u16,
}

/// What one call cost.
#[derive(Clone, Debug, Default)]
pub struct Cost {
    /// Ink from executing opcodes (the metered module's counter).
    pub opcode_ink: u64,
    /// Ink charged by hostios themselves.
    pub hostio_ink: u64,
    /// EVM gas: storage, logs, account access, memory, and everything nested calls spent.
    pub evm_gas: u64,
    pub storage_reads: u64,
    pub storage_writes: u64,
    pub program_calls: u64,
    pub mock_calls: u64,
}

impl Cost {
    /// Gas, excluding the transaction's intrinsic cost.
    pub fn gas(&self) -> u64 {
        (self.opcode_ink + self.hostio_ink).div_ceil(INK_PER_GAS) + self.evm_gas
    }
}

pub struct Outcome {
    pub ok: bool,
    pub output: Vec<u8>,
    pub cost: Cost,
}

struct Ctx {
    world: Rc<RefCell<World>>,
    me: Address,
    sender: Address,
    args: Vec<u8>,
    result: Vec<u8>,
    return_data: Vec<u8>,
    cache: HashMap<B256, (B256, bool)>,
    cost: Cost,
    memory: Option<Memory>,
}

impl World {
    pub fn new() -> Rc<RefCell<World>> {
        Rc::new(RefCell::new(World {
            engine: Engine::default(),
            programs: HashMap::new(),
            mock: Box::new(|_, _| None),
            timestamp: 0,
            warm_slots: HashSet::new(),
            warm_accounts: HashSet::new(),
            originals: HashMap::new(),
            open_pages: 0,
            ever_pages: 0,
        }))
    }

    /// Adds a Stylus program (already instrumented by `meter::instrument`).
    pub fn deploy(&mut self, at: Address, instrumented: &[u8], footprint: u16) -> Result<(), wasmi::Error> {
        let module = Module::new(&self.engine, instrumented)?;
        self.programs.insert(at, Program { module, footprint, storage: HashMap::new() });
        Ok(())
    }

    /// Starts a transaction: access lists and original values reset.
    pub fn begin_tx(&mut self) {
        self.warm_slots.clear();
        self.warm_accounts.clear();
        self.originals.clear();
        self.open_pages = 0;
        self.ever_pages = 0;
    }
}

/// Runs a call to `to` from `from`. Nested program calls go through here too.
pub fn call(world: &Rc<RefCell<World>>, from: Address, to: Address, calldata: &[u8]) -> Outcome {
    let (module, footprint, engine) = {
        let w = world.borrow();
        match w.programs.get(&to) {
            Some(p) => (p.module.clone(), p.footprint, w.engine.clone()),
            None => {
                drop(w);
                let mut w = world.borrow_mut();
                let cold = w.warm_accounts.insert(to);
                let access = if cold { 2_600 } else { 100 };
                let r = (w.mock)(to, calldata);
                let cost = Cost { evm_gas: access + r.as_ref().map(|x| x.1).unwrap_or(0), mock_calls: 1, ..Cost::default() };
                return match r {
                    Some((out, _)) => Outcome { ok: true, output: out, cost },
                    None => Outcome { ok: false, output: Vec::new(), cost },
                };
            }
        }
    };
    let mut cost = Cost { program_calls: 1, ..Cost::default() };
    {
        let mut w = world.borrow_mut();
        let cold = w.warm_accounts.insert(to);
        cost.evm_gas += if cold { 2_600 } else { 100 };
        cost.evm_gas += MIN_INIT_GAS + memory_gas(footprint, w.open_pages, w.ever_pages);
        w.open_pages = w.open_pages.saturating_add(footprint);
        w.ever_pages = w.ever_pages.max(w.open_pages);
    }
    let ctx = Ctx { world: world.clone(), me: to, sender: from, args: calldata.to_vec(), result: Vec::new(), return_data: Vec::new(), cache: HashMap::new(), cost, memory: None };
    let mut store = Store::new(&engine, ctx);
    let linker = linker(&engine);
    let instance = match linker.instantiate_and_start(&mut store, &module) {
        Ok(i) => i,
        Err(e) => panic!("instantiate {to}: {e}"),
    };
    store.data_mut().memory = instance.get_memory(&store, "memory");
    let entry = instance.get_typed_func::<i32, i32>(&store, "user_entrypoint").expect("user_entrypoint");
    let status = entry.call(&mut store, calldata.len() as i32);
    let ink = match instance.get_global(&store, crate::meter::INK_EXPORT).expect("ink counter").get(&store) {
        wasmi::Val::I64(v) => v as u64,
        _ => 0,
    };
    let ok = matches!(status, Ok(0));
    let mut ctx = store.into_data();
    ctx.cost.opcode_ink += ink;
    {
        let mut w = world.borrow_mut();
        w.open_pages = w.open_pages.saturating_sub(footprint);
    }
    Outcome { ok, output: ctx.result, cost: ctx.cost }
}

fn mem(caller: &Caller<'_, Ctx>) -> Memory {
    match caller.get_export("memory") {
        Some(Extern::Memory(m)) => m,
        _ => caller.data().memory.expect("memory"),
    }
}

fn read(caller: &Caller<'_, Ctx>, ptr: i32, len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    mem(caller).read(caller, ptr as u32 as usize, &mut buf).expect("read");
    buf
}

fn write(caller: &mut Caller<'_, Ctx>, ptr: i32, data: &[u8]) {
    let m = mem(caller);
    m.write(caller, ptr as u32 as usize, data).expect("write");
}

fn linker(engine: &Engine) -> Linker<Ctx> {
    let mut l = Linker::<Ctx>::new(engine);
    let m = "vm_hooks";
    l.func_wrap(m, "read_args", |mut c: Caller<'_, Ctx>, dest: i32| {
        let args = c.data().args.clone();
        c.data_mut().cost.hostio_ink += HOSTIO_INK + write_price(args.len() as u64);
        write(&mut c, dest, &args);
    })
    .unwrap();
    l.func_wrap(m, "write_result", |mut c: Caller<'_, Ctx>, data: i32, len: i32| {
        let out = read(&c, data, len as usize);
        c.data_mut().cost.hostio_ink += HOSTIO_INK + read_price(len as u64);
        c.data_mut().result = out;
    })
    .unwrap();
    l.func_wrap(m, "msg_reentrant", |mut c: Caller<'_, Ctx>| -> i32 {
        c.data_mut().cost.hostio_ink += HOSTIO_INK;
        0
    })
    .unwrap();
    l.func_wrap(m, "msg_sender", |mut c: Caller<'_, Ctx>, dest: i32| {
        c.data_mut().cost.hostio_ink += HOSTIO_INK + PTR_INK;
        let s = c.data().sender;
        write(&mut c, dest, s.as_slice());
    })
    .unwrap();
    l.func_wrap(m, "msg_value", |mut c: Caller<'_, Ctx>, dest: i32| {
        c.data_mut().cost.hostio_ink += HOSTIO_INK + PTR_INK;
        write(&mut c, dest, &[0u8; 32]);
    })
    .unwrap();
    l.func_wrap(m, "block_timestamp", |mut c: Caller<'_, Ctx>| -> i64 {
        c.data_mut().cost.hostio_ink += HOSTIO_INK;
        c.data().world.borrow().timestamp as i64
    })
    .unwrap();
    l.func_wrap(m, "pay_for_memory_grow", |mut c: Caller<'_, Ctx>, pages: i32| {
        let gas = {
            let world = c.data().world.clone();
            let mut w = world.borrow_mut();
            let g = memory_gas(pages as u16, w.open_pages, w.ever_pages);
            w.open_pages = w.open_pages.saturating_add(pages as u16);
            w.ever_pages = w.ever_pages.max(w.open_pages);
            g
        };
        c.data_mut().cost.hostio_ink += HOSTIO_INK;
        c.data_mut().cost.evm_gas += gas;
    })
    .unwrap();
    l.func_wrap(m, "native_keccak256", |mut c: Caller<'_, Ctx>, bytes: i32, len: i32, out: i32| {
        let data = read(&c, bytes, len as usize);
        c.data_mut().cost.hostio_ink += HOSTIO_INK + keccak_price(len as u64);
        write(&mut c, out, &keccak(&data));
    })
    .unwrap();
    l.func_wrap(m, "emit_log", |mut c: Caller<'_, Ctx>, data: i32, len: i32, topics: i32| {
        c.data_mut().cost.hostio_ink += HOSTIO_INK + EVM_API_INK + read_price(len as u64);
        let data_bytes = (len as u64).saturating_sub(32 * topics as u64);
        c.data_mut().cost.evm_gas += 375 + 375 * topics as u64 + 8 * data_bytes;
        let _ = read(&c, data, len as usize);
    })
    .unwrap();
    l.func_wrap(m, "storage_load_bytes32", |mut c: Caller<'_, Ctx>, key: i32, dest: i32| {
        let k = B256::from_slice(&read(&c, key, 32));
        c.data_mut().cost.hostio_ink += HOSTIO_INK + 2 * PTR_INK;
        let v = if let Some((v, _)) = c.data().cache.get(&k) {
            *v
        } else {
            let (me, world) = (c.data().me, c.data().world.clone());
            let mut w = world.borrow_mut();
            let v = w.programs[&me].storage.get(&k).copied().unwrap_or_default();
            w.originals.entry((me, k)).or_insert(v);
            let cold = w.warm_slots.insert((me, k));
            drop(w);
            c.data_mut().cost.evm_gas += if cold { 2_100 } else { 100 };
            c.data_mut().cost.storage_reads += 1;
            c.data_mut().cache.insert(k, (v, false));
            v
        };
        write(&mut c, dest, v.as_slice());
    })
    .unwrap();
    l.func_wrap(m, "storage_cache_bytes32", |mut c: Caller<'_, Ctx>, key: i32, value: i32| {
        let k = B256::from_slice(&read(&c, key, 32));
        let v = B256::from_slice(&read(&c, value, 32));
        c.data_mut().cost.hostio_ink += HOSTIO_INK + 2 * PTR_INK;
        c.data_mut().cache.insert(k, (v, true));
    })
    .unwrap();
    l.func_wrap(m, "storage_flush_cache", |mut c: Caller<'_, Ctx>, clear: i32| {
        c.data_mut().cost.hostio_ink += HOSTIO_INK + EVM_API_INK;
        flush(&mut c);
        if clear != 0 {
            c.data_mut().cache.clear();
        }
    })
    .unwrap();
    l.func_wrap(m, "return_data_size", |mut c: Caller<'_, Ctx>| -> i32 {
        c.data_mut().cost.hostio_ink += HOSTIO_INK;
        c.data().return_data.len() as i32
    })
    .unwrap();
    l.func_wrap(m, "read_return_data", |mut c: Caller<'_, Ctx>, dest: i32, offset: i32, size: i32| -> i32 {
        let rd = c.data().return_data.clone();
        let start = (offset as usize).min(rd.len());
        let end = (start + size as usize).min(rd.len());
        c.data_mut().cost.hostio_ink += HOSTIO_INK + EVM_API_INK + write_price((end - start) as u64);
        write(&mut c, dest, &rd[start..end]);
        (end - start) as i32
    })
    .unwrap();
    fn do_call(c: &mut Caller<'_, Ctx>, contract: i32, calldata: i32, len: i32, ret_len: i32) -> i32 {
        let to = Address::from_slice(&read(c, contract, 20));
        let data = read(c, calldata, len as usize);
        c.data_mut().cost.hostio_ink += HOSTIO_INK + 3 * PTR_INK + EVM_API_INK + read_price(len as u64);
        // The SDK flushes this program's storage before calling out.
        let (world, me) = (c.data().world.clone(), c.data().me);
        let out = call(&world, me, to, &data);
        let sub = &out.cost;
        c.data_mut().cost.evm_gas += sub.gas();
        c.data_mut().cost.storage_reads += sub.storage_reads;
        c.data_mut().cost.storage_writes += sub.storage_writes;
        c.data_mut().cost.program_calls += sub.program_calls;
        c.data_mut().cost.mock_calls += sub.mock_calls;
        let n = out.output.len() as u32;
        c.data_mut().return_data = out.output;
        write(c, ret_len, &n.to_le_bytes());
        if out.ok { 0 } else { 1 }
    }
    l.func_wrap(m, "call_contract", |mut c: Caller<'_, Ctx>, contract: i32, calldata: i32, len: i32, _value: i32, _gas: i64, ret_len: i32| -> i32 {
        do_call(&mut c, contract, calldata, len, ret_len)
    })
    .unwrap();
    l.func_wrap(m, "static_call_contract", |mut c: Caller<'_, Ctx>, contract: i32, calldata: i32, len: i32, _gas: i64, ret_len: i32| -> i32 {
        do_call(&mut c, contract, calldata, len, ret_len)
    })
    .unwrap();
    l.func_wrap(m, "delegate_call_contract", |_c: Caller<'_, Ctx>, _contract: i32, _calldata: i32, _len: i32, _gas: i64, _ret_len: i32| -> i32 {
        panic!("delegate calls are not used by these contracts")
    })
    .unwrap();
    l
}

/// Writes dirty cached slots to the world, charging SSTORE per EIP-2200 with EIP-2929 access.
fn flush(c: &mut Caller<'_, Ctx>) {
    let (me, world) = (c.data().me, c.data().world.clone());
    let dirty: Vec<(B256, B256)> = c.data().cache.iter().filter(|(_, (_, d))| *d).map(|(k, (v, _))| (*k, *v)).collect();
    let mut gas = 0;
    {
        let mut w = world.borrow_mut();
        for (k, v) in &dirty {
            let current = w.programs[&me].storage.get(k).copied().unwrap_or_default();
            let original = *w.originals.entry((me, *k)).or_insert(current);
            let cold = w.warm_slots.insert((me, *k));
            gas += sstore_gas(original, current, *v, cold);
            w.programs.get_mut(&me).unwrap().storage.insert(*k, *v);
        }
    }
    let n = dirty.len() as u64;
    let ctx = c.data_mut();
    ctx.cost.evm_gas += gas;
    ctx.cost.storage_writes += n;
    for (k, v) in dirty {
        ctx.cache.insert(k, (v, false));
    }
}

/// Transaction intrinsic gas: 21,000 plus calldata (EIP-2028).
pub fn intrinsic_gas(calldata: &[u8]) -> u64 {
    21_000 + calldata.iter().map(|&b| if b == 0 { 4 } else { 16 }).sum::<u64>()
}

/// Initial memory of a module in pages (its footprint when called).
pub fn footprint(wasm: &[u8]) -> u16 {
    for payload in wasmparser::Parser::new(0).parse_all(wasm) {
        if let Ok(wasmparser::Payload::MemorySection(r)) = payload {
            for m in r.into_iter().flatten() {
                return m.initial as u16;
            }
        }
    }
    0
}

/// Runs one call to a profiled program (`meter::instrument_with(.., true)`) and returns the ink
/// each function spent in its own blocks, dearest first.
pub fn profile_call(world: &Rc<RefCell<World>>, from: Address, to: Address, calldata: &[u8]) -> Vec<(u32, u64)> {
    let (module, engine) = {
        let w = world.borrow();
        (w.programs[&to].module.clone(), w.engine.clone())
    };
    let ctx = Ctx { world: world.clone(), me: to, sender: from, args: calldata.to_vec(), result: Vec::new(), return_data: Vec::new(), cache: HashMap::new(), cost: Cost::default(), memory: None };
    let mut store = Store::new(&engine, ctx);
    let instance = linker(&engine).instantiate_and_start(&mut store, &module).expect("instantiate");
    store.data_mut().memory = instance.get_memory(&store, "memory");
    let entry = instance.get_typed_func::<i32, i32>(&store, "user_entrypoint").expect("entry");
    assert_eq!(entry.call(&mut store, calldata.len() as i32).ok(), Some(0), "call reverted");
    let mut out: Vec<(u32, u64)> = module
        .exports()
        .filter_map(|e| e.name().strip_prefix("ink_f").and_then(|i| i.parse::<u32>().ok()).map(|i| (i, e.name().to_string())))
        .filter_map(|(i, name)| match instance.get_global(&store, &name)?.get(&store) {
            wasmi::Val::I64(v) if v > 0 => Some((i, v as u64)),
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}
