use crate::consts::RegisterId;
use crate::opcodes::Opcode::*;
use crate::bit_funcs::{from_u32_to_u8_recurse, set_bits_in_u32};

pub type ImmediateValue = u16;
pub type OpcodeInstruction = u32;

#[derive(Debug)]
pub enum Opcode {
    Add(RegisterId, RegisterId, RegisterId),
    Addi(RegisterId, RegisterId, ImmediateValue),
    And(RegisterId, RegisterId, RegisterId),
    Andi(RegisterId, RegisterId, ImmediateValue),
    Beq(RegisterId, RegisterId, ImmediateValue),
    Div(RegisterId, RegisterId, RegisterId),
    Divi(RegisterId, RegisterId, ImmediateValue),
    Mod(RegisterId, RegisterId, RegisterId),
    Modi(RegisterId, RegisterId, ImmediateValue),
    Eq(RegisterId, RegisterId, RegisterId),
    Gt(RegisterId, RegisterId, RegisterId),
    J(ImmediateValue),
    Jr(RegisterId),
    Lw(RegisterId, RegisterId, ImmediateValue),
    Mult(RegisterId, RegisterId, RegisterId),
    Multi(RegisterId, RegisterId, ImmediateValue),
    Noop(),
    Or(RegisterId, RegisterId, RegisterId),
    Ori(RegisterId, RegisterId, ImmediateValue),
    Sw(RegisterId, RegisterId, ImmediateValue),
    Sll(RegisterId, RegisterId, ImmediateValue),
    Sllv(RegisterId, RegisterId, RegisterId),
    Sltiu(RegisterId, RegisterId, ImmediateValue),
    Sltu(RegisterId, RegisterId, RegisterId),
    Sra(RegisterId, RegisterId, ImmediateValue),
    Srl(RegisterId, RegisterId, ImmediateValue),
    Srlv(RegisterId, RegisterId, RegisterId),
    Srav(RegisterId, RegisterId, RegisterId),
    Sub(RegisterId, RegisterId, RegisterId),
    Subi(RegisterId, RegisterId, ImmediateValue),
    Xor(RegisterId, RegisterId, RegisterId),
    Xori(RegisterId, RegisterId, ImmediateValue),
    Addmod(RegisterId, RegisterId, RegisterId, RegisterId),
    Mulmod(RegisterId, RegisterId, RegisterId, RegisterId),
    Exp(RegisterId, RegisterId, RegisterId),
    Expi(RegisterId, RegisterId, ImmediateValue),
    Halt(RegisterId),
    Stop(),

    Call(RegisterId, RegisterId),
    Malloc(RegisterId, ImmediateValue),
    Free(RegisterId),
    Savestate(),
    Ret(RegisterId),

    Push(RegisterId),
    Pop(RegisterId),

    LoadLocal(RegisterId, ImmediateValue),
    WriteLocal(RegisterId, ImmediateValue),

    CopyRegister(RegisterId, RegisterId),

    Gas(RegisterId),
    Gaslimit(RegisterId),
    Gasprice(RegisterId),
    Balance(RegisterId, RegisterId),
    Blockhash(RegisterId, RegisterId),
    Coinbase(RegisterId),
    Codesize(RegisterId, RegisterId),
    Codecopy(RegisterId, RegisterId, ImmediateValue),
    EthCall(RegisterId, RegisterId),
    Callcode(RegisterId, RegisterId),
    Delegatecall(RegisterId, RegisterId),
    Staticcall(RegisterId, RegisterId),
    Ethreturn(RegisterId, RegisterId),
    Create2(RegisterId, RegisterId, ImmediateValue),
    Revert(RegisterId, RegisterId),
    Keccak(RegisterId, RegisterId, RegisterId),
    Sha256(RegisterId, RegisterId, RegisterId),

    SetI(RegisterId, ImmediateValue),

    Rotr(RegisterId, RegisterId, RegisterId),
    Rotri(RegisterId, RegisterId, ImmediateValue),
    Rotl(RegisterId, RegisterId, RegisterId),
    Rotli(RegisterId, RegisterId, ImmediateValue),

    FuelBlockHeight(RegisterId),
    FuelRootProducer(RegisterId),
    FuelBlockProducer(RegisterId),

    Utxoid(RegisterId, ImmediateValue),
    Contractid(RegisterId),

    FtxOutputTo(RegisterId, ImmediateValue),
    FtxOutputAmount(RegisterId, ImmediateValue),
}

pub trait Programmable {
    // helpers
    fn set_opcode(v: OpcodeInstruction, o: u8) -> OpcodeInstruction;
    fn set_rd(v: OpcodeInstruction, o: u8) -> OpcodeInstruction;
    fn set_rs(v: OpcodeInstruction, o: u8) -> OpcodeInstruction;
    fn set_rt(v: OpcodeInstruction, o: u8) -> OpcodeInstruction;
    fn set_ru(v: OpcodeInstruction, o: u8) -> OpcodeInstruction;
    fn set_imm(v: OpcodeInstruction, imm: ImmediateValue) -> OpcodeInstruction;

    fn set_c(v: OpcodeInstruction, c: u8) -> OpcodeInstruction;

    fn get_rd(v: OpcodeInstruction) -> u8;
    fn get_rs(v: OpcodeInstruction) -> u8;
    fn get_rt(v: OpcodeInstruction) -> u8;
    fn get_ru(v: OpcodeInstruction) -> u8;
    fn get_imm(v: OpcodeInstruction) -> ImmediateValue;

    fn ser(self) -> OpcodeInstruction;
    fn deser(v: OpcodeInstruction) -> Opcode;
}

impl Programmable for Opcode {
    fn set_opcode(v: OpcodeInstruction, o: u8) -> OpcodeInstruction {
        return set_bits_in_u32(v, o as OpcodeInstruction, 0, 8);
    }

    fn set_rd(v: OpcodeInstruction, o: u8) -> OpcodeInstruction {
        return set_bits_in_u32(v, o as OpcodeInstruction, 8, 6);
    }
    fn get_rd(v: OpcodeInstruction) -> u8 {
        return from_u32_to_u8_recurse(v, 8, 6) as u8;
    }
    fn get_rs(v: OpcodeInstruction) -> u8 {
        return from_u32_to_u8_recurse(v, 14, 6) as u8;
    }
    fn get_rt(v: OpcodeInstruction) -> u8 {
        return from_u32_to_u8_recurse(v, 20, 6) as u8;
    }
    fn get_ru(v: OpcodeInstruction) -> u8 {
        return from_u32_to_u8_recurse(v, 26, 6) as u8;
    }
    fn get_imm(v: OpcodeInstruction) -> ImmediateValue {
        return from_u32_to_u8_recurse(v, 20, 16) as ImmediateValue;
    }


    fn set_rs(v: OpcodeInstruction, o: u8) -> OpcodeInstruction {
        return set_bits_in_u32(v, o as OpcodeInstruction, 14, 6);
    }

    fn set_rt(v: OpcodeInstruction, o: u8) -> OpcodeInstruction {
        return set_bits_in_u32(v, o as OpcodeInstruction, 20, 6);
    }

    fn set_ru(v: OpcodeInstruction, o: u8) -> OpcodeInstruction {
        return set_bits_in_u32(v, o as OpcodeInstruction, 26, 6);
    }

    fn set_imm(v: OpcodeInstruction, o: ImmediateValue) -> OpcodeInstruction {
        return set_bits_in_u32(v, o as OpcodeInstruction, 20, 16);
    }

    // used for SIMD
    fn set_c(v: OpcodeInstruction, o: u8) -> OpcodeInstruction {
        return set_bits_in_u32(v, o as OpcodeInstruction, 8, 3);
    }
    // todo - addtl simd data load

    fn ser(self) -> OpcodeInstruction {
        return match self {
            Add(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 1, 0, 8);
                println!(":: {:#b}", v);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Addi(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 3, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            And(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 5, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Andi(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 6, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Beq(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 7, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Div(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 15, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Divi(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 16, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Mod(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 20, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Modi(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 21, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Eq(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 18, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Gt(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 19, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            J(i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 17, 0, 8);
                v = Opcode::set_imm(v, i);
                v
            }
            Jr(rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 14, 0, 8);
                v = Opcode::set_rs(v, rs);
                v
            }
            Lw(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 22, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Mult(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 25, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Multi(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 26, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Noop() => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 27, 0, 8);
                v
            }
            Or(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 28, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Ori(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 29, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Sw(rs, rt, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 43, 0, 8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Sll(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 31, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Sllv(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 32, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Sltiu(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 35, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Sltu(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 36, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Sra(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 37, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Srl(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 38, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Srlv(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 39, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Srav(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 40, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Sub(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 41, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Subi(rd, rs, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 42, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, imm);
                v
            }
            Xor(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 45, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Xori(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 46, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Addmod(rd, rs, rt, ru) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 49, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v = Opcode::set_ru(v, ru as u8);
                v
            }
            Mulmod(rd, rs, rt, ru) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 50, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v = Opcode::set_ru(v, ru as u8);
                v
            }
            Exp(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 51, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Expi(rd, rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 54, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, i);
                v
            }

            Malloc(rd, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 100, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_imm(v, i);
                v
            }
            Free(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 101, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }

            Savestate() => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 102, 0, 8);
                v
            }

            Ret(rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 103, 0, 8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Call(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 104, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }

            Push(rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 105, 0, 8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }

            Pop(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 106, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }

            Halt(rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 107, 0, 8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Stop() => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 0, 0, 8);
                v
            }

            LoadLocal(rd, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 108, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_imm(v, imm);
                v
            }
            WriteLocal(rd, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 109, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_imm(v, imm);
                v
            }

            CopyRegister(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 110, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }

            Gas(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 128, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }
            Gaslimit(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 139, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }
            Gasprice(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 140, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }
            Balance(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 129, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Blockhash(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 134, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Coinbase(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 135, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }
            Codesize(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 144, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Codecopy(rd, rs, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 145, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, imm);
                v
            }
            EthCall(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 151, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Callcode(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 152, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Delegatecall(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 153, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Staticcall(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 154, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Ethreturn(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 155, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Create2(rd, rs, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 156, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, imm);
                v
            }
            Revert(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 157, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Keccak(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 158, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Sha256(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 159, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            SetI(rd, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 160, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_imm(v, imm);
                v
            }

            Rotr(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 161, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Rotri(rd, rs, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 162, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, imm);
                v
            }
            Rotl(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 163, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Rotli(rd, rs, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 164, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_imm(v, imm);
                v
            }
            FuelBlockHeight(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 165, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }
            FuelRootProducer(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 166, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }
            FuelBlockProducer(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 167, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }
            Utxoid(rd, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 168, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_imm(v, imm as u16);
                v
            }
            Contractid(rd) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 169, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v
            }
            FtxOutputTo(rd, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 170, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_imm(v, imm);
                v
            }
            FtxOutputAmount(rd, imm) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 171, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_imm(v, imm);
                v
            }
        };
    }

    fn deser(v: OpcodeInstruction) -> Opcode {
        let op: OpcodeInstruction = from_u32_to_u8_recurse(v, 0, 8);
        match op {
            0 => {
                Stop()
            }
            107 => {
                let rs = Opcode::get_rs(v);
                Halt(rs)
            }
            1 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Add(rd, rs, rt)
            }
            3 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Addi(rd, rs, i)
            }
            5 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                And(rd, rs, rt)
            }
            6 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Andi(rd, rs, i)
            }
            7 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Beq(rd, rs, i)
            }
            15 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Div(rd, rs, rt)
            }
            16 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Divi(rd, rs, i)
            }
            20 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Mod(rd, rs, rt)
            }
            21 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Modi(rd, rs, i)
            }
            17 => {
                let i = Opcode::get_imm(v);
                J(i)
            }
            14 => {
                let rs = Opcode::get_rs(v);
                Jr(rs)
            }
            18 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Eq(rd, rs, rt)
            }
            19 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Gt(rd, rs, rt)
            }
            22 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Lw(rd, rs, i)
            }
            25 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Mult(rd, rs, rt)
            }
            26 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Multi(rd, rs, i)
            }
            27 => {
                Noop()
            }
            28 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Or(rd, rs, rt)
            }
            29 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Ori(rd, rs, i)
            }
            43 => {
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                let i = Opcode::get_imm(v);
                Sw(rs, rt, i)
            }
            31 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Sll(rd, rs, i)
            }
            32 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Sllv(rd, rs, rt)
            }
            35 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Sltiu(rd, rs, i)
            }
            36 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Sltu(rd, rs, rt)
            }
            37 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Sra(rd, rs, i)
            }
            38 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Srl(rd, rs, i)
            }
            39 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Srlv(rd, rs, rt)
            }
            40 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Srav(rd, rs, rt)
            }
            41 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Sub(rd, rs, rt)
            }
            42 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let imm = Opcode::get_imm(v);
                Subi(rd, rs, imm)
            }
            45 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Xor(rd, rs, rt)
            }
            46 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Xori(rd, rs, i)
            }
            49 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                let ru = Opcode::get_ru(v);
                Addmod(rd, rs, rt, rt)
            }
            50 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                let ru = Opcode::get_ru(v);
                Mulmod(rd, rs, rt, rt)
            }
            51 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Exp(rd, rs, rt)
            }
            54 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Expi(rd, rs, i)
            }

            100 => {
                let rd = Opcode::get_rd(v);
                let i = Opcode::get_imm(v);
                Malloc(rd, i)
            }
            101 => {
                let rd = Opcode::get_rd(v);
                Free(rd)
            }

            102 => {
                Savestate()
            }
            103 => {
                let rs = Opcode::get_rs(v);
                Ret(rs)
            }
            104 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Call(rd, rs)
            }

            105 => {
                let rs = Opcode::get_rs(v);
                Push(rs)
            }
            106 => {
                let rd = Opcode::get_rd(v);
                Pop(rd)
            }

            108 => {
                let rd = Opcode::get_rd(v);
                let imm = Opcode::get_imm(v);
                LoadLocal(rd, imm)
            }
            109 => {
                let rd = Opcode::get_rd(v);
                let imm = Opcode::get_imm(v);
                WriteLocal(rd, imm)
            }

            110 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                CopyRegister(rd, rs)
            }

            128 => {
                let rd = Opcode::get_rd(v);
                Gas(rd)
            }
            129 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Balance(rd, rs)
            }
            134 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Blockhash(rd, rs)
            }
            135 => {
                let rd = Opcode::get_rd(v);
                Coinbase(rd)
            }
            139 => {
                let rd = Opcode::get_rd(v);
                Gaslimit(rd)
            }
            140 => {
                let rd = Opcode::get_rd(v);
                Gasprice(rd)
            }
            144 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Codesize(rd, rs)
            }
            145 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Codecopy(rd, rs, i)
            }
            151 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                EthCall(rd, rs)
            }
            152 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Callcode(rd, rs)
            }
            153 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Delegatecall(rd, rs)
            }
            154 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Staticcall(rd, rs)
            }
            155 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Ethreturn(rd, rs)
            }
            156 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Create2(rd, rs, i)
            }
            157 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Revert(rd, rs)
            }
            158 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Keccak(rd, rs, rt)
            }
            159 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Sha256(rd, rs, rt)
            }

            160 => {
                let rd = Opcode::get_rd(v);
                let i = Opcode::get_imm(v);
                SetI(rd, i)
            }
            161 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Rotr(rd, rs, rt)
            }
            162 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Rotri(rd, rs, i)
            }
            163 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Rotl(rd, rs, rt)
            }
            164 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let i = Opcode::get_imm(v);
                Rotli(rd, rs, i)
            }
            165 => {
                let rd = Opcode::get_rd(v);
                FuelBlockHeight(rd)
            }
            166 => {
                let rd = Opcode::get_rd(v);
                FuelRootProducer(rd)
            }
            167 => {
                let rd = Opcode::get_rd(v);
                FuelBlockProducer(rd)
            }
            168 => {
                let rd = Opcode::get_rd(v);
                let imm = Opcode::get_imm(v);
                Utxoid(rd, imm)
            }
            169 => {
                let rd = Opcode::get_rd(v);
                Contractid(rd)
            }
            170 => {
                let rd = Opcode::get_rd(v);
                let imm = Opcode::get_imm(v);
                FtxOutputTo(rd, imm)
            }
            171 => {
                let rd = Opcode::get_rd(v);
                let imm = Opcode::get_imm(v);
                FtxOutputAmount(rd, imm)
            }

            _ => {
                panic!("crash and burn");
            }
        }
    }
}