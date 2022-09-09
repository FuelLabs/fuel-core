use crate::database::{
    metadata,
    Column,
    Database,
};
use fuel_core_interfaces::{
    common::fuel_storage::StorageAsMut,
    db::ValidatorsSet,
    model::{
        BlockHeight,
        ConsensusId,
        DaBlockHeight,
        SealedFuelBlock,
        ValidatorId,
        ValidatorStake,
    },
    relayer::{
        RelayerDb,
        StakingDiff,
    },
};
use std::{
    collections::HashMap,
    ops::DerefMut,
    sync::Arc,
};

#[async_trait::async_trait]
impl RelayerDb for Database {
    async fn get_validators(
        &self,
    ) -> HashMap<ValidatorId, (ValidatorStake, Option<ConsensusId>)> {
        struct WrapAddress(pub ValidatorId);
        impl From<Vec<u8>> for WrapAddress {
            fn from(i: Vec<u8>) -> Self {
                Self(ValidatorId::try_from(i.as_ref()).unwrap())
            }
        }
        let mut out = HashMap::new();
        for diff in self.iter_all::<WrapAddress, (ValidatorStake, Option<ConsensusId>)>(
            Column::ValidatorSet,
            None,
            None,
            None,
        ) {
            match diff {
                Ok((address, stake)) => {
                    out.insert(address.0, stake);
                }
                Err(err) => panic!("Database internal error:{:?}", err),
            }
        }
        out
    }

    async fn get_staking_diffs(
        &self,
        from_da_height: DaBlockHeight,
        to_da_height: Option<DaBlockHeight>,
    ) -> Vec<(DaBlockHeight, StakingDiff)> {
        let to_da_height = if let Some(to_da_height) = to_da_height {
            if from_da_height > to_da_height {
                return Vec::new()
            }
            to_da_height
        } else {
            DaBlockHeight::MAX
        };
        struct WrapU64Be(pub DaBlockHeight);
        impl From<Vec<u8>> for WrapU64Be {
            fn from(i: Vec<u8>) -> Self {
                use byteorder::{
                    BigEndian,
                    ReadBytesExt,
                };
                use std::io::Cursor;
                let mut i = Cursor::new(i);
                Self(i.read_u64::<BigEndian>().unwrap_or_default())
            }
        }
        let mut out = Vec::new();
        for diff in self.iter_all::<WrapU64Be, StakingDiff>(
            Column::StackingDiffs,
            None,
            Some(from_da_height.to_be_bytes().to_vec()),
            None,
        ) {
            match diff {
                Ok((key, diff)) => {
                    let block = key.0;
                    if block > to_da_height {
                        return out
                    }
                    out.push((block, diff))
                }
                Err(err) => panic!("get_validator_diffs unexpected error:{:?}", err),
            }
        }
        out
    }

    async fn apply_validator_diffs(
        &mut self,
        da_height: DaBlockHeight,
        changes: &HashMap<ValidatorId, (ValidatorStake, Option<ConsensusId>)>,
    ) {
        // this is reimplemented here to assure it is atomic operation in case of poweroff situation.
        let mut db = self.transaction();
        // TODO
        for (address, stake) in changes {
            let _ = db
                .deref_mut()
                .storage::<ValidatorsSet>()
                .insert(address, stake);
        }
        db.set_validators_da_height(da_height).await;
        if let Err(err) = db.commit() {
            panic!("apply_validator_diffs database corrupted: {:?}", err);
        }
    }

    async fn get_chain_height(&self) -> BlockHeight {
        match self.get_block_height() {
            Ok(res) => {
                res.expect("get_block_height value should be always present and set")
            }
            Err(err) => {
                panic!("get_block_height database corruption, err:{:?}", err);
            }
        }
    }

    async fn get_sealed_block(
        &self,
        _height: BlockHeight,
    ) -> Option<Arc<SealedFuelBlock>> {
        // TODO
        Some(Arc::new(SealedFuelBlock::default()))
    }

    async fn set_finalized_da_height(&self, block: DaBlockHeight) {
        let _: Option<BlockHeight> = self
            .insert(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata, block)
            .unwrap_or_else(|err| {
                panic!("set_finalized_da_height should always succeed: {:?}", err);
            });
    }

    async fn get_finalized_da_height(&self) -> DaBlockHeight {
        match self.get(metadata::FINALIZED_DA_HEIGHT_KEY, Column::Metadata) {
            Ok(res) => {
                return res.expect(
                    "get_finalized_da_height value should be always present and set",
                )
            }
            Err(err) => {
                panic!("get_finalized_da_height database corruption, err:{:?}", err);
            }
        }
    }

    async fn set_validators_da_height(&self, block: DaBlockHeight) {
        let _: Option<BlockHeight> = self
            .insert(metadata::VALIDATORS_DA_HEIGHT_KEY, Column::Metadata, block)
            .unwrap_or_else(|err| {
                panic!("set_validators_da_height should always succeed: {:?}", err);
            });
    }

    async fn get_validators_da_height(&self) -> DaBlockHeight {
        match self.get(metadata::VALIDATORS_DA_HEIGHT_KEY, Column::Metadata) {
            Ok(res) => {
                return res.expect(
                    "get_validators_da_height value should be always present and set",
                )
            }
            Err(err) => {
                panic!(
                    "get_validators_da_height database corruption, err:{:?}",
                    err
                );
            }
        }
    }

    async fn get_last_committed_finalized_fuel_height(&self) -> BlockHeight {
        match self.get(
            metadata::LAST_COMMITTED_FINALIZED_BLOCK_HEIGHT_KEY,
            Column::Metadata,
        ) {
            Ok(res) => {
                return res
                    .expect("set_last_committed_finalized_fuel_height value should be always present and set");
            }
            Err(err) => {
                panic!(
                    "set_last_committed_finalized_fuel_height database corruption, err:{:?}",
                    err
                );
            }
        }
    }

    async fn set_last_committed_finalized_fuel_height(&self, block_height: BlockHeight) {
        let _: Option<BlockHeight> = self
            .insert(
                metadata::LAST_COMMITTED_FINALIZED_BLOCK_HEIGHT_KEY,
                Column::Metadata,
                block_height,
            )
            .unwrap_or_else(|err| {
                panic!(
                    "set_last_committed_finalized_fuel_height should always succeed: {:?}",
                    err
                );
            });
    }
}
