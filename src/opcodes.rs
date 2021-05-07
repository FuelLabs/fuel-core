use std::convert::TryFrom;
use std::io;

use crate::types::{Immediate06, Immediate12, Immediate18, Immediate24, RegisterId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Move        (RegisterId, RegisterId)                             = 0x1e,
    Mul         (RegisterId, RegisterId, RegisterId)                 = 0x1f,
    MulI        (RegisterId, RegisterId, Immediate12)                = 0x20,
    Not         (RegisterId, RegisterId)                             = 0x21,
    Or          (RegisterId, RegisterId, RegisterId)                 = 0x22,
    OrI         (RegisterId, RegisterId, Immediate12)                = 0x23,
    SLL         (RegisterId, RegisterId, RegisterId)                 = 0x24,
    SLLI        (RegisterId, RegisterId, Immediate12)                = 0x25,
    SRL         (RegisterId, RegisterId, RegisterId)                 = 0x26,
    SRLI        (RegisterId, RegisterId, Immediate12)                = 0x27,
    Sub         (RegisterId, RegisterId, RegisterId)                 = 0x28,
    SubI        (RegisterId, RegisterId, Immediate12)                = 0x29,
    Xor         (RegisterId, RegisterId, RegisterId)                 = 0x2a,
    XorI        (RegisterId, RegisterId, Immediate12)                = 0x2b,

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
    MemClearI   (RegisterId, Immediate18)                            = 0x46,
    MemCp       (RegisterId, RegisterId, RegisterId)                 = 0x47,
    MemEq       (RegisterId, RegisterId, RegisterId, RegisterId)     = 0x48,
    SB          (RegisterId, RegisterId, Immediate12)                = 0x49,
    SW          (RegisterId, RegisterId, Immediate12)                = 0x4a,

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
    SRWQ        (RegisterId, RegisterId)                             = 0x5e,
    SWW         (RegisterId, RegisterId)                             = 0x5f,
    SWWQ        (RegisterId, RegisterId)                             = 0x60,
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
        imm18: Immediate18,
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
            0x1e => Move(ra, rb),
            0x1f => Mul(ra, rb, rc),
            0x20 => MulI(ra, rb, imm12),
            0x21 => Not(ra, rb),
            0x22 => Or(ra, rb, rc),
            0x23 => OrI(ra, rb, imm12),
            0x24 => SLL(ra, rb, rc),
            0x25 => SLLI(ra, rb, imm12),
            0x26 => SRL(ra, rb, rc),
            0x27 => SRLI(ra, rb, imm12),
            0x28 => Sub(ra, rb, rc),
            0x29 => SubI(ra, rb, imm12),
            0x2a => Xor(ra, rb, rc),
            0x2b => XorI(ra, rb, imm12),
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
            0x46 => MemClearI(ra, imm18),
            0x47 => MemCp(ra, rb, rc),
            0x48 => MemEq(ra, rb, rc, rd),
            0x49 => SB(ra, rb, imm12),
            0x4a => SW(ra, rb, imm12),
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
            0x5e => SRWQ(ra, rb),
            0x5f => SWW(ra, rb),
            0x60 => SWWQ(ra, rb),
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
        // TODO Optimize with native architecture (eg SIMD?) or facilitate
        // auto-vectorization
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
            Opcode::Add(ra, rb, rc) => (0x10 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::AddI(ra, rb, imm12) => (0x11 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::And(ra, rb, rc) => (0x12 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::AndI(ra, rb, imm12) => (0x13 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::Div(ra, rb, rc) => (0x14 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::DivI(ra, rb, imm12) => (0x15 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::Eq(ra, rb, rc) => (0x16 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::Exp(ra, rb, rc) => (0x17 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::ExpI(ra, rb, imm12) => (0x18 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::GT(ra, rb, rc) => (0x19 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::MathLog(ra, rb, rc) => {
                (0x1a << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::MathRoot(ra, rb, rc) => {
                (0x1b << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Mod(ra, rb, rc) => (0x1c << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::ModI(ra, rb, imm12) => (0x1d << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::Move(ra, rb) => (0x1e << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::Mul(ra, rb, rc) => (0x1f << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::MulI(ra, rb, imm12) => (0x20 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::Not(ra, rb) => (0x21 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::Or(ra, rb, rc) => (0x22 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::OrI(ra, rb, imm12) => (0x23 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::SLL(ra, rb, rc) => (0x24 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::SLLI(ra, rb, imm12) => (0x25 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::SRL(ra, rb, rc) => (0x26 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::SRLI(ra, rb, imm12) => (0x27 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::Sub(ra, rb, rc) => (0x28 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::SubI(ra, rb, imm12) => (0x29 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::Xor(ra, rb, rc) => (0x2a << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::XorI(ra, rb, imm12) => (0x2b << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::CIMV(ra, rb, rc) => (0x30 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::CMTV(ra, rb) => (0x31 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::JI(imm24) => (0x32 << 24) | (imm24 as u32),
            Opcode::JNEI(ra, rb, imm12) => (0x33 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::Return(ra) => (0x34 << 24) | ((ra as u32) << 18),
            Opcode::CFEI(imm24) => (0x40 << 24) | (imm24 as u32),
            Opcode::CFSI(imm24) => (0x41 << 24) | (imm24 as u32),
            Opcode::LB(ra, rb, imm12) => (0x42 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::LW(ra, rb, imm12) => (0x43 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::Malloc(ra) => (0x44 << 24) | ((ra as u32) << 18),
            Opcode::MemClear(ra, rb) => (0x45 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::MemClearI(ra, imm18) => (0x46 << 24) | ((ra as u32) << 18) | (imm18 as u32),
            Opcode::MemCp(ra, rb, rc) => (0x47 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
            Opcode::MemEq(ra, rb, rc, rd) => {
                (0x48 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6) | (rd as u32)
            }
            Opcode::SB(ra, rb, imm12) => (0x49 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::SW(ra, rb, imm12) => (0x4a << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | (imm12 as u32),
            Opcode::Blockhash(ra, rb) => (0x50 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::Blockheight(ra) => (0x51 << 24) | ((ra as u32) << 18),
            Opcode::Burn(ra) => (0x52 << 24) | ((ra as u32) << 18),
            Opcode::Call(ra, rb, rc, rd) => {
                (0x53 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6) | (rd as u32)
            }
            Opcode::CodeCopy(ra, rb, rc, rd) => {
                (0x54 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6) | (rd as u32)
            }
            Opcode::CodeRoot(ra, rb) => (0x55 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::CodeSize(ra, rb) => (0x56 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::Coinbase(ra) => (0x57 << 24) | ((ra as u32) << 18),
            Opcode::Loadcode(ra, rb, rc) => {
                (0x58 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Log(ra, rb, rc, rd) => {
                (0x59 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6) | (rd as u32)
            }
            Opcode::Mint(ra) => (0x5a << 24) | ((ra as u32) << 18),
            Opcode::Revert(ra) => (0x5b << 24) | ((ra as u32) << 18),
            Opcode::SLoadCode(ra, rb, rc) => {
                (0x5c << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::SRW(ra, rb) => (0x5d << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::SRWQ(ra, rb) => (0x5e << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::SWW(ra, rb) => (0x5f << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::SWWQ(ra, rb) => (0x60 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12),
            Opcode::Transfer(ra, rb, rc) => {
                (0x61 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::TransferOut(ra, rb, rc, rd) => {
                (0x62 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6) | (rd as u32)
            }
            Opcode::ECRecover(ra, rb, rc) => {
                (0x70 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Keccak256(ra, rb, rc) => {
                (0x71 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::Sha256(ra, rb, rc) => (0x72 << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6),
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
