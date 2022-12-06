mod set;

use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    measurement::WallTime,
    BenchmarkGroup,
    Criterion,
};

use fuel_core_benches::*;
use set::*;

pub fn run_group_ref<I>(group: &mut BenchmarkGroup<WallTime>, id: I, bench: VmBench)
where
    I: AsRef<str>,
{
    let mut i = bench.prepare().expect("failed to prepare bench");
    group.bench_function::<_, _>(id.as_ref(), move |b| {
        b.iter_custom(|iters| {
            let VmBenchPrepared {
                vm,
                instruction,
                cleanup_script,
            } = &mut i;
            let code = OpcodeRepr::from_u8(instruction.op());
            let db_txn = {
                let db = vm.as_mut().database_mut();
                let db_txn = db.transaction();
                *db = db_txn.as_ref().clone();
                db_txn
            };
            let ret = Instruction::from(Opcode::RET(REG_ONE));
            let start = std::time::Instant::now();
            for _ in 0..iters {
                match code {
                    OpcodeRepr::CALL => {
                        let (_, ra, rb, rc, rd, _imm) = instruction.into_inner();
                        black_box(vm.prepare_call(ra, rb, rc, rd).unwrap());
                        vm.instruction(ret).unwrap();
                    }
                    _ => {
                        black_box(vm.instruction(*instruction).unwrap());
                        if !cleanup_script.is_empty() {
                            for i in cleanup_script.iter_mut() {
                                vm.instruction(*i).unwrap();
                            }
                        }
                    }
                }
                if matches!(
                    code,
                    OpcodeRepr::CALL
                        | OpcodeRepr::RET
                        | OpcodeRepr::LDC
                        | OpcodeRepr::RVRT
                        | OpcodeRepr::LOG
                        | OpcodeRepr::LOGD
                ) {
                    vm.clear_receipts();
                }
            }
            db_txn.commit().unwrap();
            start.elapsed()
        })
    });
}
fn vm(c: &mut Criterion) {
    alu::run(c);
    blockchain::run(c);
    crypto::run(c);
    flow::run(c);
    mem::run(c);
}

criterion_group!(benches, vm);
criterion_main!(benches);
