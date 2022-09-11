use crate::{
    database::Database,
    model::{
        Coin,
        CoinStatus,
    },
};
use fuel_core_interfaces::common::{
    fuel_asm::Word,
    fuel_storage::Storage,
    fuel_tx::{
        Address,
        AssetId,
        Bytes32,
        UtxoId,
    },
};
use itertools::Itertools;

#[derive(Default)]
pub struct TestDatabase {
    database: Database,
    last_coin_index: u64,
}

impl TestDatabase {
    pub fn make_coin(
        &mut self,
        owner: Address,
        amount: Word,
        asset_id: AssetId,
    ) -> (UtxoId, Coin) {
        let index = self.last_coin_index;
        self.last_coin_index += 1;

        let id = UtxoId::new(Bytes32::from([0u8; 32]), index.try_into().unwrap());
        let coin = Coin {
            owner,
            amount,
            asset_id,
            maturity: Default::default(),
            status: CoinStatus::Unspent,
            block_created: Default::default(),
        };

        Storage::<UtxoId, Coin>::insert(&mut self.database, &id, &coin).unwrap();

        (id, coin)
    }

    pub fn owned_coins(&self, owner: Address) -> Vec<(UtxoId, Coin)> {
        self.database
            .owned_coins(owner, None, None)
            .map(|res| {
                res.map(|id| {
                    let coin = Storage::<UtxoId, Coin>::get(&self.database, &id)
                        .unwrap()
                        .unwrap();
                    (id, coin.into_owned())
                })
            })
            .try_collect()
            .unwrap()
    }
}

impl AsRef<Database> for TestDatabase {
    fn as_ref(&self) -> &Database {
        self.database.as_ref()
    }
}
