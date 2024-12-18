use alloc::vec::Vec;
use fuel_core_types::{
    fuel_vm::{
        consts::VM_REGISTER_COUNT,
        interpreter::{
            trace::ExecutionTraceHooks,
            Memory,
            MemoryInstance,
        },
    },
    services::executor::TraceFrame,
};

/// Used to capture an execution trace for every instruction.
#[derive(Debug, Clone, Default)]
pub struct TraceOnInstruction {
    /// Append-only set of frames
    pub frames: Vec<TraceFrame>,
    /// Memory at the time of the previous snapshot
    acc_memory: AccVec,
}

impl ExecutionTraceHooks for TraceOnInstruction {
    fn after_instruction<M, S, Tx, Ecal>(
        vm: &mut fuel_core_types::fuel_vm::Interpreter<M, S, Tx, Ecal, Self>,
    ) where
        M: Memory,
    {
        let memory_diff = vm.trace().acc_memory.diff(vm.memory().as_ref());
        for patch in memory_diff.parts.iter() {
            vm.trace_mut().acc_memory.update(patch.clone());
        }

        let mut registers = [0; VM_REGISTER_COUNT];
        registers.copy_from_slice(vm.registers());

        let receipt_count = vm.receipts().len();

        vm.trace_mut().frames.push(TraceFrame {
            memory_diff: memory_diff
                .parts
                .into_iter()
                .map(|p| (p.start, p.data))
                .collect(),
            registers,
            receipt_count,
        })
    }
}

/// Used to capture an execution trace for after each receipt.
#[derive(Debug, Clone, Default)]
pub struct TraceOnReceipt {
    /// Append-only set of frames
    pub frames: Vec<TraceFrame>,
    /// Memory at the time of the previous snapshot
    acc_memory: AccVec,
}

impl ExecutionTraceHooks for TraceOnReceipt {
    fn after_instruction<M, S, Tx, Ecal>(
        vm: &mut fuel_core_types::fuel_vm::Interpreter<M, S, Tx, Ecal, Self>,
    ) where
        M: Memory,
    {
        if vm
            .trace()
            .frames
            .last()
            .map(|s| s.receipt_count)
            .unwrap_or(0)
            < vm.receipts().len()
        {
            let memory_diff = vm.trace().acc_memory.diff(vm.memory().as_ref());
            for patch in memory_diff.parts.iter() {
                vm.trace_mut().acc_memory.update(patch.clone());
            }

            let mut registers = [0; VM_REGISTER_COUNT];
            registers.copy_from_slice(vm.registers());

            let receipt_count = vm.receipts().len();

            vm.trace_mut().frames.push(TraceFrame {
                memory_diff: memory_diff
                    .parts
                    .into_iter()
                    .map(|p| (p.start, p.data))
                    .collect(),
                registers,
                receipt_count,
            })
        }
    }
}

#[derive(Debug, Clone, Default)]
struct AccVec {
    parts: Vec<AccVecPart>,
}

#[derive(Debug, Clone)]
struct AccVecPart {
    start: usize,
    data: Vec<u8>,
}
impl AccVecPart {
    #[allow(clippy::arithmetic_side_effects)] // VM memory is always within bounds
    fn end(&self) -> usize {
        self.start + self.data.len()
    }
}

impl AccVec {
    #[allow(clippy::arithmetic_side_effects)] // All operations stay within array bounds
    pub fn update(&mut self, mut new: AccVecPart) {
        let new_end = new.end();
        let start = self.parts.binary_search_by_key(&new.start, |p| p.start);

        // Figure out the number of overlapping with the new part
        let mut end = match start {
            Ok(start) => start,
            Err(start) => start,
        };
        while end < self.parts.len() && self.parts[end].start < new.end() {
            end += 1;
        }

        // Actually insert the new part
        match start {
            Ok(start) => {
                // Found a part that starts at exact same address. Figure out how many pieces we need to merge.
                if start == end {
                    // We only have one item.
                    if self.parts[start].data.len() <= new.data.len() {
                        // Replace a prefix
                        self.parts[start].data[..new.data.len()]
                            .copy_from_slice(&new.data);
                    } else {
                        // Replace and extend
                        self.parts[start] = new;
                    }
                    return;
                }

                // Multiple items to merge
                self.parts[start].data = new.data;

                // Remove the now-unnecessary parts, but keep the last one in case we need to keep some of it.
                if let Some(last) = self.parts.drain(start + 1..end).last() {
                    // How many bytes of the last item we need to keep?
                    let keep_last = last.end() - new_end;
                    self.parts[start]
                        .data
                        .extend_from_slice(&last.data[keep_last..]);
                }
            }
            Err(start) => {
                // No exact match for start address found.

                if start == self.parts.len() {
                    // This is the last item, so we can just append it.
                    self.parts.push(new);
                    return;
                }

                // Remove the now-unnecessary parts, but keep the last one in case we need to keep some of it.
                if let Some(last) = self.parts.drain(start..end).last() {
                    let keep_last = last.end() - new_end;
                    new.data.extend_from_slice(&last.data[keep_last..]);
                }

                self.parts.insert(start, new);
            }
        }
    }

    pub fn diff(&self, _new: &MemoryInstance) -> Self {
        todo!();
    }
}
