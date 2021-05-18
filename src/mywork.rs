mod bit_funcs;
mod consts;
mod interpreter;
mod opcodes;

use crate::consts::FUEL_MAX_MEMORY_SIZE;
use interpreter::Program;
use interpreter::VM;
use std::thread;

fn run() {
    let _p = Program::new();

    let mut vm = VM::new();
}

fn main() {
    println!("Hello, world!");

    // Spawn thread with explicit stack size
    let child = thread::Builder::new()
        .stack_size(FUEL_MAX_MEMORY_SIZE as usize * 1024)
        .spawn(run)
        .unwrap();

    // Wait for thread to join
    child.join().unwrap();
}
