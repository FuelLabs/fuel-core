use std::borrow::Cow;

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
    database::{
        storage::{
            FuelBlocks,
            Receipts,
            SealedBlockConsensus,
        },
        Database,
    },
    query::MessageProofData,
    state::IterDirection,
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        self,
        Connection,
        Edge,
        EmptyFields,
    },
    Context,
    Object,
};
use fuel_core_interfaces::{
    common::{
        fuel_storage::StorageAsRef,
        fuel_types,
    },
    db::{
        KvStoreError,
        Messages,
        Transactions,
    },
    model::{
        self,
        FuelBlockConsensus,
    },
};
use itertools::Itertools;

pub struct Message(pub(crate) model::Message);

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

        connection::query(
            after,
            before,
            first,
            last,
            |after: Option<MessageId>, before: Option<MessageId>, first, last| {
                async move {
                    let (records_to_fetch, direction) = if let Some(first) = first {
                        (first, IterDirection::Forward)
                    } else if let Some(last) = last {
                        (last, IterDirection::Reverse)
                    } else {
                        (0, IterDirection::Forward)
                    };

                    if (first.is_some() && before.is_some())
                        || (after.is_some() && before.is_some())
                        || (last.is_some() && after.is_some())
                    {
                        return Err(anyhow!("Wrong argument combination"))
                    }

                    let start = if direction == IterDirection::Forward {
                        after
                    } else {
                        before
                    };

                    let (mut messages, has_next_page, has_previous_page) =
                        if let Some(owner) = owner {
                            let mut message_ids = db.owned_message_ids(
                                &owner.0,
                                start.map(Into::into),
                                Some(direction),
                            );
                            let mut started = None;
                            if start.is_some() {
                                // skip initial result
                                started = message_ids.next();
                            }
                            let message_ids = message_ids.take(records_to_fetch + 1);
                            let message_ids: Vec<fuel_types::MessageId> =
                                message_ids.try_collect()?;
                            let has_next_page = message_ids.len() > records_to_fetch;

                            let messages: Vec<model::Message> = message_ids
                                .iter()
                                .take(records_to_fetch)
                                .map(|msg_id| {
                                    db.storage::<Messages>()
                                        .get(msg_id)
                                        .transpose()
                                        .ok_or(KvStoreError::NotFound)?
                                        .map(|f| f.into_owned())
                                })
                                .try_collect()?;
                            (messages, has_next_page, started.is_some())
                        } else {
                            let mut messages =
                                db.all_messages(start.map(Into::into), Some(direction));
                            let mut started = None;
                            if start.is_some() {
                                // skip initial result
                                started = messages.next();
                            }
                            let messages: Vec<model::Message> =
                                messages.take(records_to_fetch + 1).try_collect()?;
                            let has_next_page = messages.len() > records_to_fetch;
                            let messages =
                                messages.into_iter().take(records_to_fetch).collect();
                            (messages, has_next_page, started.is_some())
                        };

                    // reverse after filtering next page test record to maintain consistent ordering
                    // in the response regardless of whether first or last was used.
                    if direction == IterDirection::Forward {
                        messages.reverse();
                    }

                    let mut connection =
                        Connection::new(has_previous_page, has_next_page);

                    connection.edges.extend(
                        messages.into_iter().map(|message| {
                            Edge::new(message.id().into(), Message(message))
                        }),
                    );

                    Ok::<Connection<MessageId, Message>, anyhow::Error>(connection)
                }
            },
        )
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

pub struct MessageProof(pub(crate) model::MessageProof);

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
        transaction_id: &fuel_core_interfaces::common::prelude::Bytes32,
    ) -> Result<Vec<fuel_core_interfaces::common::prelude::Receipt>, KvStoreError> {
        Ok(self
            .0
            .storage::<Receipts>()
            .get(transaction_id)?
            .map(Cow::into_owned)
            .unwrap_or_else(|| Vec::with_capacity(0)))
    }

    fn transaction(
        &self,
        transaction_id: &fuel_core_interfaces::common::prelude::Bytes32,
    ) -> Result<Option<fuel_txpool::types::Transaction>, KvStoreError> {
        Ok(self
            .0
            .storage::<Transactions>()
            .get(transaction_id)?
            .map(Cow::into_owned))
    }

    fn transaction_status(
        &self,
        transaction_id: &fuel_core_interfaces::common::prelude::Bytes32,
    ) -> Result<Option<crate::tx_pool::TransactionStatus>, KvStoreError> {
        Ok(self.0.get_tx_status(transaction_id)?)
    }

    fn transactions_on_block(
        &self,
        block_id: &fuel_core_interfaces::common::prelude::Bytes32,
    ) -> Result<Vec<fuel_core_interfaces::common::prelude::Bytes32>, KvStoreError> {
        Ok(self
            .0
            .storage::<FuelBlocks>()
            .get(block_id)?
            .map(|block| block.into_owned().transactions)
            .unwrap_or_else(|| Vec::with_capacity(0)))
    }

    fn signature(
        &self,
        block_id: &fuel_core_interfaces::common::prelude::Bytes32,
    ) -> Result<Option<fuel_core_interfaces::common::fuel_crypto::Signature>, KvStoreError>
    {
        match self
            .0
            .storage::<SealedBlockConsensus>()
            .get(block_id)?
            .map(Cow::into_owned)
        {
            Some(FuelBlockConsensus::PoA(c)) => Ok(Some(c.signature)),
            None => Ok(None),
        }
    }

    fn block(
        &self,
        block_id: &fuel_core_interfaces::common::prelude::Bytes32,
    ) -> Result<Option<model::FuelBlockDb>, KvStoreError> {
        Ok(self
            .0
            .storage::<FuelBlocks>()
            .get(block_id)?
            .map(Cow::into_owned))
    }
}
