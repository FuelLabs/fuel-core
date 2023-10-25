use core::iter::successors;
use ethnum::U256;
use fuel_core_types::{
    fuel_asm::{
        op,
        Instruction,
        RegId,
    },
    fuel_types::{
        RegisterId,
        Word,
    },
};

/// Allocates a byte array from heap and initializes it. Then points `reg` to it.
fn aloc_bytearray<const S: usize>(reg: u8, v: [u8; S]) -> Vec<Instruction> {
    let mut ops = vec![op::movi(reg, S as u32), op::aloc(reg)];
    for (i, b) in v.iter().enumerate() {
        if *b != 0 {
            ops.push(op::movi(reg, *b as u32));
            ops.push(op::sb(RegId::HP, reg, i as u16));
        }
    }
    ops.push(op::move_(reg, RegId::HP));
    ops
}

pub fn make_u128(reg: u8, v: u128) -> Vec<Instruction> {
    aloc_bytearray(reg, v.to_be_bytes())
}

pub fn make_u256(reg: u8, v: U256) -> Vec<Instruction> {
    aloc_bytearray(reg, v.to_be_bytes())
}

pub fn arb_dependent_cost_values() -> Vec<u32> {
    let mut linear = vec![1, 10, 100, 1000, 10_000];
    let mut l = successors(Some(100_000.0f64), |n| Some(n / 1.5))
        .take(5)
        .map(|f| f as u32)
        .collect::<Vec<_>>();
    l.sort_unstable();
    linear.extend(l);
    linear
}

/// Set a register `r` to a Word-sized number value using left-shifts
pub fn set_full_word(r: RegisterId, v: Word) -> Vec<Instruction> {
    let r = u8::try_from(r).unwrap();
    let mut ops = vec![op::movi(r, 0)];
    for byte in v.to_be_bytes() {
        ops.push(op::ori(r, r, byte as u16));
        ops.push(op::slli(r, r, 8));
    }
    ops.pop().unwrap(); // Remove last shift
    ops
}
