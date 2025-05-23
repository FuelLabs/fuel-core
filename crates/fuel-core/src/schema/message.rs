use super::{
    ReadViewProvider,
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
    fuel_core_graphql_api::query_costs,
    graphql_api::IntoApiResult,
    schema::scalars::{
        BlockId,
        U32,
    },
};
use anyhow::anyhow;
use async_graphql::{
    Context,
    Enum,
    Object,
    connection::{
        Connection,
        EmptyFields,
    },
};
use fuel_core_services::stream::IntoBoxStream;
use fuel_core_types::entities;
use futures::StreamExt;

pub struct Message(pub(crate) entities::relayer::message::Message);

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
    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn message(
        &self,
        ctx: &Context<'_>,
        #[graphql(desc = "The Nonce of the message")] nonce: Nonce,
    ) -> async_graphql::Result<Option<Message>> {
        let query = ctx.read_view()?;
        let nonce = nonce.0;
        query.message(&nonce).into_api_result()
    }

    #[graphql(complexity = "{\
        query_costs().storage_iterator\
        + (query_costs().storage_read + first.unwrap_or_default() as usize) * child_complexity \
        + (query_costs().storage_read + last.unwrap_or_default() as usize) * child_complexity\
    }")]
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
        let query = ctx.read_view()?;
        let owner = owner.map(|owner| owner.0);
        let owner_ref = owner.as_ref();
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

                let messages = if let Some(owner) = owner_ref {
                    query
                        .owned_messages(owner, start, direction)
                        .into_boxed_ref()
                } else {
                    query.all_messages(start, direction).into_boxed_ref()
                };

                let messages = messages.map(|result| {
                    result.map(|message| ((*message.nonce()).into(), message.into()))
                });

                Ok(messages)
            },
        )
        .await
    }

    // 256 * QUERY_COSTS.storage_read because the depth of the Merkle tree in the worst case is 256
    #[graphql(complexity = "256 * query_costs().storage_read + child_complexity")]
    async fn message_proof(
        &self,
        ctx: &Context<'_>,
        transaction_id: TransactionId,
        nonce: Nonce,
        commit_block_id: Option<BlockId>,
        commit_block_height: Option<U32>,
    ) -> async_graphql::Result<MessageProof> {
        let query = ctx.read_view()?;
        let height = match (commit_block_id, commit_block_height) {
            (Some(commit_block_id), None) => {
                query.block_height(&commit_block_id.0.into())?
            }
            (None, Some(commit_block_height)) => commit_block_height.0.into(),
            _ => Err(anyhow::anyhow!(
                "Either `commit_block_id` or `commit_block_height` must be provided exclusively"
            ))?,
        };

        let proof = crate::query::message_proof(
            query.as_ref(),
            transaction_id.into(),
            nonce.into(),
            height,
        )?;

        Ok(MessageProof(proof))
    }

    #[graphql(complexity = "query_costs().storage_read + child_complexity")]
    async fn message_status(
        &self,
        ctx: &Context<'_>,
        nonce: Nonce,
    ) -> async_graphql::Result<MessageStatus> {
        let query = ctx.read_view()?;
        let status = crate::query::message_status(query.as_ref(), nonce.into())?;
        Ok(status.into())
    }
}
pub struct MerkleProof(pub(crate) entities::relayer::message::MerkleProof);

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

pub struct MessageProof(pub(crate) entities::relayer::message::MessageProof);

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

impl From<entities::relayer::message::Message> for Message {
    fn from(message: entities::relayer::message::Message) -> Self {
        Message(message)
    }
}

impl From<entities::relayer::message::MerkleProof> for MerkleProof {
    fn from(proof: entities::relayer::message::MerkleProof) -> Self {
        MerkleProof(proof)
    }
}

pub struct MessageStatus(pub(crate) entities::relayer::message::MessageStatus);

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
            entities::relayer::message::MessageState::Unspent => MessageState::Unspent,
            entities::relayer::message::MessageState::Spent => MessageState::Spent,
            entities::relayer::message::MessageState::NotFound => MessageState::NotFound,
        }
    }
}

impl From<entities::relayer::message::MessageStatus> for MessageStatus {
    fn from(status: entities::relayer::message::MessageStatus) -> Self {
        MessageStatus(status)
    }
}
