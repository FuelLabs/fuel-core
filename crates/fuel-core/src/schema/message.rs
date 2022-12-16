use super::{
    block::Header,
    scalars::{
        Address,
        Bytes32,
        HexString,
        MessageId,
        TransactionId,
        U64,
    },
};
use crate::{
    database::Database,
    query::MessageProofData,
};
use async_graphql::{
    connection::{
        Connection,
        EmptyFields,
    },
    Context,
    Object,
};
use fuel_core_storage::{
    not_found,
    tables::{
        FuelBlocks,
        Messages,
        Receipts,
        SealedBlockConsensus,
        Transactions,
    },
    Result as StorageResult,
    StorageAsRef,
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::Consensus,
    },
    entities,
    fuel_tx,
    services::txpool::TransactionStatus,
};
use itertools::Itertools;
use std::borrow::Cow;

pub struct Message(pub(crate) entities::message::Message);

#[Object]
impl Message {
    async fn message_id(&self) -> MessageId {
        self.0.id().into()
    }

    async fn amount(&self) -> U64 {
        self.0.amount.into()
    }

    async fn sender(&self) -> Address {
        self.0.sender.into()
    }

    async fn recipient(&self) -> Address {
        self.0.recipient.into()
    }

    async fn nonce(&self) -> U64 {
        self.0.nonce.into()
    }

    async fn data(&self) -> HexString {
        self.0.data.clone().into()
    }

    async fn da_height(&self) -> U64 {
        self.0.da_height.as_u64().into()
    }

    async fn fuel_block_spend(&self) -> Option<U64> {
        self.0.fuel_block_spend.map(|v| v.into())
    }
}

#[derive(Default)]
pub struct MessageQuery {}

#[Object]
impl MessageQuery {
    async fn messages(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "address of the owner")] owner: Option<Address>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<MessageId, Message, EmptyFields, EmptyFields>>
    {
        let db = ctx.data_unchecked::<Database>().clone();
        crate::schema::query_pagination(after, before, first, last, |start, direction| {
            let start = *start;
            // TODO: Avoid the `collect_vec`.
            let messages = if let Some(owner) = owner {
                let message_ids: Vec<_> = db
                    .owned_message_ids(&owner.0, start.map(Into::into), Some(direction))
                    .try_collect()?;

                let messages = message_ids
                    .into_iter()
                    .map(|msg_id| {
                        let message = db
                            .storage::<Messages>()
                            .get(&msg_id)
                            .transpose()
                            .ok_or(not_found!(Messages))??
                            .into_owned();

                        Ok((msg_id.into(), message.into()))
                    })
                    .collect_vec()
                    .into_iter();
                Ok::<_, anyhow::Error>(messages)
            } else {
                let messages = db
                    .all_messages(start.map(Into::into), Some(direction))
                    .map(|result| {
                        result
                            .map(|message| (message.id().into(), message.into()))
                            .map_err(Into::into)
                    })
                    .collect_vec()
                    .into_iter();
                Ok(messages)
            }?;

            Ok(messages)
        })
        .await
    }

    async fn message_proof(
        &self,
        ctx: &Context<'_>,
        transaction_id: TransactionId,
        message_id: MessageId,
    ) -> async_graphql::Result<Option<MessageProof>> {
        let data = MessageProofContext(ctx.data_unchecked());
        Ok(
            crate::query::message_proof(&data, transaction_id.into(), message_id.into())
                .await?
                .map(MessageProof),
        )
    }
}

pub struct MessageProof(pub(crate) entities::message::MessageProof);

#[Object]
impl MessageProof {
    async fn proof_set(&self) -> Vec<Bytes32> {
        self.0
            .proof_set
            .iter()
            .cloned()
            .map(Bytes32::from)
            .collect()
    }

    async fn proof_index(&self) -> U64 {
        self.0.proof_index.into()
    }

    async fn sender(&self) -> Address {
        self.0.sender.into()
    }

    async fn recipient(&self) -> Address {
        self.0.recipient.into()
    }

    async fn nonce(&self) -> Bytes32 {
        self.0.nonce.into()
    }

    async fn amount(&self) -> U64 {
        self.0.amount.into()
    }

    async fn data(&self) -> HexString {
        self.0.data.clone().into()
    }

    async fn signature(&self) -> super::scalars::Signature {
        self.0.signature.into()
    }

    async fn header(&self) -> Header {
        Header(self.0.header.clone())
    }
}

struct MessageProofContext<'a>(&'a Database);

impl MessageProofData for MessageProofContext<'_> {
    fn receipts(
        &self,
        transaction_id: &fuel_core_types::fuel_types::Bytes32,
    ) -> StorageResult<Vec<fuel_tx::Receipt>> {
        Ok(self
            .0
            .storage::<Receipts>()
            .get(transaction_id)?
            .map(Cow::into_owned)
            .unwrap_or_else(|| Vec::with_capacity(0)))
    }

    fn transaction(
        &self,
        transaction_id: &fuel_core_types::fuel_types::Bytes32,
    ) -> StorageResult<Option<fuel_tx::Transaction>> {
        Ok(self
            .0
            .storage::<Transactions>()
            .get(transaction_id)?
            .map(Cow::into_owned))
    }

    fn transaction_status(
        &self,
        transaction_id: &fuel_core_types::fuel_types::Bytes32,
    ) -> StorageResult<Option<TransactionStatus>> {
        Ok(self.0.get_tx_status(transaction_id)?)
    }

    fn transactions_on_block(
        &self,
        block_id: &fuel_core_types::fuel_types::Bytes32,
    ) -> StorageResult<Vec<fuel_core_types::fuel_types::Bytes32>> {
        Ok(self
            .0
            .storage::<FuelBlocks>()
            .get(block_id)?
            .map(|block| block.into_owned().transactions().to_vec())
            .unwrap_or_else(|| Vec::with_capacity(0)))
    }

    fn signature(
        &self,
        block_id: &fuel_core_types::fuel_types::Bytes32,
    ) -> StorageResult<Option<fuel_core_types::fuel_crypto::Signature>> {
        match self
            .0
            .storage::<SealedBlockConsensus>()
            .get(block_id)?
            .map(Cow::into_owned)
        {
            // TODO: https://github.com/FuelLabs/fuel-core/issues/816
            Some(Consensus::Genesis(_)) => Ok(Default::default()),
            Some(Consensus::PoA(c)) => Ok(Some(c.signature)),
            None => Ok(None),
        }
    }

    fn block(
        &self,
        block_id: &fuel_core_types::fuel_types::Bytes32,
    ) -> StorageResult<Option<CompressedBlock>> {
        Ok(self
            .0
            .storage::<FuelBlocks>()
            .get(block_id)?
            .map(Cow::into_owned))
    }
}

impl From<entities::message::Message> for Message {
    fn from(message: entities::message::Message) -> Self {
        Message(message)
    }
}
