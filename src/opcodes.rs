use std::convert::TryFrom;
use std::io;

use crate::consts::RegisterId;
//use crate::bit_funcs::{from_u32_to_u8_recurse, set_bits_in_u32};

pub type Immediate06 = u8;
pub type Immediate12 = u16;
pub type Immediate18 = u32;
pub type Immediate24 = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[rustfmt::skip]
pub enum Opcode {
    // Arithmetic/Logic (ALU)
    Add         (RegisterId, RegisterId, RegisterId)                 = 0x10,
    AddI        (RegisterId, RegisterId, Immediate12)                = 0x11,
    And         (RegisterId, RegisterId, RegisterId)                 = 0x12,
    AndI        (RegisterId, RegisterId, Immediate12)                = 0x13,
    Div         (RegisterId, RegisterId, RegisterId)                 = 0x14,
    DivI        (RegisterId, RegisterId, Immediate12)                = 0x15,
    Eq          (RegisterId, RegisterId, RegisterId)                 = 0x16,
    Exp         (RegisterId, RegisterId, RegisterId)                 = 0x17,
    ExpI        (RegisterId, RegisterId, Immediate12)                = 0x18,
    GT          (RegisterId, RegisterId, RegisterId)                 = 0x19,
    MathLog     (RegisterId, RegisterId, RegisterId)                 = 0x1a,
    MathRoot    (RegisterId, RegisterId, RegisterId)                 = 0x1b,
    Mod         (RegisterId, RegisterId, RegisterId)                 = 0x1c,
    ModI        (RegisterId, RegisterId, Immediate12)                = 0x1d,
    Mul         (RegisterId, RegisterId, RegisterId)                 = 0x1e,
    MulI        (RegisterId, RegisterId, Immediate12)                = 0x1f,
    Not         (RegisterId, RegisterId)                             = 0x20,
    Or          (RegisterId, RegisterId, RegisterId)                 = 0x21,
    OrI         (RegisterId, RegisterId, Immediate12)                = 0x22,
    SLL         (RegisterId, RegisterId, RegisterId)                 = 0x23,
    SLLI        (RegisterId, RegisterId, Immediate12)                = 0x24,
    SRL         (RegisterId, RegisterId, RegisterId)                 = 0x25,
    SRLI        (RegisterId, RegisterId, Immediate12)                = 0x26,
    Sub         (RegisterId, RegisterId, RegisterId)                 = 0x27,
    SubI        (RegisterId, RegisterId, Immediate12)                = 0x28,
    Xor         (RegisterId, RegisterId, RegisterId)                 = 0x29,
    XorI        (RegisterId, RegisterId, Immediate12)                = 0x2a,

    // Control Flow
    CIMV        (RegisterId, RegisterId, RegisterId)                 = 0x30,
    CMTV        (RegisterId, RegisterId)                             = 0x31,
    JI          (Immediate24)                                        = 0x32,
    JNEI        (RegisterId, RegisterId, Immediate12)                = 0x33,
    Return      (RegisterId)                                         = 0x34,

    // Memory
    CFEI        (Immediate24)                                        = 0x40,
    CFSI        (Immediate24)                                        = 0x41,
    LB          (RegisterId, RegisterId, Immediate12)                = 0x42,
    LW          (RegisterId, RegisterId, Immediate12)                = 0x43,
    Malloc      (RegisterId)                                         = 0x44,
    MemClear    (RegisterId, RegisterId)                             = 0x45,
    MemCp       (RegisterId, RegisterId, RegisterId)                 = 0x46,
    MemEq       (RegisterId, RegisterId, RegisterId, RegisterId)     = 0x47,
    SB          (RegisterId, RegisterId, Immediate12)                = 0x48,
    SW          (RegisterId, RegisterId, Immediate12)                = 0x49,

    // Contract
    Blockhash   (RegisterId, RegisterId)                             = 0x50,
    Blockheight (RegisterId)                                         = 0x51,
    Burn        (RegisterId)                                         = 0x52,
    Call        (RegisterId, RegisterId, RegisterId, RegisterId)     = 0x53,
    CodeCopy    (RegisterId, RegisterId, RegisterId, RegisterId)     = 0x54,
    CodeRoot    (RegisterId, RegisterId)                             = 0x55,
    CodeSize    (RegisterId, RegisterId)                             = 0x56,
    Coinbase    (RegisterId)                                         = 0x57,
    Loadcode    (RegisterId, RegisterId, RegisterId)                 = 0x58,
    Log         (RegisterId, RegisterId, RegisterId, RegisterId)     = 0x59,
    Mint        (RegisterId)                                         = 0x5a,
    Revert      (RegisterId)                                         = 0x5b,
    SLoadCode   (RegisterId, RegisterId, RegisterId)                 = 0x5c,
    SRW         (RegisterId, RegisterId)                             = 0x5d,
    SRWx        (RegisterId, RegisterId)                             = 0x5e,
    SWW         (RegisterId, RegisterId)                             = 0x5f,
    SWWx        (RegisterId, RegisterId)                             = 0x60,
    Transfer    (RegisterId, RegisterId, RegisterId)                 = 0x61,
    TransferOut (RegisterId, RegisterId, RegisterId, RegisterId)     = 0x62,

    // Crypto
    ECRecover   (RegisterId, RegisterId, RegisterId)                 = 0x70,
    Keccak256   (RegisterId, RegisterId, RegisterId)                 = 0x71,
    Sha256      (RegisterId, RegisterId, RegisterId)                 = 0x72,

    // Other
    Noop                                                             = 0x00,
    Flag        (RegisterId)                                         = 0x01,
    Undefined                                                        = 0x0f,
}

impl Opcode {
    pub const fn new(
        op: u8,
        ra: RegisterId,
        rb: RegisterId,
        rc: RegisterId,
        rd: RegisterId,
        _imm06: Immediate06,
        imm12: Immediate12,
        _imm18: Immediate18,
        imm24: Immediate24,
    ) -> Self {
        use Opcode::*;

        match op {
            0x00 => Noop,
            0x01 => Flag(ra),
            0x10 => Add(ra, rb, rc),
            0x11 => AddI(ra, rb, imm12),
            0x12 => And(ra, rb, rc),
            0x13 => AndI(ra, rb, imm12),
            0x14 => Div(ra, rb, rc),
            0x15 => DivI(ra, rb, imm12),
            0x16 => Eq(ra, rb, rc),
            0x17 => Exp(ra, rb, rc),
            0x18 => ExpI(ra, rb, imm12),
            0x19 => GT(ra, rb, rc),
            0x1a => MathLog(ra, rb, rc),
            0x1b => MathRoot(ra, rb, rc),
            0x1c => Mod(ra, rb, rc),
            0x1d => ModI(ra, rb, imm12),
            0x1e => Mul(ra, rb, rc),
            0x1f => MulI(ra, rb, imm12),
            0x20 => Not(ra, rb),
            0x21 => Or(ra, rb, rc),
            0x22 => OrI(ra, rb, imm12),
            0x23 => SLL(ra, rb, rc),
            0x24 => SLLI(ra, rb, imm12),
            0x25 => SRL(ra, rb, rc),
            0x26 => SRLI(ra, rb, imm12),
            0x27 => Sub(ra, rb, rc),
            0x28 => SubI(ra, rb, imm12),
            0x29 => Xor(ra, rb, rc),
            0x2a => XorI(ra, rb, imm12),
            0x30 => CIMV(ra, rb, rc),
            0x31 => CMTV(ra, rb),
            0x32 => JI(imm24),
            0x33 => JNEI(ra, rb, imm12),
            0x34 => Return(ra),
            0x40 => CFEI(imm24),
            0x41 => CFSI(imm24),
            0x42 => LB(ra, rb, imm12),
            0x43 => LW(ra, rb, imm12),
            0x44 => Malloc(ra),
            0x45 => MemClear(ra, rb),
            0x46 => MemCp(ra, rb, rc),
            0x47 => MemEq(ra, rb, rc, rd),
            0x48 => SB(ra, rb, imm12),
            0x49 => SW(ra, rb, imm12),
            0x50 => Blockhash(ra, rb),
            0x51 => Blockheight(ra),
            0x52 => Burn(ra),
            0x53 => Call(ra, rb, rc, rd),
            0x54 => CodeCopy(ra, rb, rc, rd),
            0x55 => CodeRoot(ra, rb),
            0x56 => CodeSize(ra, rb),
            0x57 => Coinbase(ra),
            0x58 => Loadcode(ra, rb, rc),
            0x59 => Log(ra, rb, rc, rd),
            0x5a => Mint(ra),
            0x5b => Revert(ra),
            0x5c => SLoadCode(ra, rb, rc),
            0x5d => SRW(ra, rb),
            0x5e => SRWx(ra, rb),
            0x5f => SWW(ra, rb),
            0x60 => SWWx(ra, rb),
            0x61 => Transfer(ra, rb, rc),
            0x62 => TransferOut(ra, rb, rc, rd),
            0x70 => ECRecover(ra, rb, rc),
            0x71 => Keccak256(ra, rb, rc),
            0x72 => Sha256(ra, rb, rc),
            // TODO set panic strategy
            _ => Undefined,
        }
    }

    pub const fn gas_cost(&self) -> u64 {
        // TODO define gas costs
        1
    }
}

impl From<u32> for Opcode {
    fn from(instruction: u32) -> Self {
        // TODO Optimize with native architecture (eg SIMD?) or facilitate auto-vectorization
        let op = (instruction >> 24) as u8;

        let ra = ((instruction >> 18) & 0x3f) as RegisterId;
        let rb = ((instruction >> 12) & 0x3f) as RegisterId;
        let rc = ((instruction >> 6) & 0x3f) as RegisterId;
        let rd = (instruction & 0x3f) as RegisterId;

        let imm06 = (instruction & 0xff) as Immediate06;
        let imm12 = (instruction & 0x0fff) as Immediate12;
        let imm18 = (instruction & 0x3ffff) as Immediate18;
        let imm24 = (instruction & 0xffffff) as Immediate24;

        Self::new(op, ra, rb, rc, rd, imm06, imm12, imm18, imm24)
    }
}

impl From<Opcode> for u32 {
    fn from(opcode: Opcode) -> u32 {
        match opcode {
            Opcode::Noop => 0x00,
            Opcode::Flag(ra) => (0x01 << 24) | ((ra as u32) << 18),
            Opcode::Add(ra, rb, rc) => {
                (0x10 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::AddI(ra, rb, imm12) => {
                (0x11 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::And(ra, rb, rc) => {
                (0x12 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::AndI(ra, rb, imm12) => {
                (0x13 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::Div(ra, rb, rc) => {
                (0x14 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::DivI(ra, rb, imm12) => {
                (0x15 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::Eq(ra, rb, rc) => {
                (0x16 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Exp(ra, rb, rc) => {
                (0x17 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::ExpI(ra, rb, imm12) => {
                (0x18 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::GT(ra, rb, rc) => {
                (0x19 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::MathLog(ra, rb, rc) => {
                (0x1a << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::MathRoot(ra, rb, rc) => {
                (0x1b << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Mod(ra, rb, rc) => {
                (0x1c << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::ModI(ra, rb, imm12) => {
                (0x1d << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::Mul(ra, rb, rc) => {
                (0x1e << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::MulI(ra, rb, imm12) => {
                (0x1f << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::Not(ra, rb) => (0x20 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::Or(ra, rb, rc) => {
                (0x21 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::OrI(ra, rb, imm12) => {
                (0x22 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::SLL(ra, rb, rc) => {
                (0x23 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::SLLI(ra, rb, imm12) => {
                (0x24 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::SRL(ra, rb, rc) => {
                (0x25 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::SRLI(ra, rb, imm12) => {
                (0x26 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::Sub(ra, rb, rc) => {
                (0x27 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::SubI(ra, rb, imm12) => {
                (0x28 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::Xor(ra, rb, rc) => {
                (0x29 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::XorI(ra, rb, imm12) => {
                (0x2a << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::CIMV(ra, rb, rc) => {
                (0x30 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::CMTV(ra, rb) => (0x31 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::JI(imm24) => (0x32 << 24) | (imm24 as u32),
            Opcode::JNEI(ra, rb, imm12) => {
                (0x33 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::Return(ra) => (0x34 << 24) | ((ra as u32) << 18),
            Opcode::CFEI(imm24) => (0x40 << 24) | (imm24 as u32),
            Opcode::CFSI(imm24) => (0x41 << 24) | (imm24 as u32),
            Opcode::LB(ra, rb, imm12) => {
                (0x42 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::LW(ra, rb, imm12) => {
                (0x43 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::Malloc(ra) => (0x44 << 24) | ((ra as u32) << 18),
            Opcode::MemClear(ra, rb) => (0x45 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::MemCp(ra, rb, rc) => {
                (0x46 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::MemEq(ra, rb, rc, rd) => {
                (0x47 << 24)
                    | ((ra as u32) << 18)
                    | ((rb as u32) << 12)
                    | ((rc as u32) << 6)
                    | (rd as u32)
            }
            Opcode::SB(ra, rb, imm12) => {
                (0x48 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::SW(ra, rb, imm12) => {
                (0x49 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32)
            }
            Opcode::Blockhash(ra, rb) => (0x50 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::Blockheight(ra) => (0x51 << 24) | ((ra as u32) << 18),
            Opcode::Burn(ra) => (0x52 << 24) | ((ra as u32) << 18),
            Opcode::Call(ra, rb, rc, rd) => {
                (0x53 << 24)
                    | ((ra as u32) << 18)
                    | ((rb as u32) << 12)
                    | ((rc as u32) << 6)
                    | (rd as u32)
            }
            Opcode::CodeCopy(ra, rb, rc, rd) => {
                (0x54 << 24)
                    | ((ra as u32) << 18)
                    | ((rb as u32) << 12)
                    | ((rc as u32) << 6)
                    | (rd as u32)
            }
            Opcode::CodeRoot(ra, rb) => (0x55 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::CodeSize(ra, rb) => (0x56 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::Coinbase(ra) => (0x57 << 24) | ((ra as u32) << 18),
            Opcode::Loadcode(ra, rb, rc) => {
                (0x58 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Log(ra, rb, rc, rd) => {
                (0x59 << 24)
                    | ((ra as u32) << 18)
                    | ((rb as u32) << 12)
                    | ((rc as u32) << 6)
                    | (rd as u32)
            }
            Opcode::Mint(ra) => (0x5a << 24) | ((ra as u32) << 18),
            Opcode::Revert(ra) => (0x5b << 24) | ((ra as u32) << 18),
            Opcode::SLoadCode(ra, rb, rc) => {
                (0x5c << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::SRW(ra, rb) => (0x5d << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::SRWx(ra, rb) => (0x5e << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::SWW(ra, rb) => (0x5f << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::SWWx(ra, rb) => (0x60 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::Transfer(ra, rb, rc) => {
                (0x61 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::TransferOut(ra, rb, rc, rd) => {
                (0x62 << 24)
                    | ((ra as u32) << 18)
                    | ((rb as u32) << 12)
                    | ((rc as u32) << 6)
                    | (rd as u32)
            }
            Opcode::ECRecover(ra, rb, rc) => {
                (0x70 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Keccak256(ra, rb, rc) => {
                (0x71 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Sha256(ra, rb, rc) => {
                (0x72 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Undefined => (0x0f << 24),
        }
    }
}

impl io::Read for Opcode {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        buf.chunks_exact_mut(4)
            .next()
            .map(|chunk| chunk.copy_from_slice(&u32::from(*self).to_be_bytes()))
            .map(|_| 4)
            .ok_or(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "The provided buffer is not big enough!",
            ))
    }
}

impl io::Write for Opcode {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        buf.chunks_exact(4)
            .next()
            .ok_or(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "The provided buffer is not big enough!",
            ))
            .and_then(|chunk| <[u8; 4]>::try_from(chunk).map_err(|_| unreachable!()))
            .map(|bytes| *self = u32::from_be_bytes(bytes).into())
            .map(|_| 4)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn opcode_encoding() {
    let r = 0x3f;
    let imm12 = 0xfff;
    let imm24 = 0xffffff;

    let data = vec![
        Opcode::Add(r, r, r),
        Opcode::AddI(r, r, imm12),
        Opcode::And(r, r, r),
        Opcode::AndI(r, r, imm12),
        Opcode::Div(r, r, r),
        Opcode::DivI(r, r, imm12),
        Opcode::Eq(r, r, r),
        Opcode::Exp(r, r, r),
        Opcode::ExpI(r, r, imm12),
        Opcode::GT(r, r, r),
        Opcode::MathLog(r, r, r),
        Opcode::MathRoot(r, r, r),
        Opcode::Mod(r, r, r),
        Opcode::ModI(r, r, imm12),
        Opcode::Mul(r, r, r),
        Opcode::MulI(r, r, imm12),
        Opcode::Not(r, r),
        Opcode::Or(r, r, r),
        Opcode::OrI(r, r, imm12),
        Opcode::SLL(r, r, r),
        Opcode::SLLI(r, r, imm12),
        Opcode::SRL(r, r, r),
        Opcode::SRLI(r, r, imm12),
        Opcode::Sub(r, r, r),
        Opcode::SubI(r, r, imm12),
        Opcode::Xor(r, r, r),
        Opcode::XorI(r, r, imm12),
        Opcode::CIMV(r, r, r),
        Opcode::CMTV(r, r),
        Opcode::JI(imm24),
        Opcode::JNEI(r, r, imm12),
        Opcode::Return(r),
        Opcode::CFEI(imm24),
        Opcode::CFSI(imm24),
        Opcode::LB(r, r, imm12),
        Opcode::LW(r, r, imm12),
        Opcode::Malloc(r),
        Opcode::MemClear(r, r),
        Opcode::MemCp(r, r, r),
        Opcode::MemEq(r, r, r, r),
        Opcode::SB(r, r, imm12),
        Opcode::SW(r, r, imm12),
        Opcode::Blockhash(r, r),
        Opcode::Blockheight(r),
        Opcode::Burn(r),
        Opcode::Call(r, r, r, r),
        Opcode::CodeCopy(r, r, r, r),
        Opcode::CodeRoot(r, r),
        Opcode::CodeSize(r, r),
        Opcode::Coinbase(r),
        Opcode::Loadcode(r, r, r),
        Opcode::Log(r, r, r, r),
        Opcode::Mint(r),
        Opcode::Revert(r),
        Opcode::SLoadCode(r, r, r),
        Opcode::SRW(r, r),
        Opcode::SRWx(r, r),
        Opcode::SWW(r, r),
        Opcode::SWWx(r, r),
        Opcode::Transfer(r, r, r),
        Opcode::TransferOut(r, r, r, r),
        Opcode::ECRecover(r, r, r),
        Opcode::Keccak256(r, r, r),
        Opcode::Sha256(r, r, r),
        Opcode::Noop,
        Opcode::Flag(r),
        Opcode::Undefined,
    ];

    use std::io::{Read, Write};

    let mut bytes: Vec<u8> = vec![];
    let mut buffer = [0u8; 4];

    for mut op in data.clone() {
        op.read(&mut buffer)
            .expect("Failed to write opcode to buffer");
        bytes.extend(&buffer);

        let op_p = u32::from(op);
        let op_p = Opcode::from(op_p);

        assert_eq!(op, op_p);
    }

    let mut op_p = Opcode::Undefined;
    bytes.chunks(4).zip(data.iter()).for_each(|(chunk, op)| {
        op_p.write(chunk)
            .expect("Failed to parse opcode from chunk");

        assert_eq!(op, &op_p);
    });
}

/*
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
            Jnz(rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 180, 0, 8);
                v = Opcode::set_rs(v, rs);
                v = Opcode::set_rt(v, rt);
                v
            }
            Jnzi(rs, i) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 181, 0, 8);
                v = Opcode::set_rs(v, rs);
                v = Opcode::set_imm(v, i);
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

            Srw(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 172, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Srwx(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 173, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Sww(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 174, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Swwx(rd, rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 175, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            MemEq(rd, rs, rt, ru) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 176, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v = Opcode::set_ru(v, ru as u8);
                v
            }
            MemCp(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 177, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Ecrecover(rd, rs, rt) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 178, 0, 8);
                v = Opcode::set_rd(v, rd as u8);
                v = Opcode::set_rs(v, rs as u8);
                v = Opcode::set_rt(v, rt as u8);
                v
            }
            Cfe(rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 182, 0, 8);
                v = Opcode::set_rs(v, rs as u8);
                v
            }
            Cfs(rs) => {
                let mut v: OpcodeInstruction = 0;
                v = set_bits_in_u32(v, 183, 0, 8);
                v = Opcode::set_rs(v, rs as u8);
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

            172 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Srw(rd, rs)
            }
            173 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Srwx(rd, rs)
            }
            174 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Sww(rd, rs)
            }
            175 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Swwx(rd, rs)
            }
            176=> {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                let ru = Opcode::get_ru(v);
                MemEq(rd, rs, rt, ru)
            }
            177 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                MemCp(rd, rs, rt)
            }
            178 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                let rt = Opcode::get_rt(v);
                Ecrecover(rd, rs, rt)
            }

            180 => {
                let rd = Opcode::get_rd(v);
                let rs = Opcode::get_rs(v);
                Jnz(rd, rs)
            }
            181 => {
                let rd = Opcode::get_rd(v);
                let imm = Opcode::get_imm(v);
                Jnzi(rd, imm)
            }
            182 => {
                let rs = Opcode::get_rs(v);
                Cfe(rs)
            }
            183 => {
                let rs = Opcode::get_rs(v);
                Cfs(rs)
            }

            _ => {
                panic!("crash and burn");
            }
        }
    }
}
*/
