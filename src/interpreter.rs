use std::vec::Vec;
use crate::consts::*;
use crate::opcodes::{Programmable, Opcode, OpcodeInstruction};
use std::ops::{BitAnd, Div, BitOr, Shl, Shr, BitXor, Rem};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use crate::bit_funcs::{transform_from_u8_to_u32, transform_from_u64_to_u8, transform_from_u8_to_u64};
use keccak_hash::keccak;
use tiny_keccak::{Sha3, Hasher as sha3hasher};

#[derive(Clone)]
pub struct VM {
    pub fuel_tx: Ftx,
    pub memory: [MemWord; FUEL_MAX_MEMORY_SIZE as usize],
    pub program: Program,
    pub hi: u64,
    pub ret: u64,
    state: u8,
    error: u8,
    of: MemWord,
    registers: [MemWord; VM_REGISTER_COUNT as usize],
    pub callframes: Vec<CallFrame>,
    pub store: Box<HashMap<u64, u64>>,
}

impl VM {
    pub fn new() -> VM {

        // some initialization code
        let mut new_callframes: Vec<CallFrame> = Vec::new();
        new_callframes.push(CallFrame::new());

        VM {
            fuel_tx: Default::default(),
            memory: [3 as MemWord; FUEL_MAX_MEMORY_SIZE as usize],
            program: Program::new(),
            hi: 0u64,
            ret: 0u64,
            state: 1u8,
            error: 0u8,
            of: 0u64,
            registers: [3 as MemWord; VM_REGISTER_COUNT as usize],
            callframes: new_callframes,
            store: Box::new(HashMap::new() as HashMap<u64, u64>),
        }
    }

    pub fn pop_from_latest_framestack(&mut self) -> Option<MemWord> {
        self.get_framestack().last_mut().unwrap().stack.pop()
    }

    pub fn push_to_latest_framestack(&mut self, v: MemWord) {
        self.get_framestack().last_mut().unwrap().stack.push(v);
    }

    pub fn get_top_frame(&self) -> Option<&CallFrame> {
        self.callframes.last()
    }

    pub fn get_framestack(&mut self) -> &mut Vec<CallFrame> {
        &mut self.callframes
    }

    pub fn get_registers(&self) -> [MemWord; VM_REGISTER_COUNT as usize] {
        self.registers
    }

    pub fn set_pc(&mut self, v: u8) {
        if let Some(stackframe) = self.get_framestack().last_mut() {
            stackframe.pc = v;
        } else {
            // really an error if we reach this part
            panic!("Error: no pc found because there is no frame");
        }
    }

    pub fn get_pc(&self) -> u8 {
        if let Some(stackframe) = self.get_top_frame() {
            return stackframe.pc;
        }
        // really an error if we reach this part
        panic!("Error: no pc found because there is no frame");
    }

    pub fn get_state(&self) -> u8 {
        self.state
    }

    pub fn get_error(&self) -> u8 {
        self.error
    }

    pub fn get_of(&mut self) -> u64 {
        self.of
    }

    pub fn set_of(&mut self, v: u64) {
        self.of = v;
    }

    pub fn set_of_b(&mut self, b: bool) {
        if b {
            self.of = 8;
        } else {
            self.of = 7;
        }
    }

    pub fn set_state(&mut self, new_state: u8) {
        if new_state > (1 << 2) {
            // internal error
        }
        self.state = new_state
    }

    pub fn set_error(&mut self, new_error: u8) {
        if new_error > (1 << 4) {
            // internal error
        }
        self.error = new_error
    }

    pub fn get_register_value(&self, rid: RegisterId) -> MemWord {
        if rid > VM_REGISTER_COUNT {
            return 0;
        }
        let r = self.registers[rid as usize];
        r
    }

    pub fn set_register_value(&mut self, rid: RegisterId, v: MemWord) {
        if v > (1 << 5) {
            // set error flag for out-of-memory-range
            self.error = 1;
        }
        self.registers[rid as usize] = v;
    }

    pub fn dump_memory(&self) {
        self.dump_memory_range(0, FUEL_MAX_MEMORY_SIZE);
    }

    pub fn dump_memory_range(&self, left: u8, right: u8) {
        let mut i = left;
        for b in left..right {
            println!("__{}: {:b}", i, self.memory[b as usize]);
            i += 1;
        }
    }

    pub fn dump_registers(&self) {
        let mut i = 0;
        for b in 0..self.registers.len() {
            println!("reg__{}: {:b}", i, self.registers[b as usize]);
            i += 1;
        }
    }

    pub fn set_memory(&mut self, address: u8, v: u64) {
        self.memory[address as usize] = v;
    }
    pub fn get_memory(&self, address: u8) -> u64 {
        self.memory[address as usize]
    }

    pub fn create_and_push_new_frame(&mut self, pc: u8, num_params: u8) {
        let mut new_frame: CallFrame;

        if let Some(current_stack_frame) = self.callframes.last() {
            new_frame = current_stack_frame.clone();

            new_frame.args.clear();
            if num_params <= new_frame.stack.len() as u8 {
                let (_, right) = &new_frame.stack.split_at(new_frame.stack.len() - num_params as usize);
                new_frame.stack = Vec::from(*right);

                for i in 0..num_params {
                    new_frame.args.push(*current_stack_frame.stack.get(current_stack_frame.stack.len() - 1 - i as usize).unwrap());
                }
            } else {
                panic!("Invalid number of args found for call!");
            }
        } else {
            new_frame = CallFrame::new();
        }

        new_frame.pc = pc as u8;
        &self.callframes.push(new_frame);
    }

    pub fn release_frame(&mut self) -> Option<CallFrame> {
        self.callframes.pop()
    }


    pub fn run(&mut self) {
        loop {
            // currently, break if any error is reported
            if self.get_error() > 0 {
                break;
            }
            if self.state == 0 {
                break;
            }

            if let Some(i) = self.program.code.get(self.get_pc() as usize) {
                let o = Opcode::deser(*i);
                println!("Executing op: {:?}", &o);
                self.execute(o);

                self.dump_registers();
                self.dump_memory();

                continue;
            }
            break;
        }

        self.dump_memory();
    }

    pub fn execute(&mut self, o: Opcode) {
        let vm = self;
        match o {
            Opcode::Add(rd, rs, rt) => {
                let v1 = vm.get_register_value(rs);
                let v2 = vm.get_register_value(rt);
                let (r, b) = v1.overflowing_add(v2);
                vm.set_register_value(rd, r);
                // set overflow
                vm.set_of_b(b);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Addi(rd, rs, imm) => {
                let v1 = vm.get_register_value(rs);
                let sign = imm & 1 << 15;
                let mut x: u64 = imm as u64;
                if sign > 0 {
                    x = (u64::MAX - (1 << 16 - 1)) | imm as u64;
                }
                let (r, b) = v1.overflowing_add(x);
                vm.set_register_value(rd, r);
                // set overflow
                vm.set_of_b(b);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::And(rd, rs, rt) => {
                let v1 = vm.get_register_value(rs);
                let v2 = vm.get_register_value(rt);
                let r = v1.bitand(v2);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Andi(rd, rs, imm) => {
                let v1 = vm.get_register_value(rs);
                let r = v1.bitand(imm as u64);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Beq(rs, rt, imm) => {
                let v1 = vm.get_register_value(rs);
                let v2 = vm.get_register_value(rt);
                let pc = vm.get_pc();
                if v2 == v1 {
                    vm.set_pc(pc + imm as u8);
                } else {
                    vm.set_pc(pc + 1);
                }
            }
            Opcode::Div(rd, rs, rt) => {
                let v1 = vm.get_register_value(rs);
                let v2 = vm.get_register_value(rt);
                if v2 == 0 {
                    vm.of = 1;
                    vm.set_register_value(rd, 0);
                    vm.hi = 0;
                } else {
                    vm.set_register_value(rd, v1.div(v2));
                    vm.hi = v1.rem(v2);
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Divi(rd, rs, imm) => {
                let v1 = vm.get_register_value(rs);
                if imm == 0 {
                    vm.set_register_value(rd, 0);
                    vm.hi = 0;
                } else {
                    vm.set_register_value(rd, v1.div(imm as u64));
                    vm.hi = v1.rem(imm as u64);
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Mod(rd, rs, rt) => {
                let v1 = vm.get_register_value(rs);
                let v2 = vm.get_register_value(rt);
                if v2 == 0 {
                    vm.set_register_value(rd, 0);
                    vm.of = 2;
                } else {
                    vm.set_register_value(rd, v1.rem(v2));
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Modi(rd, rs, imm) => {
                let v1 = vm.get_register_value(rs);
                if imm == 0 {
                    vm.set_register_value(rd, 0);
                    vm.of = 2;
                } else {
                    vm.set_register_value(rd, v1.rem(imm as u64));
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Eq(rd, rs, rt) => {
                let v1 = vm.get_register_value(rs);
                let v2 = vm.get_register_value(rt);
                vm.set_register_value(rd, u64::from(v1.eq(&v2)));

                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Gt(rd, rs, rt) => {
                let v1 = vm.get_register_value(rs);
                let v2 = vm.get_register_value(rt);
                vm.set_register_value(rd, u64::from(v1.gt(&v2)));

                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::J(imm) => {
                let pc = vm.get_pc();
                vm.set_pc(pc + imm as u8);
            }
            Opcode::Jr(rs) => {
                let rs_val = vm.get_register_value(rs);
                vm.set_pc(rs_val as u8);
            }
            Opcode::Jnz(rs, rt) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                if rt_val != 0 {
                    vm.set_pc(rs_val as u8);
                } else {
                    let pc = vm.get_pc();
                    vm.set_pc(pc + 1);
                }
            }
            Opcode::Jnzi(rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                if imm != 0 {
                    vm.set_pc(rs_val as u8);
                } else {
                    let pc = vm.get_pc();
                    vm.set_pc(pc + 1);
                }
            }
            Opcode::Lw(rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                let lw_val = vm.memory[(rs_val + imm as u64) as usize];
                vm.set_register_value(rd, lw_val);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Mult(rd, rs, rt) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let (r, o) = rs_val.overflowing_mul(rt_val);
                vm.set_of_b(o);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Multi(rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                let (r, o) = rs_val.overflowing_mul(imm as u64);
                vm.set_of_b(o);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Noop() => {
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Or(rd, rs, rt) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let r = rs_val.bitor(rt_val);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Ori(rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                let r = rs_val.bitor(imm as MemWord);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Sw(rs, rt, imm) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                vm.memory[(rs_val + imm as u64) as usize] = rt_val;
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Sll(rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                let x = rs_val.shl(imm);
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Sllv(rd, rs, rt) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let x = rs_val.shl(rt_val);
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc as u8 + 1);
            }
            Opcode::Sltiu(rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                if rs_val < imm as u64 {
                    vm.set_register_value(rd, 1);
                } else {
                    vm.set_register_value(rd, 0);
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Sltu(rd, rs, rt) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                if rs_val < rt_val {
                    vm.set_register_value(rd, 1);
                } else {
                    vm.set_register_value(rd, 0);
                }
                let pc = vm.get_pc();
                vm.set_pc(pc as u8 + 1);
            }
            Opcode::Srl(rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                let x = rs_val.shr(imm);
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc as u8 + 1);
            }
            Opcode::Srlv(rd, rs, rt) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let x = rs_val.shr(rt_val);
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc as u8 + 1);
            }
            Opcode::Sra(rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                let sign: u64 = rs_val & 1 << 63;
                let mut x: u64 = rs_val;
                for _ in 0..imm {
                    x = rs_val.shr(1) | sign;
                }
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc as u8 + 1);
            }
            Opcode::Srav(rd, rs, rt) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let sign: u64 = rs_val & 1 << 63;
                let mut x: u64 = rs_val;
                for _ in 0..rt_val {
                    x = rs_val.shr(1) | sign;
                }
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc as u8 + 1);
            }
            Opcode::Sub(rd, rs, rt) => {
                let v1 = vm.get_register_value(rs);
                let v2 = vm.get_register_value(rt);
                let (r, b) = v1.overflowing_sub(v2);
                vm.set_register_value(rd, r);
                // set overflow
                vm.set_of_b(b);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Subi(rd, rs, imm) => {
                let v1 = vm.get_register_value(rs);
                let sign = imm & 1 << 15;
                let mut x: u64 = imm as u64;
                if sign > 0 {
                    x = (u64::MAX - (1 << 16 - 1)) | imm as u64;
                }
                let (r, b) = v1.overflowing_sub(x);
                vm.set_register_value(rd, r);
                // set overflow
                vm.set_of_b(b);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Xor(rd, rs, rt) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let r = rs_val.bitxor(rt_val);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Xori(rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                let r = rs_val.bitxor(imm as MemWord);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Addmod(rd, rs, rt, ru) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let ru_val = vm.get_register_value(ru);
                if ru_val == 0 {
                    vm.set_register_value(rd, 0);
                    vm.set_of(3);
                } else {
                    let (r, o) = rs_val.overflowing_add(rt_val);
                    let r1 = r % ru_val;
                    vm.set_register_value(rd, r1);
                    vm.set_of_b(o);
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Mulmod(rd, rs, rt, ru) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let ru_val = vm.get_register_value(ru);
                if ru_val == 0 {
                    vm.set_register_value(rd, 0);
                    vm.set_of(4);
                } else {
                    let (r, o) = rs_val.overflowing_mul(rt_val);
                    let r1 = r % ru_val;
                    vm.set_register_value(rd, r1);
                    vm.set_of_b(o);
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Exp(rd, rs, rt) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let r = rs_val.pow(rt_val as u32);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Expi(rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                let r = rs_val.pow(imm as u32);
                vm.set_register_value(rd, r);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }

            Opcode::Pop(rd) => {
                if let Some(v) = vm.pop_from_latest_framestack() {
                    vm.set_register_value(rd, v);
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }

            Opcode::Push(rs) => {
                let v1 = vm.get_register_value(rs);
                vm.push_to_latest_framestack(v1);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }

            Opcode::Call(address, n) => {
                vm.create_and_push_new_frame(address, n);
            }

            Opcode::Ret(imm) => {
                let mut p: Program = Program::new();
                let mut c2: bool = false;
                let mut code_mapping = Default::default();
                if let Some(top_frame) = vm.get_top_frame() {
                    c2 = top_frame.create2_frame;
                    if c2 {
                        for i in 0..top_frame.stack.len() {
                            p.code.push(top_frame.stack[i] as u32);
                        }
                        code_mapping = top_frame.address_program_mapping.clone();
                    }
                }
                vm.release_frame();
                if c2 {
                    if let Some(prev_frame) = vm.get_framestack().last_mut() {
                        prev_frame.address_program_mapping.insert(p.code.pop().unwrap() as u64, p);
                        prev_frame.address_program_mapping.extend(code_mapping);
                    }
                }
                // let pc = vm.get_pc();
                vm.set_pc(imm as u8);
            }

            Opcode::Halt(rs) => {
                vm.set_error(rs as u8);
                vm.state = 0;
            }

            Opcode::Stop() => {
                vm.state = 0;
            }

            Opcode::Malloc(_rs, imm) => {
                if let Some(frame) = vm.get_framestack().last_mut() {
                    frame.stack.reserve(imm as usize);
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Free(rs) => {
                if let Some(frame) = vm.get_framestack().last_mut() {
                    let cap = frame.stack.capacity();
                    frame.stack.resize(cap - rs as usize, 2);
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }

            Opcode::CopyRegister(rd, rs) => {
                let rs = vm.get_register_value(rs);
                vm.set_register_value(rd, rs);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }

            Opcode::LoadLocal(rd, imm) => {
                if let Some(top_frame) = &mut vm.get_top_frame() {
                    if let Some(local_val) = top_frame.stack.get(imm as usize) {
                        vm.set_register_value(rd, *local_val);
                    }
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::WriteLocal(rd, imm) => {
                if let Some(top_frame) = vm.get_framestack().last_mut() {
                    if top_frame.stack.len() > imm as usize {
                        top_frame.stack[rd as usize] = imm as u64;
                    }
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::SetI(rd, imm) => {
                vm.set_register_value(rd, imm as u64);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Rotr(rd, rs, rt) => {
                let rs = vm.get_register_value(rs);
                let rt = vm.get_register_value(rt);
                let x = rs.rotate_right(rt as u32);
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Rotri(rd, rs, imm) => {
                let rs = vm.get_register_value(rs);
                let x = rs.rotate_right(imm as u32);
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Rotl(rd, rs, rt) => {
                let rs = vm.get_register_value(rs);
                let rt = vm.get_register_value(rt);
                let x = rs.rotate_left(rt as u32);
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Rotli(rd, rs, imm) => {
                let rs = vm.get_register_value(rs);
                let x = rs.rotate_left(imm as u32);
                vm.set_register_value(rd, x);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Gas(rd) => {
                vm.set_register_value(rd, 0);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Gaslimit(rd) => {
                vm.set_register_value(rd, vm.fuel_tx.gas_limit);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Gasprice(rd) => {
                vm.set_register_value(rd, vm.fuel_tx.gas_price);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Balance(rd, _rs) => {
                vm.set_register_value(rd, 0);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Blockhash(rd, _rs) => {
                vm.set_register_value(rd, 0);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Codesize(rd, _rs) => {
                let mut s = 0;
                for i in 0..vm.callframes.len() {
                    s += vm.callframes.get(i).unwrap().program.code.len();
                }
                vm.set_register_value(rd, s as u64);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Codecopy(_rd, _rs, _imm) => {
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::EthCall(_rd, rs) => {
                // get params from memory
                // gas, to, value, in offset, in size, out offset, out size

                let gas = vm.get_memory(rs);
                let to = vm.get_memory(rs + 1);
                let value = vm.get_memory(rs + 2);
                let in_offset = vm.get_memory(rs + 3);
                let in_size = vm.get_memory(rs + 4);
                let out_offset = vm.get_memory(rs + 5);
                let out_size = vm.get_memory(rs + 6);

                let mut p: Program = Program::new();
                if let Some(to_contract_code) = vm.get_top_frame().unwrap().address_program_mapping.get(&to) {
                    p = to_contract_code.clone();
                }
                vm.create_and_push_new_frame(0, in_size as u8);
                if let Some(new_frame) = vm.get_framestack().last_mut() {
                    new_frame.program = p;
                }
            }
            Opcode::Callcode(_rd, _rs) => {
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Delegatecall(_rd, _rs) => {
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Staticcall(_rd, _rs) => {
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Ethreturn(_rd, _rs) => {
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Create2(_rd, rs, imm) => {
                let rs_val = vm.get_register_value(rs);
                if imm == 0 || (imm as u64) < rs_val {
                    panic!("Create2: Invalid stack offset and length provided");
                }
                let mut p: Program = Program::new();
                if let Some(top_frame) = vm.get_framestack().last_mut() {
                    let len: usize = top_frame.stack.len();
                    if len < imm as usize {
                        panic!("Create2: insufficient stack length");
                    }
                    for i in rs_val as u16..imm {
                        p.code.push(*top_frame.stack.get(len - 1 - i as usize).unwrap() as u32);
                    }
                }
                vm.create_and_push_new_frame(0, (rs_val - 1) as u8);
                if let Some(new_frame) = vm.get_framestack().last_mut() {
                    new_frame.program = p;
                    new_frame.create2_frame = true;
                }
            }
            Opcode::Revert(_rd, _rs) => {
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Keccak(rd, rs, rt) => {
                let rd_val = vm.get_register_value(rd);
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let keccak_input: Vec<u8> = transform_from_u64_to_u8(&vm.memory[rs_val as usize..rt_val as usize].to_vec());
                let h256_result = keccak(keccak_input);
                let to_place_in_mem = transform_from_u8_to_u64(&h256_result.to_fixed_bytes().to_vec());
                for to_loc_index in 0..to_place_in_mem.len() {
                    vm.set_memory(to_loc_index as u8 + rd_val as u8, *to_place_in_mem.get(to_loc_index).unwrap());
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Sha256(rd, rs, rt) => {
                let rd_val = vm.get_register_value(rd);
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let input: Vec<u8> = transform_from_u64_to_u8(&vm.memory[rs_val as usize..rt_val as usize].to_vec());
                let mut sha3 = Sha3::v256();
                sha3.update(input.as_slice());
                let mut output = [0; 32];
                sha3.finalize(&mut output);
                let to_place_in_mem = transform_from_u8_to_u64(&output.to_vec());
                for to_loc_index in 0..to_place_in_mem.len() {
                    vm.set_memory(to_loc_index as u8 + rd_val as u8, *to_place_in_mem.get(to_loc_index).unwrap());
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::FuelBlockHeight(rd) => {
                vm.set_register_value(rd, 0);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::FuelRootProducer(rd) => {
                vm.set_register_value(rd, 0);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::FuelBlockProducer(rd) => {
                vm.set_register_value(rd, 0);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Utxoid(rd, imm) => {
                let rd_val = vm.get_register_value(rd);
                let x: [u8; 32] = vm.fuel_tx.inputs[imm as usize].utxo_id;
                vm.set_memory(rd_val as u8, transform_from_u8_to_u64(&x.to_vec())[0]);

                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Contractid(rd) => {
                if let Some(top_frame) = vm.get_top_frame() {
                    vm.set_register_value(rd, calculate_hash(&top_frame.program));
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::FtxOutputTo(rd, imm) => {
                match vm.fuel_tx.outputs[imm as usize].output_type {
                    FOutputTypeEnum::FOutputCoin { to, amount } => {
                        vm.set_register_value(rd, transform_from_u8_to_u64(&to.to_vec())[0]);
                    }
                    FOutputTypeEnum::FOutputContract { input_index, amount_witness_index, state_witness_index } => {}
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::FtxOutputAmount(rd, imm) => {
                match vm.fuel_tx.outputs[imm as usize].output_type {
                    FOutputTypeEnum::FOutputCoin { to, amount } => {
                        vm.set_register_value(rd, amount);
                    }
                    FOutputTypeEnum::FOutputContract { input_index, amount_witness_index, state_witness_index } => {}
                }

                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Srw(rd, rs) => {
                let rs_val = vm.get_register_value(rs);

                let i1 = vm.store.get_word(rs_val);
                vm.set_register_value(rd, i1 as u64);

                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Srwx(rd, rs) => {
                let rs_val = vm.get_register_value(rs);

                let i1 = vm.store.get_word_x(rs_val);
                vm.set_register_value(rd, i1 as u64);

                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Sww(rd, rs) => {
                let rd_val = vm.get_register_value(rd);
                let rs_val = vm.get_register_value(rs);

                vm.store.set_word(rd_val, rs_val);

                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::Swwx(rd, rs) => {
                let rd_val = vm.get_register_value(rd);
                let rs_val = vm.get_register_value(rs);

                vm.store.set_word_x(rd_val, rs_val);

                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::MemEq(rd, rs, rt, ru) => {
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);
                let ru_val = vm.get_register_value(ru);

                let mut cmp: bool = true;
                for x in 0 .. ru_val {
                    let i_0 = vm.get_memory(rs_val as u8 + x as u8);
                    let i_1 = vm.get_memory(rt_val as u8 + x as u8);
                    if i_0 != i_1 {
                        cmp = false;
                        break;
                    }
                }
                vm.set_register_value(rd, cmp as u64);
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }
            Opcode::MemCp(rd, rs, rt) => {
                let rd_val = vm.get_register_value(rd);
                let rs_val = vm.get_register_value(rs);
                let rt_val = vm.get_register_value(rt);

                for x in 0 .. rt_val {
                    vm.set_memory(rd_val as u8 + x as u8, vm.get_memory(rs_val as u8 + x as u8))
                }
                let pc = vm.get_pc();
                vm.set_pc(pc + 1);
            }

            _ => {
                panic!("####Encountered unimplemented op");
            }
        }
    }
}

#[derive(Clone)]
pub struct CallFrame {
    pub create2_frame: bool,
    pub pc: u8,
    pub sp: u8,
    pub fp: u8,
    pub prev_registers: [MemWord; VM_REGISTER_COUNT as usize],
    pub stack: Vec<MemWord>,
    pub args: Vec<MemWord>,
    pub program: Program,
    pub address_program_mapping: HashMap<u64, Program>,
}

impl CallFrame {
    pub fn new() -> CallFrame {
        CallFrame {
            create2_frame: false,
            pc: 0,
            sp: 0,
            fp: 0,
            prev_registers: [0; VM_REGISTER_COUNT as usize],
            stack: Vec::new(),
            args: Vec::new(),
            program: Program::new(),
            address_program_mapping: HashMap::new(),
        }
    }

    pub fn copy_registers(&mut self, a: [MemWord; VM_REGISTER_COUNT as usize]) {
        self.prev_registers = a;
    }

    pub fn save_state(&mut self, pc: u8) {
        // keep return value here
        self.stack.push(pc as u64);

        // track current fp
        self.stack.push(self.fp as u64);

        // initialize new frame at current stack pointer
        self.fp = self.sp;
        // alternative approach
        // self.stack.push(self.sp);

        // for non-heap approach, need to allocate space
        // malloc numbytes for all local storage needs
        // since we use Vec, current implementation is heap-based
    }

    pub fn ret_state(&mut self) {
        // return stack pointer to current frame pointer
        self.sp = self.fp;

        // alternative approach:
        // self.sp = self.stack.pop().unwrap();

        // return to previous frame pointer
        self.fp = self.stack.pop().unwrap() as u8;
    }

    pub fn pop_from_framestack(&mut self) {
        self.stack.pop();
    }
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

trait FuelStore {
    fn get_word(&self, k: u64) -> u8;
    fn get_word_x(&self, k: u64) -> u64;
    fn set_word(&mut self, k: u64, v: u64);
    fn set_word_x(&mut self, k: u64, v: u64);
}

impl FuelStore for HashMap<u64, u64> {
    fn get_word(&self, k: u64) -> u8 {
        match self.get(&k) {
            None => {
                0
            }
            v => {
                let x: &u64 = v.unwrap();
                transform_from_u64_to_u8(&[*x].to_vec())[0]
            }
        }
    }

    fn get_word_x(&self, k: u64) -> u64 {
        match self.get(&k) {
            None => {
                0
            }
            v => {
                *v.unwrap()
            }
        }
    }

    fn set_word(&mut self, k: u64, v: u64) {
        let i = transform_from_u64_to_u8(&[v].to_vec())[0];
        self.insert(k, i as u64);
    }

    fn set_word_x(&mut self, k: u64, v: u64) {
        self.insert(k, v);
    }
}

// Fuel Unsigned Tx Format
// https://github.com/FuelLabs/docs/blob/master/src/v1.0.0/3.%20Concepts/1.%20Data%20Structures/Transactions.md

pub type Uint256 = [FuelTxEthUnit; FUEL_TX_ETH_UNIT_WIDTH as usize];

pub type FuelTxEthUnit = u8;
pub type FuelTxCoreUnit = u32;
pub type FuelTxInputLength = u16;
pub type FuelTxInputType = u8;
pub type FuelTxOutputLength = u16;
pub type FuelTxOutputType = u8;
pub type FuelTxDataLength = u8;
pub type FuelTxDataUnit = u32;

pub const FUEL_TX_ETH_UNIT_WIDTH: FuelTxEthUnit = 32;
pub const FUEL_TX_ETH_ADDRESS_WIDTH: FuelTxEthUnit = 20;
pub const FUEL_TX_INPUT_LENGTH: FuelTxInputLength = 8;
pub const FUEL_TX_OUTPUT_LENGTH: FuelTxOutputLength = 8;
pub const FUEL_TX_DATA_LENGTH: FuelTxDataLength = 8;

pub type FuelAddress = [FuelTxEthUnit; FUEL_TX_ETH_ADDRESS_WIDTH as usize];

#[derive(Copy, Clone, Debug, Default)]
pub struct UnsignedTransactionFormat {
    pub address: FuelAddress,
    pub inputs_length: FuelTxInputLength,
    pub inputs: [FuelTxInputType; FUEL_TX_INPUT_LENGTH as usize],
    pub outputs_length: FuelTxOutputLength,
    pub output: [FuelTxOutputType; FUEL_TX_OUTPUT_LENGTH as usize],
    pub data_length: FuelTxDataLength,
    pub data: [FuelTxDataUnit; FUEL_TX_DATA_LENGTH as usize],
    pub signature_fee_token: [FuelTxEthUnit; FUEL_TX_ETH_UNIT_WIDTH as usize],
    pub signature_fee: [FuelTxEthUnit; FUEL_TX_ETH_UNIT_WIDTH as usize],
}

impl UnsignedTransactionFormat {
    pub fn new() -> UnsignedTransactionFormat {
        UnsignedTransactionFormat {
            address: [0; FUEL_TX_ETH_ADDRESS_WIDTH as usize],
            inputs_length: FUEL_TX_INPUT_LENGTH,
            inputs: [0; FUEL_TX_INPUT_LENGTH as usize],
            outputs_length: FUEL_TX_OUTPUT_LENGTH,
            output: [0; FUEL_TX_OUTPUT_LENGTH as usize],
            data_length: FUEL_TX_DATA_LENGTH,
            data: [0; FUEL_TX_DATA_LENGTH as usize],
            signature_fee_token: [0; FUEL_TX_ETH_UNIT_WIDTH as usize],
            signature_fee: [0; FUEL_TX_ETH_UNIT_WIDTH as usize],
        }
    }
}

pub type FuelTxRootsLength = u8;

pub const FUEL_TX_ARRAY_LENGTH: FuelTxRootsLength = 8;

#[derive(Copy, Clone, Debug, Default)]
pub struct TransactionProof {
    pub block_producer: FuelAddress,
    pub previous_block_hash: Uint256,
    pub block_height: Uint256,
    pub block_number: Uint256,
    pub num_tokens: Uint256,
    pub num_addresses: Uint256,
    pub roots_length: u16,
    pub roots: [Uint256; FUEL_TX_ARRAY_LENGTH as usize],
    pub root_producer: FuelAddress,
    pub merkle_tree_root: Uint256,
    pub commitment_hash: Uint256,
    pub root_length: Uint256,
    pub fee_token: Uint256,
    pub fee: Uint256,
    pub root_index: u16,
    pub merkle_proof_length: u16,
    pub merkle_proof: [Uint256; FUEL_TX_ARRAY_LENGTH as usize],
    pub input_output_index: u8,
    pub transaction_index: u16,
    pub transaction_length: u16,
    pub transaction: [u8; FUEL_TX_ARRAY_LENGTH as usize],
    pub data_length: u8,
    pub data: [Uint256; FUEL_TX_ARRAY_LENGTH as usize],
    pub signature_fee_token: Uint256,
    pub signature_fee: Uint256,
    pub token_address: FuelAddress,
    pub selector: FuelAddress,
}

impl TransactionProof {
    pub fn new_default() -> TransactionProof {
        Default::default()
    }
}

pub type MetaDataUnit = u64;

#[derive(Copy, Clone, Debug, Default)]
pub struct Transactionleaf {
    pub length: u16,
    pub metadata_length: u8,
    pub metadata: [MetaDataUnit; FUEL_TX_ARRAY_LENGTH as usize],
    pub witnesses_length: u16,
    pub witnesses: [u8; FUEL_TX_ARRAY_LENGTH as usize],
    pub inputs_length: u16,
    pub inputs: [u8; FUEL_TX_ARRAY_LENGTH as usize],
    pub outputs_length: u16,
    pub outputs: [u8; FUEL_TX_ARRAY_LENGTH as usize],
}

impl Transactionleaf {
    pub fn new_default() -> Transactionleaf {
        Default::default()
    }

    pub fn new() -> Transactionleaf {
        Transactionleaf {
            length: 0,
            metadata_length: 0,
            metadata: [0; FUEL_TX_ARRAY_LENGTH as usize],
            witnesses_length: 0,
            witnesses: [0; FUEL_TX_ARRAY_LENGTH as usize],
            inputs_length: 0,
            inputs: [0; FUEL_TX_ARRAY_LENGTH as usize],
            outputs_length: 0,
            outputs: [0; FUEL_TX_ARRAY_LENGTH as usize],
        }
    }
}

pub fn handle_txleaf(tx: Transactionleaf, vm: &mut VM) {
    // metadata contains function selector, and function parameters
    let meta = tx.metadata;
    if meta.len() == 0 {
        panic!("Incorrect tx format");
    }

    // params
    for i in 1..meta.len() {
        vm.push_to_latest_framestack(meta[i]);
    }

    let function_selector = meta[0];
    vm.push_to_latest_framestack(function_selector);

    vm.run();
}

pub fn handle_fueltx_init(tx: FuelTx, vm: &mut VM) {
    if tx.eth_tx.init.code.len() == 0 {
        panic!("No init code found");
    }
    if tx.eth_tx.to[0] != 0 {
        panic!("Invalid to for init code");
    }

    if let Some(top_frame) = vm.get_framestack().last_mut() {
        top_frame.program = tx.eth_tx.init;
    }

    vm.run();
}

pub fn handle_ftx(tx: Ftx, vm: &mut VM) {
    if tx.script_length > 0 {
        vm.program.code = transform_from_u8_to_u32(&tx.script);
    }

    for v in tx.inputs {
        match v.input_type {
            FInputTypeEnum::Coin(_) => {
                // noop
            }
            FInputTypeEnum::Contract(_) => {
                let d_vec = transform_from_u8_to_u32(&v.data);
                for d in 0..d_vec.len() {
                    vm.program.code.insert(d, d_vec[d]);
                }
            }
        }
    }

    vm.run();
}

pub fn handle_fueltx_data(tx: FuelTx, vm: &mut VM) {
    // metadata contains function selector, and function parameters
    let data = tx.eth_tx.data;
    if data.code.len() == 0 {
        panic!("Incorrect tx format");
    }

    for i in 1..data.code.len() {
        vm.push_to_latest_framestack(*data.code.get(i).unwrap() as u64);
    }

    let function_selector = data.code[1];
    vm.push_to_latest_framestack(function_selector as u64);

    vm.run();
}

#[derive(Copy, Clone, Debug, Default)]
pub struct EthAccount {
    pub nonce: u64,
    pub balance: u64,
    pub storage_root: Uint256,
    pub code_hash: u64,
}

impl EthAccount {
    pub fn create_default() -> EthAccount {
        Default::default()
    }
}

#[derive(Clone, Debug, Default)]
pub struct EthereumTx {
    pub nonce: u64,
    pub gas_price: u64,
    pub gas_limit: u64,
    pub to: FuelAddress,
    pub value: u64,
    v: u64,
    r: u64,
    s: u64,
    init: Program,
    data: Program,
}

impl EthereumTx {
    pub fn create_default() -> EthereumTx {
        Default::default()
    }
}

#[derive(Clone, Debug, Default)]
pub struct FuelTx {
    pub utxo_tx_id: u64,
    pub eth_tx: EthereumTx,
    pub root_producer: u64,
    pub block_producer: u64,
    pub block_height: u64,
}

impl FuelTx {
    pub fn create_default() -> FuelTx {
        Default::default()
    }
}


#[derive(Clone, Default, Hash, Debug)]
pub struct Program {
    pub code: Vec<OpcodeInstruction>
}

impl Program {
    pub fn new() -> Program {
        Program {
            code: Vec::new()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Ftx {
    pub script_length: u16,
    pub script: Vec<u8>,
    pub gas_price: u64,
    pub gas_limit: u64,
    pub inputs_count: u8,
    pub inputs: Vec<FInput>,
    pub outputs_count: u8,
    pub outputs: Vec<FOutput>,
}

#[derive(Clone, Debug)]
pub struct FInput {
    pub utxo_id: FUtxoId,
    pub input_type: FInputTypeEnum,
    pub data_length: u16,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct FInputCoin {
    pub witness_index: u8,
}

#[derive(Clone, Debug, Default)]
pub struct FInputContract {
    pub contract_id: FContractId,
}

#[derive(Clone, Debug)]
pub enum FInputTypeEnum {
    Coin(FInputCoin),
    Contract(FInputContract),
}

#[derive(Clone, Debug)]
pub struct FOutput {
    pub output_type: FOutputTypeEnum,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct FWitness {
    pub data_length: u16,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum FOutputTypeEnum {
    FOutputCoin {
        to: [u8; 32],
        amount: u64,
    },
    FOutputContract {
        input_index: u8,
        amount_witness_index: u8,
        state_witness_index: u8,
    },
}

pub type FInputType = u8;
pub type FOutputType = u8;
pub type FUtxoId = [u8; 32];
pub type FContractId = [u8; 32];
