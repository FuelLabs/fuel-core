use std::{convert::TryInto, num::NonZeroU32};

use graph::runtime::gas::CONST_MAX_GAS_PER_HANDLER;
use parity_wasm::elements::Instruction;
use wasm_instrument::gas_metering::{MemoryGrowCost, Rules};

pub const GAS_COST_STORE: u32 = 2263;
pub const GAS_COST_LOAD: u32 = 1573;

pub struct GasRules;

impl Rules for GasRules {
    fn instruction_cost(&self, instruction: &Instruction) -> Option<u32> {
        use Instruction::*;
        let weight = match instruction {
            // These are taken from this post: https://github.com/paritytech/substrate/pull/7361#issue-506217103
            // from the table under the "Schedule" dropdown. Each decimal is multiplied by 10.
            // Note that those were calculated for wasi, not wasmtime, so they are likely very conservative.
            I64Const(_) => 16,
            I64Load(_, _) => GAS_COST_LOAD,
            I64Store(_, _) => GAS_COST_STORE,
            Select => 61,
            Instruction::If(_) => 79,
            Br(_) => 30,
            BrIf(_) => 63,
            BrTable(data) => 146 + data.table.len() as u32,
            Call(_) => 951,
            // TODO: To figure out the param cost we need to look up the function
            CallIndirect(_, _) => 1995,
            GetLocal(_) => 18,
            SetLocal(_) => 21,
            TeeLocal(_) => 21,
            GetGlobal(_) => 66,
            SetGlobal(_) => 107,
            CurrentMemory(_) => 23,
            GrowMemory(_) => 435000,
            I64Clz => 23,
            I64Ctz => 23,
            I64Popcnt => 29,
            I64Eqz => 24,
            I64ExtendSI32 => 22,
            I64ExtendUI32 => 22,
            I32WrapI64 => 23,
            I64Eq => 26,
            I64Ne => 25,
            I64LtS => 25,
            I64LtU => 26,
            I64GtS => 25,
            I64GtU => 25,
            I64LeS => 25,
            I64LeU => 26,
            I64GeS => 26,
            I64GeU => 25,
            I64Add => 25,
            I64Sub => 26,
            I64Mul => 25,
            I64DivS => 82,
            I64DivU => 72,
            I64RemS => 81,
            I64RemU => 73,
            I64And => 25,
            I64Or => 25,
            I64Xor => 26,
            I64Shl => 25,
            I64ShrS => 26,
            I64ShrU => 26,
            I64Rotl => 25,
            I64Rotr => 26,

            // These are similar enough to something above so just referencing a similar
            // instruction
            I32Load(_, _)
            | F32Load(_, _)
            | F64Load(_, _)
            | I32Load8S(_, _)
            | I32Load8U(_, _)
            | I32Load16S(_, _)
            | I32Load16U(_, _)
            | I64Load8S(_, _)
            | I64Load8U(_, _)
            | I64Load16S(_, _)
            | I64Load16U(_, _)
            | I64Load32S(_, _)
            | I64Load32U(_, _) => GAS_COST_LOAD,

            I32Store(_, _)
            | F32Store(_, _)
            | F64Store(_, _)
            | I32Store8(_, _)
            | I32Store16(_, _)
            | I64Store8(_, _)
            | I64Store16(_, _)
            | I64Store32(_, _) => GAS_COST_STORE,

            I32Const(_) | F32Const(_) | F64Const(_) => 16,
            I32Eqz => 26,
            I32Eq => 26,
            I32Ne => 25,
            I32LtS => 25,
            I32LtU => 26,
            I32GtS => 25,
            I32GtU => 25,
            I32LeS => 25,
            I32LeU => 26,
            I32GeS => 26,
            I32GeU => 25,
            I32Add => 25,
            I32Sub => 26,
            I32Mul => 25,
            I32DivS => 82,
            I32DivU => 72,
            I32RemS => 81,
            I32RemU => 73,
            I32And => 25,
            I32Or => 25,
            I32Xor => 26,
            I32Shl => 25,
            I32ShrS => 26,
            I32ShrU => 26,
            I32Rotl => 25,
            I32Rotr => 26,
            I32Clz => 23,
            I32Popcnt => 29,
            I32Ctz => 23,

            // Float weights not calculated by reference source material. Making up
            // some conservative values. The point here is not to be perfect but just
            // to have some reasonable upper bound.
            F64ReinterpretI64 | F32ReinterpretI32 | F64PromoteF32 | F64ConvertUI64
            | F64ConvertSI64 | F64ConvertUI32 | F64ConvertSI32 | F32DemoteF64 | F32ConvertUI64
            | F32ConvertSI64 | F32ConvertUI32 | F32ConvertSI32 | I64TruncUF64 | I64TruncSF64
            | I64TruncUF32 | I64TruncSF32 | I32TruncUF64 | I32TruncSF64 | I32TruncUF32
            | I32TruncSF32 | F64Copysign | F64Max | F64Min | F64Mul | F64Sub | F64Add
            | F64Trunc | F64Floor | F64Ceil | F64Neg | F64Abs | F64Nearest | F32Copysign
            | F32Max | F32Min | F32Mul | F32Sub | F32Add | F32Nearest | F32Trunc | F32Floor
            | F32Ceil | F32Neg | F32Abs | F32Eq | F32Ne | F32Lt | F32Gt | F32Le | F32Ge | F64Eq
            | F64Ne | F64Lt | F64Gt | F64Le | F64Ge | I32ReinterpretF32 | I64ReinterpretF64 => 100,
            F64Div | F64Sqrt | F32Div | F32Sqrt => 100,

            // More invented weights
            Block(_) => 100,
            Loop(_) => 100,
            Else => 100,
            End => 100,
            Return => 100,
            Drop => 100,
            SignExt(_) => 100,
            Nop => 1,
            Unreachable => 1,
        };
        Some(weight)
    }

    fn memory_grow_cost(&self) -> MemoryGrowCost {
        // Each page is 64KiB which is 65536 bytes.
        const PAGE: u64 = 64 * 1024;
        // 1 GB
        const GIB: u64 = 1073741824;
        // 12GiB to pages for the max memory allocation
        // In practice this will never be hit unless we also
        // free pages because this is 32bit WASM.
        const MAX_PAGES: u64 = 12 * GIB / PAGE;
        let gas_per_page =
            NonZeroU32::new((CONST_MAX_GAS_PER_HANDLER / MAX_PAGES).try_into().unwrap()).unwrap();

        MemoryGrowCost::Linear(gas_per_page)
    }
}
