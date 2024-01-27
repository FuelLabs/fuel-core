use super::{
    block::Header,
    scalars::{
        Address,
        Bytes32,
        HexString,
        Nonce,
        TransactionId,
        U64,
    },
};
use crate::{
    fuel_core_graphql_api::{
        database::ReadView,
        ports::DatabaseBlocks,
    },
    graphql_api::IntoApiResult,
    query::MessageQueryData,
    schema::scalars::{
        BlockId,
        U32,
    },
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        Connection,
        EmptyFields,
    },
    Context,
    Enum,
    Object,
};
use fuel_core_types::entities;

pub struct Message(pub(crate) entities::message::Message);

#[Object]
impl Message {
    async fn amount(&self) -> U64 {
        self.0.amount().into()
    }

    async fn sender(&self) -> Address {
        (*self.0.sender()).into()
    }

    async fn recipient(&self) -> Address {
        (*self.0.recipient()).into()
    }

    async fn nonce(&self) -> Nonce {
        (*self.0.nonce()).into()
    }

    async fn data(&self) -> HexString {
        self.0.data().clone().into()
    }

    async fn da_height(&self) -> U64 {
        self.0.da_height().as_u64().into()
    }
}

#[derive(Default)]
pub struct MessageQuery {}

#[Object]
impl MessageQuery {
    async fn message(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The Nonce of the message")] nonce: Nonce,
    ) -> async_graphql::Result<Option<Message>> {
        let query: &ReadView = ctx.data_unchecked();
        let nonce = nonce.0;
        query.message(&nonce).into_api_result()
    }

    async fn messages(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "address of the owner")] owner: Option<Address>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<HexString, Message, EmptyFields, EmptyFields>>
    {
        let query: &ReadView = ctx.data_unchecked();
        crate::schema::query_pagination(
            after,
            before,
            first,
            last,
            |start: &Option<HexString>, direction| {
                let start = if let Some(start) = start.clone() {
                    Some(start.try_into().map_err(|err| anyhow!("{}", err))?)
                } else {
                    None
                };

                let messages = if let Some(owner) = owner {
                    query.owned_messages(&owner.0, start, direction)
                } else {
                    query.all_messages(start, direction)
                };

                let messages = messages.map(|result| {
                    result
                        .map(|message| ((*message.nonce()).into(), message.into()))
                        .map_err(Into::into)
                });

                Ok(messages)
            },
        )
        .await
    }

    async fn message_proof(
        &self,
        ctx: &Context<'_>,
        transaction_id: TransactionId,
        nonce: Nonce,
        commit_block_id: Option<BlockId>,
        commit_block_height: Option<U32>,
    ) -> async_graphql::Result<Option<MessageProof>> {
        let query: &ReadView = ctx.data_unchecked();
        let height = match (commit_block_id, commit_block_height) {
            (Some(commit_block_id), None) => {
                query.block_height(&commit_block_id.0.into())?
            },
            (None, Some(commit_block_height)) => {
                commit_block_height.0.into()
            }
            _ => Err(anyhow::anyhow!(
                "Either `commit_block_id` or `commit_block_height` must be provided exclusively"
            ))?,
        };

        Ok(crate::query::message_proof(
            query,
            transaction_id.into(),
            nonce.into(),
            height,
        )?
        .map(MessageProof))
    }

    async fn message_status(
        &self,
        ctx: &Context<'_>,
        nonce: Nonce,
    ) -> async_graphql::Result<MessageStatus> {
        let query: &ReadView = ctx.data_unchecked();
        let status = crate::query::message_status(query, nonce.into())?;
        Ok(status.into())
    }
}
pub struct MerkleProof(pub(crate) entities::message::MerkleProof);

#[Object]
impl MerkleProof {
    async fn proof_set(&self) -> Vec<Bytes32> {
        self.0
            .proof_set
            .iter()
            .cloned()
            .map(|array| Bytes32::from(fuel_core_types::fuel_types::Bytes32::from(array)))
            .collect()
    }

    async fn proof_index(&self) -> U64 {
        self.0.proof_index.into()
    }
}

pub struct MessageProof(pub(crate) entities::message::MessageProof);

#[Object]
impl MessageProof {
    async fn message_proof(&self) -> MerkleProof {
        self.0.message_proof.clone().into()
    }

    async fn block_proof(&self) -> MerkleProof {
        self.0.block_proof.clone().into()
    }

    async fn message_block_header(&self) -> Header {
        self.0.message_block_header.clone().into()
    }

    async fn commit_block_header(&self) -> Header {
        self.0.commit_block_header.clone().into()
    }

    async fn sender(&self) -> Address {
        self.0.sender.into()
    }

    async fn recipient(&self) -> Address {
        self.0.recipient.into()
    }

    async fn nonce(&self) -> Nonce {
        self.0.nonce.into()
    }

    async fn amount(&self) -> U64 {
        self.0.amount.into()
    }

    async fn data(&self) -> HexString {
        self.0.data.clone().into()
    }
}

impl From<entities::message::Message> for Message {
    fn from(message: entities::message::Message) -> Self {
        Message(message)
    }
}

impl From<entities::message::MerkleProof> for MerkleProof {
    fn from(proof: entities::message::MerkleProof) -> Self {
        MerkleProof(proof)
    }
}

pub struct MessageStatus(pub(crate) entities::message::MessageStatus);

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum MessageState {
    Unspent,
    Spent,
    NotFound,
}

#[Object]
impl MessageStatus {
    async fn state(&self) -> MessageState {
        match self.0.state {
            entities::message::MessageState::Unspent => MessageState::Unspent,
            entities::message::MessageState::Spent => MessageState::Spent,
            entities::message::MessageState::NotFound => MessageState::NotFound,
        }
    }
}

impl From<entities::message::MessageStatus> for MessageStatus {
    fn from(status: entities::message::MessageStatus) -> Self {
        MessageStatus(status)
    }
}
