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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::transaction::TransactionIndex;
    use async_graphql::{EmptyMutation, EmptySubscription, Request, Schema, Variables};
    use fuel_asm::Opcode;
    use fuel_tx::Transaction as FuelTx;
    use rand::{prelude::StdRng, Rng, SeedableRng};
    use serde_json::json;

    #[tokio::test]
    async fn account_transactions_output() {
        let mut rng = StdRng::seed_from_u64(1);
        let owner: Address = rng.gen();

        let mut db = Database::default();
        let txns = (0..10)
            .map(|_| FuelTx::Script {
                gas_price: 0,
                gas_limit: 1_000_000,
                byte_price: 0,
                maturity: 0,
                receipts_root: Default::default(),
                script: Opcode::RET(0x10).to_bytes().to_vec(),
                script_data: vec![],
                inputs: vec![],
                outputs: vec![],
                witnesses: vec![vec![].into()],
                metadata: None,
            })
            .collect::<Vec<_>>();

        for (i, tx) in txns.iter().enumerate() {
            let tx_id = tx.id();
            Storage::<Bytes32, FuelTx>::insert(&mut db, &tx_id, tx).unwrap();
            db.record_tx_id_owner(&owner, 0u64.into(), i as TransactionIndex, &tx_id)
                .unwrap();
        }

        let schema = Schema::build(AccountQuery, EmptyMutation, EmptySubscription)
            .data(db)
            .finish();
        let query = Request::new(
            r#"
                query AccountTransactionsQuery($address: HexString256) {
                    account(address: $address) {
                        transactions(first: 10) {
                            edges {
                                node {
                                    id
                                }
                            }
                        }
                    }
                }
            "#,
        )
        .variables(Variables::from_json(json!({
            "address": owner.to_string()
        })));

        let result = schema.execute(query).await;

        let expected_edges = txns
            .iter()
            .map(|tx| json!({ "node": { "id": tx.id().to_string() } }))
            .collect::<Vec<_>>();

        assert_eq!(
            result.data.into_json().unwrap(),
            json!({
                "account": {
                    "transactions": {
                        "edges": expected_edges
                    }
                }
            })
        )
    }
}
