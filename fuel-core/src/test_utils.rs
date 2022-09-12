use crate::{
    database::Database,
    model::{
        Coin,
        CoinStatus,
    },
};
use fuel_core_interfaces::{
    common::{
        fuel_asm::Word,
        fuel_storage::Storage,
        fuel_tx::{
            Address,
            AssetId,
            Bytes32,
            UtxoId,
        },
        fuel_types::MessageId,
    },
    model::{
        BlockHeight,
        DaBlockHeight,
        Message,
    },
};
use itertools::Itertools;

#[derive(Default)]
pub struct TestDatabase {
    database: Database,
    last_coin_index: u64,
    last_message_index: u64,
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

    pub fn make_message(&mut self, owner: Address, amount: Word) -> (MessageId, Message) {
        let nonce = self.last_message_index;
        self.last_message_index += 1;

        let message = Message {
            sender: Default::default(),
            recipient: owner,
            nonce,
            amount,
            data: vec![],
            da_height: DaBlockHeight::from(BlockHeight::from(1u64)),
            fuel_block_spend: None,
        };

        Storage::<MessageId, Message>::insert(
            &mut self.database,
            &message.id(),
            &message,
        )
        .unwrap();

        (message.id(), message)
    }

    pub fn owned_coins(&self, owner: &Address) -> Vec<(UtxoId, Coin)> {
        self.database
            .owned_coins_utxos(owner, None, None)
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

    pub fn owned_messages(&self, owner: &Address) -> Vec<Message> {
        self.database
            .owned_message_ids(owner, None, None)
            .map(|res| {
                res.map(|id| {
                    let message = Storage::<MessageId, Message>::get(&self.database, &id)
                        .unwrap()
                        .unwrap();
                    message.into_owned()
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
