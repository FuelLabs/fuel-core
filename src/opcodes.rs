use crate::consts::RegId;
//use crate::bit_funcs::{from_u32_to_u8_recurse, set_bits_in_u32};

pub type Imm06 = u8;
pub type Imm12 = u16;
pub type Imm18 = u32;
pub type Imm24 = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[rustfmt::skip]
pub enum Opcode {
    // Arithmetic/Logic (ALU)
    Add         (RegId, RegId, RegId)            = 0x10,
    AddI        (RegId, RegId, Imm12)            = 0x11,
    And         (RegId, RegId, RegId)            = 0x12,
    AndI        (RegId, RegId, Imm12)            = 0x13,
    Div         (RegId, RegId, RegId)            = 0x14,
    DivI        (RegId, RegId, Imm12)            = 0x15,
    Eq          (RegId, RegId, RegId)            = 0x16,
    Exp         (RegId, RegId, RegId)            = 0x17,
    ExpI        (RegId, RegId, Imm12)            = 0x18,
    GT          (RegId, RegId, RegId)            = 0x19,
    LogB        (RegId, RegId, RegId)            = 0x1a,
    RootN       (RegId, RegId, RegId)            = 0x1b,
    Mod         (RegId, RegId, RegId)            = 0x1c,
    ModI        (RegId, RegId, Imm12)            = 0x1d,
    Mul         (RegId, RegId, RegId)            = 0x1e,
    MulI        (RegId, RegId, Imm12)            = 0x1f,
    Not         (RegId, RegId)                   = 0x20,
    Or          (RegId, RegId, RegId)            = 0x21,
    OrI         (RegId, RegId, Imm12)            = 0x22,
    SLL         (RegId, RegId, RegId)            = 0x23,
    SLLI        (RegId, RegId, Imm12)            = 0x24,
    SRL         (RegId, RegId, RegId)            = 0x25,
    SRLI        (RegId, RegId, Imm12)            = 0x26,
    Sub         (RegId, RegId, RegId)            = 0x27,
    SubI        (RegId, RegId, Imm12)            = 0x28,
    Xor         (RegId, RegId, RegId)            = 0x29,
    XorI        (RegId, RegId, Imm12)            = 0x2a,

    // Control Flow
    CIMV        (RegId, RegId, RegId)            = 0x30,
    CMTV        (RegId, RegId)                   = 0x31,
    JI          (Imm24)                          = 0x32,
    JNEI        (RegId, RegId, Imm12)            = 0x33,
    Return      (RegId)                          = 0x34,

    // Memory
    CFE         (RegId)                          = 0x40,
    CFS         (RegId)                          = 0x41,
    LB          (RegId, RegId, Imm12)            = 0x42,
    LW          (RegId, RegId, Imm12)            = 0x43,
    Malloc      (RegId)                          = 0x44,
    MemClear    (RegId, RegId)                   = 0x45,
    MemCp       (RegId, RegId, RegId)            = 0x46,
    MemEq       (RegId, RegId, RegId, RegId)     = 0x47,
    SB          (RegId, RegId, Imm12)            = 0x48,
    SW          (RegId, RegId, Imm12)            = 0x49,

    // Contract
    Blockhash   (RegId, RegId)                   = 0x50,
    Blockheight (RegId)                          = 0x51,
    Burn        (RegId)                          = 0x52,
    Call        (RegId, RegId, RegId, RegId)     = 0x53,
    CodeCopy    (RegId, RegId, RegId, RegId)     = 0x54,
    CodeRoot    (RegId, RegId)                   = 0x55,
    CodeSize    (RegId, RegId)                   = 0x56,
    Coinbase    (RegId)                          = 0x57,
    Loadcode    (RegId, RegId, RegId)            = 0x58,
    Log         (RegId, RegId, RegId, RegId)     = 0x59,
    Mint        (RegId)                          = 0x5a,
    Revert      (RegId)                          = 0x5b,
    SLoadCode   (RegId, RegId, RegId)            = 0x5c,
    SRW         (RegId, RegId)                   = 0x5d,
    SRWx        (RegId, RegId)                   = 0x5e,
    SWW         (RegId, RegId)                   = 0x5f,
    SWWx        (RegId, RegId)                   = 0x60,
    Transfer    (RegId, RegId, RegId)            = 0x61,
    TransferOut (RegId, RegId, RegId, RegId)     = 0x62,

    // Crypto
    ECRecover   (RegId, RegId, RegId)            = 0x70,
    Keccak256   (RegId, RegId, RegId)            = 0x71,
    Sha256      (RegId, RegId, RegId)            = 0x72,

    // Other
    Noop                                         = 0x00,
    Flag        (RegId)                          = 0x01,
    Undefined                                    = 0x0f,
}

impl From<u32> for Opcode {
    fn from(instruction: u32) -> Self {
        let op = (instruction >> 24) as u8;

        let ra = ((instruction >> 18) & 0x3f) as u8;
        let rb = ((instruction >> 12) & 0x3f) as u8;
        let rc = ((instruction >> 6) & 0x3f) as u8;
        let rd = (instruction & 0x3f) as u8;

        let _imm06 = (instruction & 0xff) as u8;
        let imm12 = (instruction & 0x0fff) as u16;
        let _imm18 = (instruction & 0x3ffff) as u32;
        let imm24 = (instruction & 0xffffff) as u32;

        match op {
            0x00 => Self::Noop,
            0x01 => Self::Flag(ra),
            0x10 => Self::Add(ra, rb, rc),
            0x11 => Self::AddI(ra, rb, imm12),
            0x12 => Self::And(ra, rb, rc),
            0x13 => Self::AndI(ra, rb, imm12),
            0x14 => Self::Div(ra, rb, rc),
            0x15 => Self::DivI(ra, rb, imm12),
            0x16 => Self::Eq(ra, rb, rc),
            0x17 => Self::Exp(ra, rb, rc),
            0x18 => Self::ExpI(ra, rb, imm12),
            0x19 => Self::GT(ra, rb, rc),
            0x1a => Self::LogB(ra, rb, rc),
            0x1b => Self::RootN(ra, rb, rc),
            0x1c => Self::Mod(ra, rb, rc),
            0x1d => Self::ModI(ra, rb, imm12),
            0x1e => Self::Mul(ra, rb, rc),
            0x1f => Self::MulI(ra, rb, imm12),
            0x20 => Self::Not(ra, rb),
            0x21 => Self::Or(ra, rb, rc),
            0x22 => Self::OrI(ra, rb, imm12),
            0x23 => Self::SLL(ra, rb, rc),
            0x24 => Self::SLLI(ra, rb, imm12),
            0x25 => Self::SRL(ra, rb, rc),
            0x26 => Self::SRLI(ra, rb, imm12),
            0x27 => Self::Sub(ra, rb, rc),
            0x28 => Self::SubI(ra, rb, imm12),
            0x29 => Self::Xor(ra, rb, rc),
            0x2a => Self::XorI(ra, rb, imm12),
            0x30 => Self::CIMV(ra, rb, rc),
            0x31 => Self::CMTV(ra, rb),
            0x32 => Self::JI(imm24),
            0x33 => Self::JNEI(ra, rb, imm12),
            0x34 => Self::Return(ra),
            0x40 => Self::CFE(ra),
            0x41 => Self::CFS(ra),
            0x42 => Self::LB(ra, rb, imm12),
            0x43 => Self::LW(ra, rb, imm12),
            0x44 => Self::Malloc(ra),
            0x45 => Self::MemClear(ra, rb),
            0x46 => Self::MemCp(ra, rb, rc),
            0x47 => Self::MemEq(ra, rb, rc, rd),
            0x48 => Self::SB(ra, rb, imm12),
            0x49 => Self::SW(ra, rb, imm12),
            0x50 => Self::Blockhash(ra, rb),
            0x51 => Self::Blockheight(ra),
            0x52 => Self::Burn(ra),
            0x53 => Self::Call(ra, rb, rc, rd),
            0x54 => Self::CodeCopy(ra, rb, rc, rd),
            0x55 => Self::CodeRoot(ra, rb),
            0x56 => Self::CodeSize(ra, rb),
            0x57 => Self::Coinbase(ra),
            0x58 => Self::Loadcode(ra, rb, rc),
            0x59 => Self::Log(ra, rb, rc, rd),
            0x5a => Self::Mint(ra),
            0x5b => Self::Revert(ra),
            0x5c => Self::SLoadCode(ra, rb, rc),
            0x5d => Self::SRW(ra, rb),
            0x5e => Self::SRWx(ra, rb),
            0x5f => Self::SWW(ra, rb),
            0x60 => Self::SWWx(ra, rb),
            0x61 => Self::Transfer(ra, rb, rc),
            0x62 => Self::TransferOut(ra, rb, rc, rd),
            0x70 => Self::ECRecover(ra, rb, rc),
            0x71 => Self::Keccak256(ra, rb, rc),
            0x72 => Self::Sha256(ra, rb, rc),
            _ => Self::Undefined,
        }
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
            Opcode::LogB(ra, rb, rc) => {
                (0x1a << 24) | ((ra as u32) << 18) | ((rb as u32) << 12) | ((rc as u32) << 6)
            }
            Opcode::RootN(ra, rb, rc) => {
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
            Opcode::CFE(ra) => (0x40 << 24) | ((ra as u32) << 18),
            Opcode::CFS(ra) => (0x41 << 24) | ((ra as u32) << 18),
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

#[test]
fn opcode_encoding() {
    let r = 0b00111111;
    let imm12 = 0b0000111111111111;
    let imm24 = 0b00000000111111111111111111111111;

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
        Opcode::LogB(r, r, r),
        Opcode::RootN(r, r, r),
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
        Opcode::CFE(r),
        Opcode::CFS(r),
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

    for op in data {
        let op_p = u32::from(op);
        let op_p = Opcode::from(op_p);

        assert_eq!(op, op_p);
    }
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
