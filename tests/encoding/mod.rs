use fuel_vm_rust::prelude::*;
use std::io::{Read, Write};

#[test]
fn opcode() {
    let r = 0x3f;
    let imm12 = 0xfff;
    let imm18 = 0x3ffff;
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
        Opcode::Move(r, r),
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
        Opcode::MemClearI(r, imm18),
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
        Opcode::SRWQ(r, r),
        Opcode::SWW(r, r),
        Opcode::SWWQ(r, r),
        Opcode::Transfer(r, r, r),
        Opcode::TransferOut(r, r, r, r),
        Opcode::ECRecover(r, r, r),
        Opcode::Keccak256(r, r, r),
        Opcode::Sha256(r, r, r),
        Opcode::Noop,
        Opcode::Flag(r),
        Opcode::Undefined,
    ];

    let mut bytes: Vec<u8> = vec![];
    let mut buffer = [0u8; 4];

    for mut op in data.clone() {
        op.read(&mut buffer).expect("Failed to write opcode to buffer");
        bytes.extend(&buffer);

        let op_p = u32::from(op);
        let op_p = Opcode::from(op_p);

        assert_eq!(op, op_p);
    }

    let mut op_p = Opcode::Undefined;
    bytes.chunks(4).zip(data.iter()).for_each(|(chunk, op)| {
        op_p.write(chunk).expect("Failed to parse opcode from chunk");

        assert_eq!(op, &op_p);
    });
}
