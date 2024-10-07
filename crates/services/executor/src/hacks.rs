use fuel_core_storage::{
    column::Column,
    kv_store::KeyValueInspect,
    tables::ContractsState,
    transactional::StorageTransaction,
    ContractsStateKey,
    StorageAsMut,
};
use fuel_core_types::{
    fuel_tx::Input,
    fuel_types::{
        Bytes32,
        ContractId,
    },
    services::executor::Result as ExecutorResult,
};

struct Slot {
    key: &'static str,
    value: &'static str,
}

const CONTRACT_ID_WITH_BAD_STATE: &str =
    "0x7e2becd64cd598da59b4d1064b711661898656c6b1f4918a787156b8965dc83c";

const STATE: [Slot; 4] = [
    Slot {
        key: "35fa5b7532d53cf687e13e3db014eaf208c5b8c534ab693dd7090d5e02675f3e",
        value: "0000000000000000000000000000000000000000000000000000000000000000",
    },
    Slot {
        key: "35fa5b7532d53cf687e13e3db014eaf208c5b8c534ab693dd7090d5e02675f3f",
        value: "0000000000000000000000000000000000000000000000000000000000000000",
    },
    Slot {
        key: "7bb458adc1d118713319a5baa00a2d049dd64d2916477d2688d76970c898cd55",
        value: "0000000000000000000000000000000000000000000000000000000000000000",
    },
    Slot {
        key: "7bb458adc1d118713319a5baa00a2d049dd64d2916477d2688d76970c898cd56",
        value: "0000000000000000000000000000000000000000000000000000000000000000",
    },
];

pub fn maybe_fix_storage<S>(
    inputs: &[Input],
    storage: &mut StorageTransaction<S>,
) -> ExecutorResult<()>
where
    S: KeyValueInspect<Column = Column>,
{
    for input in inputs {
        if let Input::Contract(contract) = input {
            maybe_fix_contract(&contract.contract_id, storage)?;
        }
    }
    Ok(())
}

pub(crate) fn maybe_fix_contract<S>(
    contract_id: &ContractId,
    storage: &mut StorageTransaction<S>,
) -> ExecutorResult<()>
where
    S: KeyValueInspect<Column = Column>,
{
    let bad_contract_id: ContractId = CONTRACT_ID_WITH_BAD_STATE.parse().unwrap();

    if contract_id == &bad_contract_id {
        for slot in STATE.iter() {
            let storage_key: Bytes32 = slot.key.parse().unwrap();

            let key = ContractsStateKey::new(contract_id, &storage_key);
            let contains = storage
                .storage_as_mut::<ContractsState>()
                .contains_key(&key)?;

            if contains {
                continue;
            }

            let value: Bytes32 = slot.value.parse().unwrap();

            storage
                .storage_as_mut::<ContractsState>()
                .insert(&key, value.as_ref())?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use fuel_core_storage::{
        iter::{
            changes_iterator::ChangesIterator,
            IteratorOverTable,
        },
        structured_storage::test::InMemoryStorage,
        transactional::IntoTransaction,
    };

    #[test]
    fn dummy() {
        // Given
        let contract_id: ContractId =
            "0x7e2becd64cd598da59b4d1064b711661898656c6b1f4918a787156b8965dc83c"
                .parse()
                .unwrap();
        let input = Input::contract(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            contract_id,
        );
        let mut storage = InMemoryStorage::default().into_transaction();

        // When
        maybe_fix_storage(&[input], &mut storage).unwrap();

        // Then
        let changes = storage.into_changes();
        let view = ChangesIterator::new(&changes);
        let entries = view
            .iter_all::<ContractsState>(None)
            .map(|r| r.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), STATE.len());
    }
}
