use error::Result;
use fuel_core_services::{
    RunnableTask,
    StateWatcher,
    TaskNextAction,
};
use fuel_core_types::fuel_tx::Transaction;

pub mod error;

pub struct PreConfirmationTask<
    TxReceiver,
    ParentSignature,
    KeyGenerator,
    Key,
    KeyRotationTrigger,
> {
    _tx_receiver: TxReceiver,
    _parent_signature: ParentSignature,
    _key_generator: KeyGenerator,
    _current_delegate_key: Key,
    _key_rotation_trigger: KeyRotationTrigger,
}

#[async_trait::async_trait]
pub trait TxReceiver: Send {
    async fn _receive(&mut self) -> Result<Vec<Transaction>>;
}

#[allow(dead_code)]
pub struct Sealed<T> {
    _phantom: std::marker::PhantomData<T>,
}

pub trait ParentSignature: Send {
    fn _sign<T>(&self, data: T) -> Sealed<T>;
}

pub trait KeyGenerator: Send {
    type Key: SigningKey;
    fn _generate(&mut self) -> Result<Self::Key>;
}

pub trait SigningKey: Send {}

#[async_trait::async_trait]
pub trait KeyRotationTrigger: Send {
    async fn _next_rotation(&self) -> Result<()>;
}

impl<TxRcv, Parent, Gen, Key, Trigger>
    PreConfirmationTask<TxRcv, Parent, Gen, Key, Trigger>
where
    TxRcv: TxReceiver,
    Parent: ParentSignature,
    Gen: KeyGenerator<Key = Key>,
    Key: SigningKey,
    Trigger: KeyRotationTrigger,
{
    pub async fn _run(&mut self, _watcher: &mut StateWatcher) -> Result<()> {
        todo!()
    }
}

impl<TxRcv, Parent, Gen, Key, Trigger> RunnableTask
    for PreConfirmationTask<TxRcv, Parent, Gen, Key, Trigger>
where
    TxRcv: TxReceiver,
    Parent: ParentSignature,
    Gen: KeyGenerator<Key = Key>,
    Key: SigningKey,
    Trigger: KeyRotationTrigger,
{
    async fn run(&mut self, watcher: &mut StateWatcher) -> TaskNextAction {
        let res = self._run(watcher).await;
        TaskNextAction::always_continue(res)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        todo!()
    }
}
