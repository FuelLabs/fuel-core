/*
 *
 * Contract
 let has_next_page = balances.len() > records_to_fetch;

 if has_next_page {
     balances.pop();
 }

 if direction == IterDirection::Reverse {
     balances.reverse();
 }

 let mut connection = Connection::new(started.is_some(), has_next_page);
 connection.edges.extend(
     balances
         .into_iter()
         .map(|item| Edge::new(item.asset_id.into(), item)),
 );
 Ok::<Connection<AssetId, ContractBalance>, anyhow::Error>(connection)
 * Balance
 *
                let mut balances: Vec<Balance> = balances.collect();
                if direction == IterDirection::Reverse {
                    balances.reverse();
                }

                let mut connection =
                    Connection::new(started.is_some(), records_to_fetch <= balances.len());

                connection.edges.extend(
                    balances
                        .into_iter()
                        .map(|item| Edge::new(item.asset_id.into(), item)),
                );

                Ok::<Connection<AssetId, Balance>, KvStoreError>(connection)
