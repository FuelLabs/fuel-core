use fuel_core_executor::ports::RelayerPort;
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    services::relayer::Event,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::Arc,
};

#[derive(Debug, Clone, Default)]
pub struct Relayer(RefCell<BTreeMap<DaBlockHeight, Vec<Event>>>);

impl Relayer {
    pub fn add_event(&self, da_block_height: DaBlockHeight, events: Vec<Event>) {
        self.0.borrow_mut().insert(da_block_height, events);
    }
}

#[derive(Debug, Clone)]
pub struct RelayerRecorder<S> {
    storage: S,
    pub record: Arc<RefCell<Relayer>>,
}

impl<S> RelayerRecorder<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            record: Default::default(),
        }
    }
}

impl<S> RelayerPort for RelayerRecorder<S>
where
    S: RelayerPort,
{
    fn enabled(&self) -> bool {
        self.storage.enabled()
    }

    fn get_events(&self, da_height: &DaBlockHeight) -> anyhow::Result<Vec<Event>> {
        println!("events for {:?}", da_height); // TODO: remove this
        let events = self.storage.get_events(da_height)?;
        self.record.borrow().add_event(*da_height, events.clone());
        Ok(events)
    }
}
