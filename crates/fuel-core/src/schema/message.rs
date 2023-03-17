use std::ops::Deref;

use super::{
    block::Header,
    scalars::{
        Address,
        Bytes32,
        HexString,
        MessageId,
        Nonce,
        TransactionId,
        U64,
    },
};
use crate::{
    fuel_core_graphql_api::service::Database,
    query::MessageQueryData,
};
use anyhow::anyhow;
use async_graphql::{
    connection::{
        Connection,
        EmptyFields,
    },
    Context,
    Object,
};
use fuel_core_types::entities;

pub struct Message(pub(crate) entities::message::Message);

#[Object]
impl Message {
    async fn amount(&self) -> U64 {
        self.0.amount.into()
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

    async fn data(&self) -> HexString {
        self.0.data.clone().into()
    }

    async fn da_height(&self) -> U64 {
        self.0.da_height.as_u64().into()
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
    ) -> async_graphql::Result<Connection<HexString, Message, EmptyFields, EmptyFields>>
    {
        let query: &Database = ctx.data_unchecked();
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
                    // Rocksdb doesn't support reverse iteration over a prefix
                    if matches!(last, Some(last) if last > 0) {
                        return Err(anyhow!(
                            "reverse pagination isn't supported for this resource"
                        )
                        .into())
                    }

                    query.owned_messages(&owner.0, start, direction)
                } else {
                    query.all_messages(start, direction)
                };

                let messages = messages.map(|result| {
                    result
                        .map(|message| (message.nonce.into(), message.into()))
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
        message_id: MessageId,
    ) -> async_graphql::Result<Option<MessageProof>> {
        let data: &Database = ctx.data_unchecked();
        Ok(crate::query::message_proof(
            data.deref(),
            transaction_id.into(),
            message_id.into(),
        )?
        .map(MessageProof))
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

    async fn nonce(&self) -> Nonce {
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

impl From<entities::message::Message> for Message {
    fn from(message: entities::message::Message) -> Self {
        Message(message)
    }
}
