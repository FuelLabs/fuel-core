use actix_web::web;
use async_graphql::types::EmptySubscription;
use async_graphql::{Context, Object, Schema, ID};
use async_graphql_actix_web::{Request, Response};
use fuel_vm::consts;
use futures::lock::Mutex;
use tracing::{debug, trace};
use uuid::Uuid;

use std::collections::HashMap;
use std::{io, sync};

use fuel_vm::prelude::*;

#[derive(Debug, Clone, Default)]
// TODO replace for dyn dispatch
pub struct ConcreteStorage<S>
where
    S: InterpreterStorage,
{
    vm: HashMap<ID, Interpreter<S>>,
    tx: HashMap<ID, Vec<Transaction>>,
}

impl<S> ConcreteStorage<S>
where
    S: InterpreterStorage,
{
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

    pub fn init(&mut self, txs: &[Transaction], storage: S) -> Result<ID, ExecuteError> {
        let id = Uuid::new_v4();
        let id = ID::from(id);

        let tx = txs.first().cloned().unwrap_or_default();
        self.tx
            .get_mut(&id)
            .map(|tx| tx.extend_from_slice(txs))
            .unwrap_or_else(|| {
                self.tx.insert(id.clone(), txs.to_owned());
            });

        let mut vm = Interpreter::with_storage(storage);
        vm.init(tx)?;
        self.vm.insert(id.clone(), vm);

        Ok(id)
    }

    pub fn kill(&mut self, id: &ID) -> bool {
        self.tx.remove(id);
        self.vm.remove(id).is_some()
    }

    pub fn reset(&mut self, id: &ID) -> Result<(), ExecuteError> {
        let tx = self
            .tx
            .get(id)
            .and_then(|tx| tx.first())
            .cloned()
            .unwrap_or_default();

        self.vm
            .get_mut(id)
            .map(|vm| vm.init(tx))
            .transpose()?
            .ok_or(ExecuteError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "The VM isntance was not found",
            )))
    }

    pub fn exec(&mut self, id: &ID, op: Opcode) -> Result<(), ExecuteError> {
        self.vm
            .get_mut(id)
            .map(|vm| vm.execute(op))
            .transpose()?
            .map(|_| ())
            .ok_or(ExecuteError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "The VM isntance was not found",
            )))
    }
}

pub type GraphStorage = sync::Arc<Mutex<ConcreteStorage<MemoryStorage>>>;
pub struct QueryRoot;
pub struct MutationRoot;

pub type DAPSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn schema() -> DAPSchema {
    let storage = GraphStorage::default();

    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(storage)
        .finish()
}

pub async fn service(schema: web::Data<DAPSchema>, req: Request) -> Response {
    schema.execute(req.into_inner()).await.into()
}

#[Object]
impl QueryRoot {
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
            .ok_or(async_graphql::Error::new("Invalid register identifier"))
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
            .ok_or(async_graphql::Error::new("Invalid memory range"))
            .and_then(|mem| Ok(serde_json::to_string(mem)?))
    }
}

#[Object]
impl MutationRoot {
    async fn start_session(&self, ctx: &Context<'_>) -> async_graphql::Result<ID> {
        trace!("Initializing new interpreter");

        let id = ctx
            .data_unchecked::<GraphStorage>()
            .lock()
            .await
            .init(&[], MemoryStorage::default())?;

        debug!("Session {:?} initialized", id);

        Ok(id)
    }

    async fn end_session(&self, ctx: &Context<'_>, id: ID) -> bool {
        let existed = ctx.data_unchecked::<GraphStorage>().lock().await.kill(&id);

        debug!("Session {:?} dropped with result {}", id, existed);

        existed
    }

    async fn reset(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<bool> {
        ctx.data_unchecked::<GraphStorage>()
            .lock()
            .await
            .reset(&id)?;

        debug!("Session {:?} was reset", id);

        Ok(true)
    }

    async fn execute(&self, ctx: &Context<'_>, id: ID, op: String) -> async_graphql::Result<bool> {
        trace!("Execute encoded op {}", op);

        let op: Opcode = serde_json::from_str(op.as_str())?;

        trace!("Op decoded to {:?}", op);

        let result = ctx
            .data_unchecked::<GraphStorage>()
            .lock()
            .await
            .exec(&id, op)
            .is_ok();

        debug!("Op {:?} executed with result {}", op, result);

        Ok(result)
    }
}
