pub trait Randomize {
    fn randomize(rng: impl rand::Rng) -> Self;
}

use fuel_core_storage::{
    ContractsAssetKey,
    ContractsStateData,
    ContractsStateKey,
    tables::{
        Coins,
        ContractsAssets,
        ContractsLatestUtxo,
        ContractsRawCode,
        ContractsState,
        Messages,
    },
};
use fuel_core_types::{
    blockchain::{
        block::CompressedBlock,
        consensus::{
            Consensus,
            poa::PoAConsensus,
        },
        header::PartialBlockHeader,
        primitives::DaBlockHeight,
    },
    entities::{
        Message,
        coins::coin::{
            CompressedCoin,
            CompressedCoinV1,
        },
        contract::{
            ContractUtxoInfo,
            ContractsInfoType,
        },
        relayer::message::MessageV1,
    },
    fuel_asm::{
        RegId,
        op,
    },
    fuel_tx::{
        BlobId,
        ContractId,
        Transaction,
        TransactionBuilder,
        UniqueIdentifier,
        UtxoId,
    },
    fuel_types::{
        Address,
        AssetId,
        BlockHeight,
        Bytes32,
        ChainId,
        Nonce,
    },
    fuel_vm::{
        BlobBytes,
        BlobData,
        Salt,
        Signature,
    },
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
                    rng.r#gen()
                }
            }
        )*
    };
}

delegating_table_impl!(
    Coins,
    BlobData,
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
    BlobBytes,
    BlobId,
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
            owner: rng.r#gen(),
            amount: rng.r#gen(),
            asset_id: rng.r#gen(),
            tx_pointer: rng.r#gen(),
        })
    }
}

impl Randomize for Message {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        Self::V1(MessageV1 {
            sender: rng.r#gen(),
            recipient: rng.r#gen(),
            nonce: rng.r#gen(),
            amount: rng.r#gen(),
            data: rng.r#gen::<[u8; 32]>().to_vec(),
            da_height: rng.r#gen(),
        })
    }
}

impl Randomize for fuel_core_types::fuel_vm::Contract {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        rng.r#gen::<[u8; 32]>().to_vec().into()
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
            utxo_id: rng.r#gen(),
            tx_pointer: rng.r#gen(),
        })
    }
}

impl Randomize for ContractsStateData {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        rng.r#gen::<[u8; 32]>().to_vec().into()
    }
}

impl Randomize for ContractsAssetKey {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let contract_id: ContractId = rng.r#gen();
        let asset_id: fuel_core_types::fuel_types::AssetId = rng.r#gen();
        Self::new(&contract_id, &asset_id)
    }
}

impl Randomize for ContractsStateKey {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let contract_id: ContractId = rng.r#gen();
        let state_key: Bytes32 = rng.r#gen();
        Self::new(&contract_id, &state_key)
    }
}

impl Randomize for CompressedBlock {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let tx1: Transaction = Randomize::randomize(&mut rng);
        let tx2: Transaction = Randomize::randomize(&mut rng);
        let default_chain_id = ChainId::default();

        let tx_ids = vec![tx1.id(&default_chain_id), tx2.id(&default_chain_id)];

        Self::test(
            PartialBlockHeader::default()
                .generate(
                    &[tx1, tx2],
                    &[],
                    rng.r#gen(),
                    #[cfg(feature = "fault-proving")]
                    &default_chain_id,
                )
                .expect("The header is valid"),
            tx_ids,
        )
    }
}

impl Randomize for (BlockHeight, Consensus) {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        (
            rng.r#gen::<u32>().into(),
            Consensus::PoA(PoAConsensus::new(Signature::from_bytes([rng.r#gen(); 64]))),
        )
    }
}

impl Randomize for Transaction {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let program: Vec<u8> = [op::ret(RegId::ONE)].into_iter().collect();
        match rng.gen_range(0..2) {
            0 => {
                TransactionBuilder::create(program.into(), rng.r#gen(), vec![rng.r#gen()])
                    .finalize_as_transaction()
            }
            1 => TransactionBuilder::script(program, vec![rng.r#gen(), rng.r#gen()])
                .finalize_as_transaction(),
            _ => unreachable!(),
        }
    }
}
