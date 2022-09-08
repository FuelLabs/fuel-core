use crate::{
    database::{
        columns::CONTRACTS_STATE,
        Database,
    },
    state::{
        Error,
        IterDirection,
        MultiKey,
    },
};
use fuel_core_interfaces::common::fuel_vm::{
    crypto,
    prelude::{
        Bytes32,
        ContractId,
        MerkleRoot,
        MerkleStorage,
    },
};
use itertools::Itertools;
use std::borrow::Cow;

impl MerkleStorage<ContractId, Bytes32, Bytes32> for Database {
    type Error = Error;

    fn insert(
        &mut self,
        parent: &ContractId,
        key: &Bytes32,
        value: &Bytes32,
    ) -> Result<Option<Bytes32>, Error> {
        let key = MultiKey::new((parent, key));
        Database::insert(self, key.as_ref().to_vec(), CONTRACTS_STATE, *value)
            .map_err(Into::into)
    }

    fn remove(
        &mut self,
        parent: &ContractId,
        key: &Bytes32,
    ) -> Result<Option<Bytes32>, Error> {
        let key = MultiKey::new((parent, key));
        Database::remove(self, key.as_ref(), CONTRACTS_STATE).map_err(Into::into)
    }

    fn get(
        &self,
        parent: &ContractId,
        key: &Bytes32,
    ) -> Result<Option<Cow<Bytes32>>, Error> {
        let key = MultiKey::new((parent, key));
        self.get(key.as_ref(), CONTRACTS_STATE).map_err(Into::into)
    }

    fn contains_key(&self, parent: &ContractId, key: &Bytes32) -> Result<bool, Error> {
        let key = MultiKey::new((parent, key));
        self.exists(key.as_ref(), CONTRACTS_STATE)
            .map_err(Into::into)
    }

    fn root(&mut self, parent: &ContractId) -> Result<MerkleRoot, Error> {
        let items: Vec<_> = Database::iter_all::<Vec<u8>, Bytes32>(
            self,
            CONTRACTS_STATE,
            Some(parent.as_ref().to_vec()),
            None,
            Some(IterDirection::Forward),
        )
        .try_collect()?;

        let root = items
            .iter()
            .filter_map(|(key, value)| {
                (&key[..parent.len()] == parent.as_ref()).then(|| (key, value))
            })
            .sorted_by_key(|t| t.0)
            .map(|(_, value)| value);

        Ok(crypto::ephemeral_merkle_root(root).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let database = Database::default();
        database
            .insert(MultiKey::new(storage_id), CONTRACTS_STATE, stored_value)
            .unwrap();

        assert_eq!(
            MerkleStorage::<ContractId, Bytes32, Bytes32>::get(
                &database,
                &storage_id.0,
                &storage_id.1
            )
            .unwrap()
            .unwrap()
            .into_owned(),
            stored_value
        );
    }

    #[test]
    fn put() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let mut database = Database::default();
        MerkleStorage::<ContractId, Bytes32, Bytes32>::insert(
            &mut database,
            &storage_id.0,
            &storage_id.1,
            &stored_value,
        )
        .unwrap();

        let returned: Bytes32 = database
            .get(MultiKey::new(storage_id).as_ref(), CONTRACTS_STATE)
            .unwrap()
            .unwrap();
        assert_eq!(returned, stored_value);
    }

    #[test]
    fn remove() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let mut database = Database::default();
        database
            .insert(MultiKey::new(storage_id), CONTRACTS_STATE, stored_value)
            .unwrap();

        MerkleStorage::<ContractId, Bytes32, Bytes32>::remove(
            &mut database,
            &storage_id.0,
            &storage_id.1,
        )
        .unwrap();

        assert!(!database
            .exists(MultiKey::new(storage_id).as_ref(), CONTRACTS_STATE)
            .unwrap());
    }

    #[test]
    fn exists() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let database = Database::default();
        database
            .insert(MultiKey::new(storage_id), CONTRACTS_STATE, stored_value)
            .unwrap();

        assert!(MerkleStorage::<ContractId, Bytes32, Bytes32>::contains_key(
            &database,
            &storage_id.0,
            &storage_id.1
        )
        .unwrap());
    }

    #[test]
    fn root() {
        let storage_id: (ContractId, Bytes32) =
            (ContractId::from([1u8; 32]), Bytes32::from([1u8; 32]));
        let stored_value: Bytes32 = Bytes32::from([2u8; 32]);

        let mut database = Database::default();

        MerkleStorage::<ContractId, Bytes32, Bytes32>::insert(
            &mut database,
            &storage_id.0,
            &storage_id.1,
            &stored_value,
        )
        .unwrap();

        let root = MerkleStorage::<ContractId, Bytes32, Bytes32>::root(
            &mut database,
            &storage_id.0,
        );
        assert!(root.is_ok())
    }
}
