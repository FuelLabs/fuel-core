mod interpreter;
mod opcodes;
mod consts;
mod bit_funcs;

use interpreter::Program;
use interpreter::VM;
use std::thread;
use crate::consts::{FUEL_MAX_MEMORY_SIZE};

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

