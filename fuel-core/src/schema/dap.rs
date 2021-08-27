use crate::database::{DatabaseTransaction, SharedDatabase};
use async_graphql::{Context, Object, SchemaBuilder, ID};
use fuel_vm::consts;
use fuel_vm::prelude::*;
use futures::lock::Mutex;
use std::collections::HashMap;
use std::{io, sync};
use tracing::{debug, trace};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct ConcreteStorage {
    vm: HashMap<ID, Interpreter<DatabaseTransaction>>,
    tx: HashMap<ID, Vec<Transaction>>,
    db: HashMap<ID, DatabaseTransaction>,
}

impl ConcreteStorage {
    pub fn register(&self, id: &ID, register: RegisterId) -> Option<Word> {
        self.vm
            .get(id)
            .and_then(|vm| vm.registers().get(register).copied())
    }

    pub fn memory(&self, id: &ID, start: usize, size: usize) -> Option<&[u8]> {
        let (end, overflow) = start.overflowing_add(size);
        if overflow || end > consts::VM_MAX_RAM as usize {
            return None;
        }

        self.vm.get(id).map(|vm| &vm.memory()[start..end])
    }

    pub fn init(
        &mut self,
        txs: &[Transaction],
        storage: DatabaseTransaction,
    ) -> Result<ID, ExecuteError> {
        let id = Uuid::new_v4();
        let id = ID::from(id);

        let tx = txs.first().cloned().unwrap_or_default();
        self.tx
            .get_mut(&id)
            .map(|tx| tx.extend_from_slice(txs))
            .unwrap_or_else(|| {
                self.tx.insert(id.clone(), txs.to_owned());
            });

        let mut vm = Interpreter::with_storage(storage.clone());
        vm.transact(tx)?;
        self.vm.insert(id.clone(), vm);
        self.db.insert(id.clone(), storage);

        Ok(id)
    }

    pub fn kill(&mut self, id: &ID) -> bool {
        self.tx.remove(id);
        self.vm.remove(id);
        self.db.remove(id).is_some()
    }

    pub fn reset(&mut self, id: &ID, storage: DatabaseTransaction) -> Result<(), ExecuteError> {
        let tx = self
            .tx
            .get(id)
            .and_then(|tx| tx.first())
            .cloned()
            .unwrap_or_default();

        let mut vm = Interpreter::with_storage(storage.clone());
        vm.transact(tx)?;
        self.vm.insert(id.clone(), vm).ok_or_else(|| {
            ExecuteError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "The VM instance was not found",
            ))
        })?;
        self.db.insert(id.clone(), storage);
        Ok(())
    }

    pub fn exec(&mut self, id: &ID, op: Opcode) -> Result<(), ExecuteError> {
        self.vm
            .get_mut(id)
            .map(|vm| vm.execute(op))
            .transpose()?
            .map(|_| ())
            .ok_or_else(|| {
                ExecuteError::Io(io::Error::new(
                    io::ErrorKind::NotFound,
                    "The VM isntance was not found",
                ))
            })
    }
}

pub type GraphStorage = sync::Arc<Mutex<ConcreteStorage>>;

#[derive(Default)]
pub struct DapQuery;
#[derive(Default)]
pub struct DapMutation;

pub fn init<Q, M, S>(schema: SchemaBuilder<Q, M, S>) -> SchemaBuilder<Q, M, S> {
    schema.data(GraphStorage::default())
}

#[Object]
impl DapQuery {
    async fn register(
        &self,
        ctx: &Context<'_>,
        id: ID,
        register: RegisterId,
    ) -> async_graphql::Result<Word> {
        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .register(&id, register)
            .ok_or_else(|| async_graphql::Error::new("Invalid register identifier"))
    }

    async fn memory(
        &self,
        ctx: &Context<'_>,
        id: ID,
        start: usize,
        size: usize,
    ) -> async_graphql::Result<String> {
        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .memory(&id, start, size)
            .ok_or_else(|| async_graphql::Error::new("Invalid memory range"))
            .and_then(|mem| Ok(serde_json::to_string(mem)?))
    }
}

#[Object]
impl DapMutation {
    async fn start_session(&self, ctx: &Context<'_>) -> async_graphql::Result<ID> {
        trace!("Initializing new interpreter");

        let db = ctx.data_unchecked::<SharedDatabase>();

        let id = ctx
            .data_unchecked::<GraphStorage>()
            .lock()
            .await
            .init(&[], db.0.transaction())?;

        debug!("Session {:?} initialized", id);

        Ok(id)
    }

    async fn end_session(&self, ctx: &Context<'_>, id: ID) -> bool {
        let existed = ctx.data_unchecked::<GraphStorage>().lock().await.kill(&id);

        debug!("Session {:?} dropped with result {}", id, existed);

        existed
    }

    async fn reset(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<bool> {
        let db = ctx.data_unchecked::<SharedDatabase>();

        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .reset(&id, db.0.transaction())?;

        debug!("Session {:?} was reset", id);

        Ok(true)
    }

    async fn execute(&self, ctx: &Context<'_>, id: ID, op: String) -> async_graphql::Result<bool> {
        trace!("Execute encoded op {}", op);

        let op: Opcode = serde_json::from_str(op.as_str())?;

        trace!("Op decoded to {:?}", op);

        let storage = ctx.data_unchecked::<GraphStorage>().clone();
        let result = storage.lock().await.exec(&id, op).is_ok();

        debug!("Op {:?} executed with result {}", op, result);

        Ok(result)
    }
}
