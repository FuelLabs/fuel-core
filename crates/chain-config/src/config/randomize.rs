pub trait Randomize {
    fn randomize(rng: impl rand::Rng) -> Self;
}

use fuel_core_storage::{
    tables::{
        Coins,
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        Messages,
    },
    ContractsAssetKey,
    ContractsStateData,
    ContractsStateKey,
};
use fuel_core_types::{
    blockchain::primitives::DaBlockHeight,
    entities::{
        coins::coin::{
            CompressedCoin,
            CompressedCoinV1,
        },
        contract::{
            ContractUtxoInfo,
            ContractsInfoType,
        },
        relayer::message::MessageV1,
        Message,
    },
    fuel_tx::{
        ContractId,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
        BlockHeight,
        Bytes32,
        Nonce,
    },
    fuel_vm::Salt,
};

use crate::TableEntry;

macro_rules! delegating_table_impl {
    ($($table: ty),*) => {
        $(
            impl $crate::Randomize for $crate::TableEntry<$table> {
                fn randomize(mut rng: impl ::rand::Rng) -> Self {
                    Self {
                        key: $crate::Randomize::randomize(&mut rng),
                        value: $crate::Randomize::randomize(&mut rng),
                    }
                }
            }
        )*
    };
}

macro_rules! delegating_impl {
    ($($t: ty),*) => {
        $(
            impl $crate::Randomize for $t {
                fn randomize(mut rng: impl ::rand::Rng) -> Self {
                    rng.gen()
                }
            }
        )*
    };
}

delegating_table_impl!(
    Coins,
    ContractsRawCode,
    ContractsState,
    ContractsAssets,
    ContractsLatestUtxo
);

// Messages are special, because they have the key inside of the value as well
impl Randomize for TableEntry<Messages> {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let value = Message::randomize(&mut rng);
        Self {
            key: *value.nonce(),
            value,
        }
    }
}

delegating_impl!(
    DaBlockHeight,
    BlockHeight,
    Address,
    Nonce,
    ContractId,
    Salt,
    Bytes32,
    UtxoId,
    AssetId,
    u64
);

impl Randomize for CompressedCoin {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        Self::V1(CompressedCoinV1 {
            owner: rng.gen(),
            amount: rng.gen(),
            asset_id: rng.gen(),
            tx_pointer: rng.gen(),
        })
    }
}

impl Randomize for Message {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        Self::V1(MessageV1 {
            sender: rng.gen(),
            recipient: rng.gen(),
            nonce: rng.gen(),
            amount: rng.gen(),
            data: rng.gen::<[u8; 32]>().to_vec(),
            da_height: rng.gen(),
        })
    }
}

impl Randomize for fuel_core_types::fuel_vm::Contract {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        rng.gen::<[u8; 32]>().to_vec().into()
    }
}

impl Randomize for ContractsInfoType {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let salt: Salt = Randomize::randomize(&mut rng);
        Self::V1(salt.into())
    }
}

impl Randomize for ContractUtxoInfo {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        ContractUtxoInfo::V1(fuel_core_types::entities::contract::ContractUtxoInfoV1 {
            utxo_id: rng.gen(),
            tx_pointer: rng.gen(),
        })
    }
}

impl Randomize for ContractsStateData {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        rng.gen::<[u8; 32]>().to_vec().into()
    }
}

impl Randomize for ContractsAssetKey {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let contract_id: ContractId = rng.gen();
        let asset_id: fuel_core_types::fuel_types::AssetId = rng.gen();
        Self::new(&contract_id, &asset_id)
    }
}

impl Randomize for ContractsStateKey {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let contract_id: ContractId = rng.gen();
        let state_key: Bytes32 = rng.gen();
        Self::new(&contract_id, &state_key)
    }
}
