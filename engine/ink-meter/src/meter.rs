//! Stylus ink metering, reproduced from Offchain Labs' nitro (Stylus version 3, the version
//! Robinhood Chain runs: `ArbWasm.stylusVersion() = 3`):
//!   - per-opcode ink: `crates/prover/src/programs/meter.rs`, `pricing_v1`
//!   - basic blocks end at End, Else, Return, Loop, Br, BrTable, BrIf, If, Call and CallIndirect
//!     (`crates/arbutil/src/operator.rs`, `ends_basic_block`)
//!   - each block is charged its ops' ink plus a 2,450-ink header when it's entered, and
//!     memory.copy / memory.fill cost 100 ink per byte on top (`crates/prover/src/programs/config.rs`)
//!
//! `instrument` rewrites a module so that an exported i64 global `stylus_ink` counts up exactly
//! what Stylus would count down.

use wasm_encoder::reencode::{Reencode, RoundtripReencoder};
use wasm_encoder::{CodeSection, ConstExpr, ExportKind, ExportSection, Function, GlobalSection, GlobalType, Instruction, RawSection, ValType};
use wasmparser::{FuncType, Operator, Parser, Payload, TypeRef};

pub const HEADER_INK: u64 = 2450;
pub const BULK_MEMORY_INK_PER_BYTE: i64 = 100;
pub const INK_EXPORT: &str = "stylus_ink";

type Error = Box<dyn std::error::Error>;

/// nitro `pricing_v1`. Floats, SIMD, atomics, references and tables are rejected at activation,
/// so they can't occur in a deployable module; they panic here.
pub fn price(op: &Operator, types: &[FuncType]) -> u64 {
    use Operator::*;
    match op {
        Unreachable | Return => 1,
        Nop | I32Const { .. } | I64Const { .. } => 1,
        Drop => 9,
        Block { .. } | Loop { .. } | Else | End => 1,
        Br { .. } | BrIf { .. } | If { .. } => 765,
        Select => 1250,
        Call { .. } => 3800,
        LocalGet { .. } | LocalTee { .. } => 75,
        LocalSet { .. } => 210,
        GlobalGet { .. } => 225,
        GlobalSet { .. } => 575,
        I32Load { .. } | I32Load8S { .. } | I32Load8U { .. } | I32Load16S { .. } | I32Load16U { .. } => 670,
        I64Load { .. } | I64Load8S { .. } | I64Load8U { .. } | I64Load16S { .. } | I64Load16U { .. } | I64Load32S { .. } | I64Load32U { .. } => 680,
        I32Store { .. } | I32Store8 { .. } | I32Store16 { .. } => 825,
        I64Store { .. } | I64Store8 { .. } | I64Store16 { .. } | I64Store32 { .. } => 950,
        MemorySize { .. } => 3000,
        MemoryGrow { .. } => 8050,
        I32Eqz | I32Eq | I32Ne | I32LtS | I32LtU | I32GtS | I32GtU | I32LeS | I32LeU | I32GeS | I32GeU => 170,
        I64Eqz | I64Eq | I64Ne | I64LtS | I64LtU | I64GtS | I64GtU | I64LeS | I64LeU | I64GeS | I64GeU => 225,
        I32Clz | I32Ctz => 210,
        I32Add | I32Sub => 70,
        I32Mul => 160,
        I32DivS | I32DivU | I32RemS | I32RemU => 1120,
        I32And | I32Or | I32Xor | I32Shl | I32ShrS | I32ShrU | I32Rotl | I32Rotr => 70,
        I64Clz | I64Ctz => 210,
        I64Add | I64Sub => 100,
        I64Mul => 160,
        I64DivS | I64DivU | I64RemS | I64RemU => 1270,
        I64And | I64Or | I64Xor | I64Shl | I64ShrS | I64ShrU | I64Rotl | I64Rotr => 100,
        I32Popcnt => 2650,
        I64Popcnt => 6000,
        I32WrapI64 | I64ExtendI32S | I64ExtendI32U => 100,
        I32Extend8S | I32Extend16S | I64Extend8S | I64Extend16S | I64Extend32S => 100,
        MemoryCopy { .. } | MemoryFill { .. } => 950,
        BrTable { targets } => 2400 + 325 * targets.len() as u64,
        CallIndirect { type_index, .. } => 13610 + 650 * types[*type_index as usize].params().len() as u64,
        other => panic!("opcode Stylus rejects at activation: {other:?}"),
    }
}

fn ends_block(op: &Operator) -> bool {
    use Operator::*;
    matches!(op, End | Else | Return | Loop { .. } | Br { .. } | BrTable { .. } | BrIf { .. } | If { .. } | Call { .. } | CallIndirect { .. })
}

/// Static facts about a module, for the report.
#[derive(Default, Debug)]
pub struct Stats {
    pub functions: u32,
    pub blocks: u64,
    pub bulk_memory_ops: u64,
}

pub fn instrument(wasm: &[u8]) -> Result<(Vec<u8>, Stats), Error> {
    instrument_with(wasm, false)
}

/// With `profile`, each defined function also gets its own counter global, exported as
/// `ink_f<index>` (its index among all functions, imports first), charged the same block costs.
pub fn instrument_with(wasm: &[u8], profile: bool) -> Result<(Vec<u8>, Stats), Error> {
    let mut out = wasm_encoder::Module::new();
    let mut re = RoundtripReencoder;
    let mut types: Vec<FuncType> = Vec::new();
    let mut imported_globals = 0u32;
    let mut defined_globals = 0u32;
    let mut code = CodeSection::new();
    let mut remaining = 0u32;
    let mut stats = Stats::default();
    let mut saw_globals = false;
    let params = function_params(wasm, &types_of(wasm)?)?;
    let n_defined = params.len() as u32;
    let mut imported_funcs = 0u32;
    // Index of the counter: after every imported and defined global.
    let ink_index = |imported: u32, defined: u32| imported + defined;

    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload?;
        match payload {
            Payload::TypeSection(reader) => {
                for group in reader.clone() {
                    for sub in group?.into_types() {
                        types.push(sub.unwrap_func().clone());
                    }
                }
                copy(&mut out, wasm, &Payload::TypeSection(reader));
            }
            Payload::ImportSection(reader) => {
                for imp in reader.clone().into_imports() {
                    match imp?.ty {
                        TypeRef::Global(_) => imported_globals += 1,
                        TypeRef::Func(_) => imported_funcs += 1,
                        _ => {}
                    }
                }
                copy(&mut out, wasm, &Payload::ImportSection(reader));
            }
            Payload::GlobalSection(reader) => {
                let mut globals = GlobalSection::new();
                for g in reader {
                    re.parse_global(&mut globals, g?)?;
                    defined_globals += 1;
                }
                let counters = if profile { 1 + n_defined } else { 1 };
                for _ in 0..counters {
                    globals.global(GlobalType { val_type: ValType::I64, mutable: true, shared: false }, &ConstExpr::i64_const(0));
                }
                out.section(&globals);
                saw_globals = true;
            }
            Payload::ExportSection(reader) => {
                if !saw_globals {
                    return Err("module has no global section; not a Stylus build".into());
                }
                let mut exports = ExportSection::new();
                for e in reader {
                    let e = e?;
                    exports.export(e.name, re.export_kind(e.kind)?, e.index);
                }
                exports.export(INK_EXPORT, ExportKind::Global, ink_index(imported_globals, defined_globals));
                if profile {
                    for f in 0..n_defined {
                        exports.export(&format!("ink_f{}", imported_funcs + f), ExportKind::Global, ink_index(imported_globals, defined_globals) + 1 + f);
                    }
                }
                out.section(&exports);
            }
            Payload::CodeSectionStart { count, .. } => {
                remaining = count;
                stats.functions = count;
            }
            Payload::CodeSectionEntry(body) => {
                let ink = ink_index(imported_globals, defined_globals);
                let mut locals: Vec<(u32, ValType)> = Vec::new();
                let mut n_locals = 0u32;
                for l in body.get_locals_reader()? {
                    let (n, t) = l?;
                    n_locals += n;
                    locals.push((n, re.val_type(t)?));
                }
                // One scratch i32 local (numbered after params and locals), for charging
                // bulk-memory ops by length.
                locals.push((1, ValType::I32));
                let mut f = Function::new(locals);
                let scratch = params[(stats.functions - remaining) as usize] + n_locals;
                let mut block: Vec<Operator> = Vec::new();
                let mut cost = 0u64;
                for op in body.get_operators_reader()? {
                    let op = op?;
                    cost += price(&op, &types);
                    let end = ends_block(&op);
                    block.push(op);
                    if end {
                        let own = ink + 1 + (stats.functions - remaining);
                        for g in if profile { vec![ink, own] } else { vec![ink] } {
                            f.instruction(&Instruction::GlobalGet(g));
                            f.instruction(&Instruction::I64Const((cost + HEADER_INK) as i64));
                            f.instruction(&Instruction::I64Add);
                            f.instruction(&Instruction::GlobalSet(g));
                        }
                        for op in block.drain(..) {
                            if matches!(op, Operator::MemoryCopy { .. } | Operator::MemoryFill { .. }) {
                                stats.bulk_memory_ops += 1;
                                // Stack: [.., len]. Charge len × 100 ink and put len back.
                                for i in [
                                    Instruction::LocalSet(scratch),
                                    Instruction::LocalGet(scratch),
                                    Instruction::I64ExtendI32U,
                                    Instruction::I64Const(BULK_MEMORY_INK_PER_BYTE),
                                    Instruction::I64Mul,
                                    Instruction::GlobalGet(ink),
                                    Instruction::I64Add,
                                    Instruction::GlobalSet(ink),
                                    Instruction::LocalGet(scratch),
                                ] {
                                    f.instruction(&i);
                                }
                            }
                            f.instruction(&re.instruction(op)?);
                        }
                        cost = 0;
                        stats.blocks += 1;
                    }
                }
                code.function(&f);
                remaining -= 1;
                if remaining == 0 {
                    out.section(&code);
                }
            }
            Payload::Version { .. } | Payload::End(_) => {}
            other => copy(&mut out, wasm, &other),
        }
    }
    Ok((out.finish(), stats))
}

fn copy(out: &mut wasm_encoder::Module, wasm: &[u8], p: &Payload) {
    if let Some((id, range)) = p.as_section() {
        out.section(&RawSection { id, data: &wasm[range] });
    }
}

fn types_of(wasm: &[u8]) -> Result<Vec<FuncType>, Error> {
    let mut types = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        if let Payload::TypeSection(r) = payload? {
            for group in r {
                for sub in group?.into_types() {
                    types.push(sub.unwrap_func().clone());
                }
            }
        }
    }
    Ok(types)
}

/// Parameter count of each defined function (its locals are numbered after them).
fn function_params(wasm: &[u8], types: &[FuncType]) -> Result<Vec<u32>, Error> {
    let mut out = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        if let Payload::FunctionSection(r) = payload? {
            for s in r {
                out.push(types[s? as usize].params().len() as u32);
            }
        }
    }
    Ok(out)
}

/// Function names from the module's name section (absent in stripped builds).
pub fn function_names(wasm: &[u8]) -> std::collections::HashMap<u32, String> {
    let mut names = std::collections::HashMap::new();
    for payload in Parser::new(0).parse_all(wasm).flatten() {
        if let Payload::CustomSection(c) = payload {
            if let wasmparser::KnownCustom::Name(reader) = c.as_known() {
                for n in reader.flatten() {
                    if let wasmparser::Name::Function(map) = n {
                        for naming in map.into_iter().flatten() {
                            names.insert(naming.index, naming.name.to_string());
                        }
                    }
                }
            }
        }
    }
    names
}
