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
    blockchain::{
        block::CompressedBlock,
        consensus::{
            poa::PoAConsensus,
            Consensus,
        },
        header::PartialBlockHeader,
        primitives::DaBlockHeight,
    },
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
    fuel_asm::{
        op,
        RegId,
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
                    rng.gen()
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

impl Randomize for CompressedBlock {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let tx1: Transaction = Randomize::randomize(&mut rng);
        let tx2: Transaction = Randomize::randomize(&mut rng);

        let tx_ids = vec![tx1.id(&ChainId::default()), tx2.id(&ChainId::default())];

        Self::test(
            PartialBlockHeader::default()
                .generate(&[tx1, tx2], &[], rng.gen())
                .expect("The header is valid"),
            tx_ids,
        )
    }
}

impl Randomize for (BlockHeight, Consensus) {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        (
            rng.gen::<u32>().into(),
            Consensus::PoA(PoAConsensus::new(Signature::from_bytes([rng.gen(); 64]))),
        )
    }
}

impl Randomize for Transaction {
    fn randomize(mut rng: impl rand::Rng) -> Self {
        let program: Vec<u8> = [op::ret(RegId::ONE)].into_iter().collect();
        match rng.gen_range(0..2) {
            0 => TransactionBuilder::create(program.into(), rng.gen(), vec![rng.gen()])
                .finalize_as_transaction(),
            1 => TransactionBuilder::script(program, vec![rng.gen(), rng.gen()])
                .finalize_as_transaction(),
            _ => unreachable!(),
        }
    }
}
