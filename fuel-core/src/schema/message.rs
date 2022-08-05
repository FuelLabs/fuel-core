use anyhow::anyhow;
use async_graphql::{
    connection::{self, Connection, Edge, EmptyFields},
    Context, Object,
};
use fuel_core_interfaces::{
    common::{fuel_storage::Storage, fuel_types},
    db::KvStoreError,
    model,
};
use itertools::Itertools;
use std::borrow::Cow;

use crate::{database::Database, state::IterDirection};

use super::scalars::{Address, MessageId, OwnerAndMessageId, U64};

pub struct DaMessage(pub(crate) model::DaMessage);

#[Object]
impl DaMessage {
    async fn amount(&self) -> U64 {
        self.0.amount.into()
    }

    async fn sender(&self) -> Address {
        self.0.sender.into()
    }

    async fn recipient(&self) -> Address {
        self.0.recipient.into()
    }

    async fn owner(&self) -> Address {
        self.0.owner.into()
    }

    async fn nonce(&self) -> U64 {
        self.0.nonce.into()
    }

    async fn data(&self) -> &Vec<u8> {
        &self.0.data
    }

    async fn da_height(&self) -> U64 {
        self.0.da_height.into()
    }

    async fn fuel_block_spend(&self) -> Option<u64> {
        self.0.fuel_block_spend.map(|v| v.into())
    }
}

#[derive(Default)]
pub struct MessageQuery {}

#[Object]
impl MessageQuery {
    async fn messages_by_owner(
        &self,
        ctx: &Context<'_>,
        owner: Address,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<MessageId, DaMessage, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>().clone();

        connection::query(
            after,
            before,
            first,
            last,
            |after: Option<MessageId>, before: Option<MessageId>, first, last| async move {
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
                    return Err(anyhow!("Wrong argument combination"));
                }

                let start;
                let end;

                if direction == IterDirection::Forward {
                    start = after;
                    end = before;
                } else {
                    start = before;
                    end = after;
                }

                let mut message_ids =
                    db.owned_message_ids(owner.into(), start.map(Into::into), Some(direction));
                let mut started = None;
                if start.is_some() {
                    // skip initial result
                    started = message_ids.next();
                }

                let compare_end = end.map(|v| v.into());

                let message_ids = message_ids
                    .take_while(|r| {
                        if let (Ok(next), Some(end)) = (r, &compare_end) {
                            if next == end {
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch);

                let mut message_ids: Vec<fuel_types::MessageId> = message_ids.try_collect()?;
                if direction == IterDirection::Forward {
                    message_ids.reverse();
                }

                let messages: Vec<Cow<model::DaMessage>> = message_ids
                    .iter()
                    .map(|msg_id| {
                        Storage::<fuel_types::MessageId, model::DaMessage>::get(&db, msg_id.into())
                            .transpose()
                            .ok_or(KvStoreError::NotFound)?
                    })
                    .try_collect()?;

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= messages.len());

                connection
                    .edges
                    .extend(
                        messages
                            .into_iter()
                            .zip(message_ids)
                            .map(|(message, msg_id)| {
                                Edge::new(msg_id.into(), DaMessage(message.into_owned()))
                            }),
                    );

                Ok::<Connection<MessageId, DaMessage>, anyhow::Error>(connection)
            },
        )
        .await
    }

    async fn messages(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<OwnerAndMessageId, DaMessage, EmptyFields, EmptyFields>>
    {
        let db = ctx.data_unchecked::<Database>().clone();

        connection::query(
            after,
            before,
            first,
            last,
            |after: Option<OwnerAndMessageId>,
             before: Option<OwnerAndMessageId>,
             first,
             last| async move {
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
                    return Err(anyhow!("Wrong argument combination"));
                }

                let start;
                let end;

                if direction == IterDirection::Forward {
                    start = after;
                    end = before;
                } else {
                    start = before;
                    end = after;
                }

                let mut message_ids = db.all_owners_and_message_ids(start.clone(), Some(direction));
                let mut started = None;
                if start.is_some() {
                    // skip initial result
                    started = message_ids.next();
                }

                let message_ids = message_ids
                    .take_while(|r| {
                        if let (Ok(next), Some(end)) = (r, &end) {
                            if next.message_id.0 == end.message_id.0 {
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch);

                let mut message_ids: Vec<OwnerAndMessageId> = message_ids.try_collect()?;
                if direction == IterDirection::Forward {
                    message_ids.reverse();
                }

                let messages: Vec<Cow<model::DaMessage>> = message_ids
                    .iter()
                    .map(|value| {
                        Storage::<fuel_types::MessageId, model::DaMessage>::get(
                            &db,
                            &value.message_id.into(),
                        )
                        .transpose()
                        .ok_or(KvStoreError::NotFound)?
                    })
                    .try_collect()?;

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= messages.len());

                connection
                    .edges
                    .extend(messages.into_iter().zip(message_ids).map(
                        |(message, owner_msg_id)| {
                            Edge::new(owner_msg_id, DaMessage(message.into_owned()))
                        },
                    ));

                Ok::<Connection<OwnerAndMessageId, DaMessage>, anyhow::Error>(connection)
            },
        )
        .await
    }
}
