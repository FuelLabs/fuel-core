use graph::{
    blockchain::{MappingTriggerTrait, TriggerData},
    runtime::{asc_new, gas::GasCounter, AscPtr, HostExportError},
};
use graph_runtime_wasm::module::ToAscPtr;
use starknet_ff::FieldElement;
use std::{cmp::Ordering, sync::Arc};

use crate::codec;

#[derive(Debug, Clone)]
pub enum StarknetTrigger {
    Block(StarknetBlockTrigger),
    Event(StarknetEventTrigger),
}

#[derive(Debug, Clone)]
pub struct StarknetBlockTrigger {
    pub(crate) block: Arc<codec::Block>,
}

#[derive(Debug, Clone)]
pub struct StarknetEventTrigger {
    pub(crate) event: Arc<codec::Event>,
    pub(crate) block: Arc<codec::Block>,
    pub(crate) transaction: Arc<codec::Transaction>,
}

impl PartialEq for StarknetTrigger {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Block(l), Self::Block(r)) => l.block == r.block,
            (Self::Event(l), Self::Event(r)) => {
                // Without event index we can't really tell if they're the same
                // TODO: implement add event index to trigger data
                l.block.hash == r.block.hash
                    && l.transaction.hash == r.transaction.hash
                    && l.event == r.event
            }
            _ => false,
        }
    }
}

impl Eq for StarknetTrigger {}

impl PartialOrd for StarknetTrigger {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StarknetTrigger {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Block(l), Self::Block(r)) => l.block.height.cmp(&r.block.height),

            // Block triggers always come last
            (Self::Block(..), _) => Ordering::Greater,
            (_, Self::Block(..)) => Ordering::Less,

            // Keep the order when comparing two event triggers
            // TODO: compare block hash, tx index, and event index
            (Self::Event(..), Self::Event(..)) => Ordering::Equal,
        }
    }
}

impl TriggerData for StarknetTrigger {
    fn error_context(&self) -> String {
        match self {
            Self::Block(block) => format!("block #{}", block.block.height),
            Self::Event(event) => {
                format!(
                    "event from {}",
                    match FieldElement::from_byte_slice_be(&event.event.from_addr) {
                        Ok(from_addr) => format!("{from_addr:#x}"),
                        Err(_) => "[unable to parse source address]".into(),
                    }
                )
            }
        }
    }

    fn address_match(&self) -> Option<&[u8]> {
        None
    }
}

impl ToAscPtr for StarknetTrigger {
    fn to_asc_ptr<H: graph::runtime::AscHeap>(
        self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscPtr<()>, HostExportError> {
        Ok(match self {
            StarknetTrigger::Block(block) => asc_new(heap, &block, gas)?.erase(),
            StarknetTrigger::Event(event) => asc_new(heap, &event, gas)?.erase(),
        })
    }
}

impl MappingTriggerTrait for StarknetTrigger {
    fn error_context(&self) -> String {
        match self {
            Self::Block(block) => format!("block #{}", block.block.height),
            Self::Event(event) => {
                format!(
                    "event from {}",
                    match FieldElement::from_byte_slice_be(&event.event.from_addr) {
                        Ok(from_addr) => format!("{from_addr:#x}"),
                        Err(_) => "[unable to parse source address]".into(),
                    }
                )
            }
        }
    }
}
