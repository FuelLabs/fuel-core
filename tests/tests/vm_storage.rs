#[cfg(test)]
mod tests {
    use fuel_core::database::Database;

    use fuel_core_storage::{
        InterpreterStorage,
        StorageAsMut,
        StorageMutate,
        tables::ContractsState,
        vm_storage::VmStorage,
    };
    use fuel_core_types::{
        fuel_tx::ContractId,
        fuel_types::Bytes32,
        fuel_vm::ContractsStateKey,
    };
    use primitive_types::U256;
    use std::borrow::Cow;
    use test_case::test_case;

    fn u256_to_bytes32(u: U256) -> Bytes32 {
        let mut bytes = [0u8; 32];
        u.to_big_endian(&mut bytes);
        Bytes32::from(bytes)
    }

    const fn key(k: u8) -> [u8; 32] {
        [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, k,
        ]
    }

    #[test_case(
    &[], [0; 32], 1
    ; "remove single value over uninitialized range"
    )]
    #[test_case(
    &[([0; 32], vec![0; 32])], [0; 32], 1
    ; "remove single value over initialized range"
    )]
    #[test_case(
    &[], [0; 32], 2
    ; "remove multiple slots over uninitialized range"
    )]
    #[test_case(
    &[([0; 32], vec![0; 32]), (key(1), vec![0; 32])], [0; 32], 2
    ; "remove multiple slots over initialized range"
    )]
    #[test_case(
    &[(key(1), vec![0; 32]), (key(2), vec![0; 32])], [0; 32], 3
    ; "remove multiple slots over partially uninitialized start range"
    )]
    #[test_case(
    &[([0; 32], vec![0; 32]), (key(1), vec![0; 32])], [0; 32], 3
    ; "remove multiple slots over partially uninitialized end range"
    )]
    #[test_case(
    &[([0; 32], vec![0; 32]), (key(2), vec![0; 32])], [0; 32], 3
    ; "remove multiple slots over partially uninitialized middle range"
    )]
    #[test_case(
    &[(*u256_to_bytes32(U256::MAX), vec![0; 32])], *u256_to_bytes32(U256::MAX), 1
    ; "try to modify only the last value of storage"
    )]
    fn remove_range(
        prefilled_slots: &[([u8; 32], Vec<u8>)],
        start_key: [u8; 32],
        remove_count: usize,
    ) {
        let mut db = VmStorage::<Database>::default();

        let contract_id = ContractId::new([0u8; 32]);

        // prefill db
        for (key, value) in prefilled_slots {
            let key = Bytes32::from(*key);

            StorageMutate::<ContractsState>::insert(
                db.database_mut(),
                &(&contract_id, &key).into(),
                value.as_slice(),
            )
            .unwrap();
        }

        // test remove range
        db.contract_state_remove_range(
            &contract_id,
            &Bytes32::new(start_key),
            remove_count,
        )
        .unwrap();

        // check stored data
        for i in 0..remove_count {
            let (current_key, overflow) =
                U256::from_big_endian(&start_key).overflowing_add(i.into());

            if overflow {
                return;
            }

            let current_key = u256_to_bytes32(current_key);
            let state_key = ContractsStateKey::new(&contract_id, &current_key);
            let result = db
                .storage::<ContractsState>()
                .get(&state_key)
                .unwrap()
                .map(Cow::into_owned)
                .map(|b| b.0);
            if let Some(r) = result {
                assert!(r.is_empty());
            }
        }
    }
}
