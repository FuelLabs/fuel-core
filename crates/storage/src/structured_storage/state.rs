//! The module contains implementations and tests for the `ContractsState` table.

use crate::{
    blueprint::{
        avl_merkle,
        avl_merkle::{
            FromPrefix,
            PrefixedKey,
        },
        btree_merkle,
        sparse::{
            PrimaryKey,
            Sparse,
        },
    },
    codec::raw::Raw,
    column::Column,
    storage_interlayer::Interlayer,
    structured_storage::TableWithBlueprint,
    tables::{
        merkle::{
            ContractsStateMerkleData,
            ContractsStateMerkleMetadata,
        },
        ContractsState,
    },
    Mappable,
};
use fuel_core_types::fuel_merkle::common::Bytes32;
use fuel_vm_private::storage::ContractsStateKey;

/// The key convertor used to convert the key from the `ContractsState` table
/// to the key of the `ContractsStateMerkleMetadata` table.
pub struct KeyConverter;

impl PrimaryKey for KeyConverter {
    type InputKey = <ContractsState as Mappable>::Key;
    type OutputKey = <ContractsStateMerkleMetadata as Mappable>::Key;

    fn primary_key(key: &Self::InputKey) -> &Self::OutputKey {
        key.contract_id()
    }
}

impl avl_merkle::PrefixedKey for ContractsStateKey {
    fn prefix(&self) -> (Bytes32, Bytes32) {
        let contract_id = *self.contract_id();
        let state_key = *self.state_key();
        (contract_id.into(), state_key.into())
    }
}

impl avl_merkle::FromPrefix for ContractsStateKey {
    fn from_prefix(prefix: Bytes32, unique: Bytes32) -> Self {
        let contract_id = prefix.into();
        let state_key = unique.into();
        Self::new(&contract_id, &state_key)
    }
}

impl btree_merkle::PrefixedKey for ContractsStateKey {
    fn prefix(&self) -> (Bytes32, Bytes32) {
        let contract_id = *self.contract_id();
        let state_key = *self.state_key();
        (contract_id.into(), state_key.into())
    }
}

impl btree_merkle::FromPrefix for ContractsStateKey {
    fn from_prefix(prefix: Bytes32, unique: Bytes32) -> Self {
        let contract_id = prefix.into();
        let state_key = unique.into();
        Self::new(&contract_id, &state_key)
    }
}

impl TableWithBlueprint for ContractsState {
    type Blueprint = btree_merkle::BTreeMerkle<
        ContractsStateMerkleMetadata,
        ContractsStateMerkleData,
        Raw,
    >;
}

impl Interlayer for ContractsState {
    type KeyCodec = Raw;
    type ValueCodec = Raw;
    type Column = Column;

    fn column() -> Column {
        Column::ContractsState
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Mappable;

    fn generate_key(
        primary_key: &<ContractsStateMerkleMetadata as Mappable>::Key,
        rng: &mut impl rand::Rng,
    ) -> <ContractsState as Mappable>::Key {
        let mut bytes = [0u8; 32];
        rng.fill(bytes.as_mut());
        <ContractsState as Mappable>::Key::new(primary_key, &bytes.into())
    }

    crate::basic_storage_tests!(
        ContractsState,
        <ContractsState as Mappable>::Key::default(),
        [0u8; 32],
        vec![0u8; 32].into()
    );

    fn generate_value(rng: &mut impl rand::Rng) -> Vec<u8> {
        let mut bytes = [0u8; 32];
        rng.fill(bytes.as_mut());
        bytes.to_vec()
    }

    crate::root_storage_tests!(
        ContractsState,
        ContractsStateMerkleMetadata,
        <ContractsStateMerkleMetadata as Mappable>::Key::from([1u8; 32]),
        <ContractsStateMerkleMetadata as Mappable>::Key::from([2u8; 32]),
        generate_key,
        generate_value
    );
}

#[cfg(test)]
#[allow(non_snake_case)]
mod structured_storage_tests {
    use crate::{
        column::Column,
        structured_storage::test::InMemoryStorage,
        transactional::ReadTransaction,
        StorageAsMut,
        StorageMutate,
        StorageWrite,
    };
    use fuel_vm_private::{
        prelude::{
            Bytes32,
            ContractId,
        },
        storage::{
            ContractsState,
            ContractsStateKey,
        },
    };
    use rand::{
        prelude::StdRng,
        Rng,
        SeedableRng,
    };

    #[test]
    fn storage_write__write__generates_the_same_merkle_root_as_storage_insert() {
        type Storage = InMemoryStorage<Column>;

        let mut rng = StdRng::seed_from_u64(1234);

        // Given
        let contract_id = ContractId::default();
        let keys = std::iter::from_fn(|| Some(rng.gen::<Bytes32>()))
            .take(10)
            .map(|state_key| ContractsStateKey::from((&contract_id, &state_key)))
            .collect::<Vec<_>>();
        let value = vec![0u8; 32];

        // When
        let merkle_root_write = {
            let storage = Storage::default();
            let mut structure = storage.read_transaction();
            let mut merkle_root = structure
                .storage::<ContractsState>()
                .root(&contract_id)
                .expect("Unable to retrieve Merkle root");
            for key in keys.iter() {
                <_ as StorageWrite<ContractsState>>::write_bytes(
                    &mut structure,
                    key,
                    &value,
                )
                .expect("Unable to write storage");
                let new_merkle_root = structure
                    .storage::<ContractsState>()
                    .root(&contract_id)
                    .expect("Unable to retrieve Merkle root");
                assert_ne!(merkle_root, new_merkle_root);
                merkle_root = new_merkle_root;
            }

            structure
                .storage::<ContractsState>()
                .root(&contract_id)
                .expect("Unable to retrieve Merkle root")
        };

        // Then
        let merkle_root_insert = {
            let storage = Storage::default();
            let mut structure = storage.read_transaction();
            for key in keys.iter() {
                <_ as StorageMutate<ContractsState>>::insert(&mut structure, key, &value)
                    .expect("Unable to write storage");
            }

            structure
                .storage::<ContractsState>()
                .root(&contract_id)
                .expect("Unable to retrieve Merkle root")
        };

        assert_eq!(merkle_root_write, merkle_root_insert);
    }

    #[test]
    fn storage_write__replace__generates_the_same_merkle_root_as_storage_insert() {
        type Storage = InMemoryStorage<Column>;

        let mut rng = StdRng::seed_from_u64(1234);

        // Given
        let contract_id = ContractId::default();
        let keys = std::iter::from_fn(|| Some(rng.gen::<Bytes32>()))
            .take(10)
            .map(|state_key| ContractsStateKey::from((&contract_id, &state_key)))
            .collect::<Vec<_>>();
        let value = vec![0u8; 32];

        // When
        let merkle_root_replace = {
            let storage = Storage::default();
            let mut structure = storage.read_transaction();
            let mut merkle_root = structure
                .storage::<ContractsState>()
                .root(&contract_id)
                .expect("Unable to retrieve Merkle root");
            for key in keys.iter() {
                <_ as StorageWrite<ContractsState>>::replace_bytes(
                    &mut structure,
                    key,
                    &value,
                )
                .expect("Unable to write storage");
                let new_merkle_root = structure
                    .storage::<ContractsState>()
                    .root(&contract_id)
                    .expect("Unable to retrieve Merkle root");
                assert_ne!(merkle_root, new_merkle_root);
                merkle_root = new_merkle_root;
            }

            structure
                .storage::<ContractsState>()
                .root(&contract_id)
                .expect("Unable to retrieve Merkle root")
        };

        // Then
        let merkle_root_insert = {
            let storage = Storage::default();
            let mut structure = storage.read_transaction();
            for key in keys.iter() {
                <_ as StorageMutate<ContractsState>>::insert(&mut structure, key, &value)
                    .expect("Unable to write storage");
            }

            structure
                .storage::<ContractsState>()
                .root(&contract_id)
                .expect("Unable to retrieve Merkle root")
        };

        assert_eq!(merkle_root_replace, merkle_root_insert);
    }

    #[test]
    fn storage_write__take__generates_the_same_merkle_root_as_storage_remove() {
        type Storage = InMemoryStorage<Column>;

        let mut rng = StdRng::seed_from_u64(1234);

        // Given
        let contract_id = ContractId::default();
        let keys = std::iter::from_fn(|| Some(rng.gen::<Bytes32>()))
            .take(10)
            .map(|state_key| ContractsStateKey::from((&contract_id, &state_key)))
            .collect::<Vec<_>>();
        let value = vec![0u8; 32];

        let storage = Storage::default();
        let mut structure = storage.read_transaction();
        let mut merkle_root = structure
            .storage::<ContractsState>()
            .root(&contract_id)
            .expect("Unable to retrieve Merkle root");
        for key in keys.iter() {
            <_ as StorageWrite<ContractsState>>::replace_bytes(
                &mut structure,
                key,
                &value,
            )
            .expect("Unable to write storage");

            let new_merkle_root = structure
                .storage::<ContractsState>()
                .root(&contract_id)
                .expect("Unable to retrieve Merkle root");
            assert_ne!(merkle_root, new_merkle_root);
            merkle_root = new_merkle_root;
        }

        // When
        let state_key = rng.gen::<Bytes32>();
        let key = ContractsStateKey::from((&contract_id, &state_key));

        let merkle_root_replace = {
            <_ as StorageWrite<ContractsState>>::write_bytes(
                &mut structure,
                &key,
                &value,
            )
            .expect("Unable to write storage");

            <_ as StorageWrite<ContractsState>>::take_bytes(&mut structure, &key)
                .expect("Unable to take value from storage");

            structure
                .storage::<ContractsState>()
                .root(&contract_id)
                .expect("Unable to retrieve Merkle root")
        };

        // Then
        let merkle_root_remove = {
            <_ as StorageWrite<ContractsState>>::write_bytes(
                &mut structure,
                &key,
                &value,
            )
            .expect("Unable to write storage");

            structure
                .storage::<ContractsState>()
                .remove(&key)
                .expect("Unable to take value from storage");

            structure
                .storage::<ContractsState>()
                .root(&contract_id)
                .expect("Unable to retrieve Merkle root")
        };

        assert_eq!(merkle_root_replace, merkle_root_remove);
    }
}
