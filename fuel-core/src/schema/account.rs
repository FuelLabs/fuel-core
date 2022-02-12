use crate::database::transaction::OwnedTransactionIndexCursor;
use crate::database::KvStoreError;
use crate::schema::scalars::HexString;
use crate::schema::tx::types::Transaction;
use crate::state::IterDirection;
use crate::{database::Database, schema::scalars::HexString256};
use async_graphql::connection::{Connection, Edge, EmptyFields};
use async_graphql::{connection::query, Context, Object};
use fuel_storage::Storage;
use fuel_tx::Transaction as FuelTx;
use fuel_tx::{Address, Bytes32};
use itertools::Itertools;

pub struct Account(Address);

impl From<Address> for Account {
    fn from(address: Address) -> Self {
        Self(address)
    }
}

#[Object]
impl Account {
    async fn address(&self) -> HexString256 {
        self.0.into()
    }

    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
    ) -> async_graphql::Result<Connection<HexString, Transaction, EmptyFields, EmptyFields>> {
        let db = ctx.data_unchecked::<Database>();
        let owner = self.0;

        query(
            after,
            before,
            first,
            last,
            |after: Option<HexString>, before: Option<HexString>, first, last| async move {
                let (records_to_fetch, direction) = if let Some(first) = first {
                    (first, IterDirection::Forward)
                } else if let Some(last) = last {
                    (last, IterDirection::Reverse)
                } else {
                    (0, IterDirection::Forward)
                };

                let after = after.map(OwnedTransactionIndexCursor::from);
                let before = before.map(OwnedTransactionIndexCursor::from);

                let start;
                let end;

                if direction == IterDirection::Forward {
                    start = after;
                    end = before;
                } else {
                    start = before;
                    end = after;
                }

                let mut txs = db.owned_transactions(&owner, start.as_ref(), Some(direction));
                let mut started = None;
                if start.is_some() {
                    // skip initial result
                    started = txs.next();
                }

                // take desired amount of results
                let txs = txs
                    .take_while(|r| {
                        // take until we've reached the end
                        if let (Ok(t), Some(end)) = (r, end.as_ref()) {
                            if &t.0 == end {
                                return false;
                            }
                        }
                        true
                    })
                    .take(records_to_fetch)
                    .map(|res| {
                        res.and_then(|(cursor, tx_id)| {
                            let tx = Storage::<Bytes32, FuelTx>::get(db, &tx_id)?
                                .ok_or(KvStoreError::NotFound)?
                                .into_owned();
                            Ok((cursor, tx))
                        })
                    });
                let mut txs: Vec<(OwnedTransactionIndexCursor, FuelTx)> = txs.try_collect()?;
                if direction == IterDirection::Reverse {
                    txs.reverse();
                }

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= txs.len());
                connection.append(
                    txs.into_iter()
                        .map(|item| Edge::new(HexString::from(item.0), Transaction(item.1))),
                );
                Ok(connection)
            },
        )
        .await
    }
}

#[derive(Default)]
pub struct AccountQuery;

#[Object]
impl AccountQuery {
    async fn account(
        &self,
        #[graphql(desc = "Address of the Account")] address: HexString256,
    ) -> async_graphql::Result<Option<Account>> {
        let account = Account(address.into());
        Ok(Some(account))
    }
}
