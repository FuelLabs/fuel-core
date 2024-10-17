use fuel_core_chain_config::{
    AddTable,
    AsTable,
    StateConfig,
    StateConfigBuilder,
    TableEntry,
};
use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        manual::Manual,
        postcard::Postcard,
        raw::Raw,
        Decode,
        Encode,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_types::{
    fuel_tx::{
        Address,
        AssetId,
        Bytes32,
        Bytes64,
        Bytes8,
    },
    fuel_types::BlockHeight,
    fuel_vm::double_key,
    services::txpool::TransactionStatus,
};
use rand::{
    distributions::Standard,
    prelude::Distribution,
    Rng,
};
use std::{
    array::TryFromSliceError,
    mem::size_of,
};

type Amount = u64;

double_key!(BalancesKey, Address, address, AssetId, asset_id);
impl Distribution<BalancesKey> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> BalancesKey {
        let mut bytes = [0u8; BalancesKey::LEN];
        rng.fill_bytes(bytes.as_mut());
        BalancesKey::from_array(bytes)
    }
}

/// These table stores the balances of asset id per owner.
pub struct Balances;

impl Mappable for Balances {
    type Key = BalancesKey;
    type OwnedKey = Self::Key;
    type Value = Amount;
    type OwnedValue = Self::Value;
}

impl TableWithBlueprint for Balances {
    type Blueprint = Plain<Raw, Postcard>; // TODO[RC]: What is Plain, Raw, Postcard, Primitive<N> and others in this context?
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::Balances
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fuel_core_storage::{
        iter::IterDirection,
        StorageInspect,
        StorageMutate,
    };
    use fuel_core_types::fuel_tx::{
        Address,
        AssetId,
        Bytes64,
        Bytes8,
    };

    use crate::combined_database::CombinedDatabase;

    use super::{
        Balances,
        BalancesKey,
    };

    pub struct TestDatabase {
        database: CombinedDatabase,
    }

    impl TestDatabase {
        pub fn new() -> Self {
            Self {
                database: Default::default(),
            }
        }

        pub fn balance_tx(
            &mut self,
            owner: &Address,
            (asset_id, amount): &(AssetId, u64),
        ) {
            let current_balance = self.query_balance(owner, asset_id);
            let new_balance = current_balance.unwrap_or(0) + amount;

            let db = self.database.off_chain_mut();
            let key = BalancesKey::new(owner, asset_id);
            let _ = StorageMutate::<Balances>::insert(db, &key, &new_balance)
                .expect("couldn't store test asset");
        }

        pub fn query_balance(&self, owner: &Address, asset_id: &AssetId) -> Option<u64> {
            let db = self.database.off_chain();
            let key = BalancesKey::new(owner, asset_id);
            let result = StorageInspect::<Balances>::get(db, &key).unwrap();

            result.map(|r| r.into_owned())
        }

        pub fn query_balances(&self, owner: &Address) -> HashMap<AssetId, u64> {
            let db = self.database.off_chain();

            let mut key_prefix = owner.as_ref().to_vec();
            db.entries::<Balances>(Some(key_prefix), IterDirection::Forward)
                .map(|asset| {
                    let asset = asset.expect("TODO[RC]: Fixme");
                    let asset_id = asset.key.asset_id().clone();
                    let balance = asset.value;
                    (asset_id, balance)
                })
                .collect()
        }
    }

    #[test]
    fn can_retrieve_balance_of_asset() {
        let mut db = TestDatabase::new();

        let alice = Address::from([1; 32]);
        let bob = Address::from([2; 32]);
        let carol = Address::from([3; 32]);

        let ASSET_1 = AssetId::from([1; 32]);
        let ASSET_2 = AssetId::from([2; 32]);

        // Alice has 100 of asset 1 and a total of 1000 of asset 2
        let alice_tx_1 = (ASSET_1, 100_u64);
        let alice_tx_2 = (ASSET_2, 600_u64);
        let alice_tx_3 = (ASSET_2, 400_u64);

        // Carol has 200 of asset 2
        let carol_tx_1 = (ASSET_2, 200_u64);

        let res = db.balance_tx(&alice, &alice_tx_1);
        let res = db.balance_tx(&alice, &alice_tx_2);
        let res = db.balance_tx(&alice, &alice_tx_3);
        let res = db.balance_tx(&carol, &carol_tx_1);

        // Alice has correct balances
        assert_eq!(db.query_balance(&alice, &alice_tx_1.0), Some(100));
        assert_eq!(db.query_balance(&alice, &alice_tx_2.0), Some(1000));

        // Carol has correct balances
        assert_eq!(db.query_balance(&carol, &carol_tx_1.0), Some(200_u64));
    }

    #[test]
    fn can_retrieve_balances_of_all_assets_of_owner() {
        let mut db = TestDatabase::new();

        let alice = Address::from([1; 32]);
        let bob = Address::from([2; 32]);
        let carol = Address::from([3; 32]);

        let ASSET_1 = AssetId::from([1; 32]);
        let ASSET_2 = AssetId::from([2; 32]);

        // Alice has 100 of asset 1 and a total of 1000 of asset 2
        let alice_tx_1 = (ASSET_1, 100_u64);
        let alice_tx_2 = (ASSET_2, 600_u64);
        let alice_tx_3 = (ASSET_2, 400_u64);

        // Carol has 200 of asset 2
        let carol_tx_1 = (ASSET_2, 200_u64);

        let res = db.balance_tx(&alice, &alice_tx_1);
        let res = db.balance_tx(&alice, &alice_tx_2);
        let res = db.balance_tx(&alice, &alice_tx_3);
        let res = db.balance_tx(&carol, &carol_tx_1);

        // Verify Alice balances
        let expected: HashMap<_, _> = vec![(ASSET_1, 100_u64), (ASSET_2, 1000_u64)]
            .into_iter()
            .collect();
        let actual = db.query_balances(&alice);
        assert_eq!(expected, actual);

        // Verify Bob balances
        let actual = db.query_balances(&bob);
        assert_eq!(HashMap::new(), actual);

        // Verify Carol balances
        let expected: HashMap<_, _> = vec![(ASSET_2, 200_u64)].into_iter().collect();
        let actual = db.query_balances(&carol);
        assert_eq!(expected, actual);
    }

    fuel_core_storage::basic_storage_tests!(
        Balances,
        <Balances as fuel_core_storage::Mappable>::Key::default(),
        <Balances as fuel_core_storage::Mappable>::Value::default()
    );
}
