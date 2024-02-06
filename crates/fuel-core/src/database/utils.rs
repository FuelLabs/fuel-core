use fuel_core_storage::{
    structured_storage::TableWithBlueprint,
    tables::merkle::SparseMerkleMetadata,
    Error,
    Mappable,
    StorageAsMut,
    StorageBatchMutate,
    StorageMutate,
};
use fuel_core_types::{
    fuel_merkle::sparse::{
        self,
        MerkleTree,
        MerkleTreeError,
        MerkleTreeKey,
    },
    fuel_types::ContractId,
};

use super::Database;

pub(crate) struct MerkleTreeDbUtils<'a, Metadata, Data> {
    db: &'a mut Database,
    _metadata: std::marker::PhantomData<Metadata>,
    _data: std::marker::PhantomData<Data>,
}
impl<'a, Metadata, Data> MerkleTreeDbUtils<'a, Metadata, Data>
where
    Metadata: TableWithBlueprint
        + Mappable<
            Key = ContractId,
            Value = SparseMerkleMetadata,
            OwnedValue = SparseMerkleMetadata,
        >,
    Data: TableWithBlueprint
        + Mappable<
            Key = [u8; 32],
            OwnedValue = (u32, u8, [u8; 32], [u8; 32]),
            Value = (u32, u8, [u8; 32], [u8; 32]),
        >,
    Database: StorageMutate<Data, Error = Error>
        + StorageMutate<Metadata, Error = Error>
        + StorageBatchMutate<Data>,
{
    fn new(db: &'a mut Database) -> Self {
        Self {
            db,
            _metadata: std::marker::PhantomData,
            _data: std::marker::PhantomData,
        }
    }

    pub(crate) fn update(
        db: &'a mut Database,
        contract_id: &ContractId,
        entries: impl Iterator<Item = (impl AsRef<[u8]>, impl AsRef<[u8]>)>,
    ) -> Result<(), Error> {
        Self::new(db).update_or_create(contract_id, entries)
    }

    pub(crate) fn update_or_create(
        &mut self,
        contract_id: &ContractId,
        entries: impl Iterator<Item = (impl AsRef<[u8]>, impl AsRef<[u8]>)>,
    ) -> Result<(), Error> {
        let new_root = if let Some(root) = self.current_root(contract_id)? {
            self.update_existing_merkle_tree(root, entries)
                .map_err(|e| Error::Other(anyhow::anyhow!("{e:?}")))?
        } else {
            self.create_new_merkle_tree(entries)?
        };
        self.update_cached_root(new_root, contract_id)
    }

    fn create_new_merkle_tree(
        &mut self,
        entries: impl Iterator<Item = (impl AsRef<[u8]>, impl AsRef<[u8]>)>,
    ) -> Result<[u8; 32], Error> {
        let set = entries.map(|(key, value)| (MerkleTreeKey::new(key), value));
        let (root, nodes) = sparse::in_memory::MerkleTree::nodes_from_set(set);

        // TODO dont
        let iter = nodes.iter().map(|(key, value)| (key, value));
        <Database as StorageBatchMutate<Data>>::insert_batch(self.db, iter)?;

        Ok(root)
    }

    fn update_existing_merkle_tree(
        &mut self,
        root: [u8; 32],
        entries: impl Iterator<Item = (impl AsRef<[u8]>, impl AsRef<[u8]>)>,
    ) -> Result<[u8; 32], MerkleTreeError<Error>> {
        let mut tree = MerkleTree::<Data, _>::load(&mut self.db, &root)?;
        for (key, value) in entries {
            let key = MerkleTreeKey::new(key);
            tree.update(key, value.as_ref())?;
        }

        Ok(tree.root())
    }
    fn update_cached_root(
        &mut self,
        new_root: [u8; 32],
        contract_id: &ContractId,
    ) -> Result<(), Error> {
        let metadata = SparseMerkleMetadata::new(new_root);
        self.db
            .storage::<Metadata>()
            .insert(&ContractId::from(**contract_id), &metadata)?;

        Ok(())
    }

    fn current_root(&mut self, key: &ContractId) -> Result<Option<[u8; 32]>, Error> {
        let root = self
            .db
            .storage::<Metadata>()
            .get(key)?
            .map(|c| c.root().to_owned());

        Ok(root)
    }
}
