#[cfg(test)]
mod tests {
    use fuel_core::database::Database;

    use fuel_core_storage::{
        tables::ContractsState,
        vm_storage::VmStorage,
        InterpreterStorage,
        StorageMutate,
    };
    use fuel_core_txpool::types::ContractId;
    use fuel_core_types::fuel_types::Bytes32;
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
    &[], key(0), 1
    => Ok(vec![None])
    ; "read single uninitialized value"
    )]
    #[test_case(
    &[(key(0), [0; 32])], key(0), 1
    => Ok(vec![Some([0; 32])])
    ; "read single initialized value"
    )]
    #[test_case(
    &[], key(0), 3
    => Ok(vec![None, None, None])
    ; "read uninitialized range"
    )]
    #[test_case(
    &[(key(1), [1; 32]), (key(2), [2; 32])], key(0), 3
    => Ok(vec![None, Some([1; 32]), Some([2; 32])])
    ; "read uninitialized start range"
    )]
    #[test_case(
    &[(key(0), [0; 32]), (key(2), [2; 32])], key(0), 3
    => Ok(vec![Some([0; 32]), None, Some([2; 32])])
    ; "read uninitialized middle range"
    )]
    #[test_case(
    &[(key(0), [0; 32]), (key(1), [1; 32])], key(0), 3
    => Ok(vec![Some([0; 32]), Some([1; 32]), None])
    ; "read uninitialized end range"
    )]
    #[test_case(
    &[(key(0), [0; 32]), (key(1), [1; 32]), (key(2), [2; 32])], key(0), 3
    => Ok(vec![Some([0; 32]), Some([1; 32]), Some([2; 32])])
    ; "read fully initialized range"
    )]
    #[test_case(
    &[(key(0), [0; 32]), (key(1), [1; 32]), (key(2), [2; 32])], key(0), 2
    => Ok(vec![Some([0; 32]), Some([1; 32])])
    ; "read subset of initialized range"
    )]
    #[test_case(
    &[(key(0), [0; 32]), (key(2), [2; 32])], key(0), 2
    => Ok(vec![Some([0; 32]), None])
    ; "read subset of partially set range without running too far"
    )]
    #[test_case(
    &[], *u256_to_bytes32(U256::MAX), 2
    => Err(())
    ; "read fails on uninitialized range if keyspace exceeded"
    )]
    #[test_case(
    &[(*u256_to_bytes32(U256::MAX), [0; 32])], *u256_to_bytes32(U256::MAX), 2
    => Err(())
    ; "read fails on partially initialized range if keyspace exceeded"
    )]
    fn read_sequential_range(
        prefilled_slots: &[([u8; 32], [u8; 32])],
        start_key: [u8; 32],
        range: usize,
    ) -> Result<Vec<Option<[u8; 32]>>, ()> {
        let mut db = VmStorage::<Database>::default();

        let contract_id = ContractId::new([0u8; 32]);

        // prefill db
        for (key, value) in prefilled_slots {
            let key = Bytes32::from(*key);
            let value = Bytes32::new(*value);

            StorageMutate::<ContractsState>::insert(
                db.database_mut(),
                &(&contract_id, &key).into(),
                &value,
            )
            .unwrap();
        }

        // perform sequential read
        Ok(db
            .merkle_contract_state_range(&contract_id, &Bytes32::new(start_key), range)
            .map_err(|_| ())?
            .into_iter()
            .map(|v| v.map(Cow::into_owned).map(|v| *v))
            .collect())
    }

    #[test_case(
    &[], key(0), &[[1; 32]]
    => Ok(false)
    ; "insert single value over uninitialized range"
    )]
    #[test_case(
    &[(key(0), [0; 32])], key(0), &[[1; 32]]
    => Ok(true)
    ; "insert single value over initialized range"
    )]
    #[test_case(
    &[], key(0), &[[1; 32], [2; 32]]
    => Ok(false)
    ; "insert multiple slots over uninitialized range"
    )]
    #[test_case(
    &[(key(1), [0; 32]), (key(2), [0; 32])], key(0), &[[1; 32], [2; 32], [3; 32]]
    => Ok(false)
    ; "insert multiple slots with uninitialized start of the range"
    )]
    #[test_case(
    &[(key(0), [0; 32]), (key(2), [0; 32])], key(0), &[[1; 32], [2; 32], [3; 32]]
    => Ok(false)
    ; "insert multiple slots with uninitialized middle of the range"
    )]
    #[test_case(
    &[(key(0), [0; 32]), (key(1), [0; 32])], key(0), &[[1; 32], [2; 32], [3; 32]]
    => Ok(false)
    ; "insert multiple slots with uninitialized end of the range"
    )]
    #[test_case(
    &[(key(0), [0; 32]), (key(1), [0; 32]), (key(2), [0; 32])], key(0), &[[1; 32], [2; 32], [3; 32]]
    => Ok(true)
    ; "insert multiple slots over initialized range"
    )]
    #[test_case(
    &[(key(0), [0; 32]), (key(1), [0; 32]), (key(2), [0; 32]), (key(3), [0; 32])], key(1), &[[1; 32], [2; 32]]
    => Ok(true)
    ; "insert multiple slots over sub-range of prefilled data"
    )]
    #[test_case(
    &[], *u256_to_bytes32(U256::MAX), &[[1; 32], [2; 32]]
    => Err(())
    ; "insert fails if start_key + range > u256::MAX"
    )]
    fn insert_range(
        prefilled_slots: &[([u8; 32], [u8; 32])],
        start_key: [u8; 32],
        insertion_range: &[[u8; 32]],
    ) -> Result<bool, ()> {
        let mut db = VmStorage::<Database>::default();

        let contract_id = ContractId::new([0u8; 32]);

        // prefill db
        for (key, value) in prefilled_slots {
            let key = Bytes32::from(*key);
            let value = Bytes32::new(*value);

            StorageMutate::<ContractsState>::insert(
                db.database_mut(),
                &(&contract_id, &key).into(),
                &value,
            )
            .unwrap();
        }

        // test insert range
        let insert_status = db
            .merkle_contract_state_insert_range(
                &contract_id,
                &Bytes32::new(start_key),
                &insertion_range
                    .iter()
                    .map(|v| Bytes32::new(*v))
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| ())
            .map(|v| v == 0);

        // check stored data
        let results: Vec<_> = (0..insertion_range.len())
            .filter_map(|i| {
                let current_key =
                    U256::from_big_endian(&start_key).checked_add(i.into())?;
                let current_key = u256_to_bytes32(current_key);
                let result = db
                    .merkle_contract_state(&contract_id, &current_key)
                    .unwrap()
                    .map(Cow::into_owned)
                    .map(|b| *b);
                result
            })
            .collect();

        // verify all data from insertion request is actually inserted if successful
        // or not inserted at all if unsuccessful
        if insert_status.is_ok() {
            assert_eq!(insertion_range, results);
        } else {
            assert_eq!(results.len(), 0);
        }

        insert_status
    }

    #[test_case(
    &[], [0; 32], 1
    => (vec![], false)
    ; "remove single value over uninitialized range"
    )]
    #[test_case(
    &[([0; 32], [0; 32])], [0; 32], 1
    => (vec![], true)
    ; "remove single value over initialized range"
    )]
    #[test_case(
    &[], [0; 32], 2
    => (vec![], false)
    ; "remove multiple slots over uninitialized range"
    )]
    #[test_case(
    &[([0; 32], [0; 32]), (key(1), [0; 32])], [0; 32], 2
    => (vec![], true)
    ; "remove multiple slots over initialized range"
    )]
    #[test_case(
    &[(key(1), [0; 32]), (key(2), [0; 32])], [0; 32], 3
    => (vec![], false)
    ; "remove multiple slots over partially uninitialized start range"
    )]
    #[test_case(
    &[([0; 32], [0; 32]), (key(1), [0; 32])], [0; 32], 3
    => (vec![], false)
    ; "remove multiple slots over partially uninitialized end range"
    )]
    #[test_case(
    &[([0; 32], [0; 32]), (key(2), [0; 32])], [0; 32], 3
    => (vec![], false)
    ; "remove multiple slots over partially uninitialized middle range"
    )]
    fn remove_range(
        prefilled_slots: &[([u8; 32], [u8; 32])],
        start_key: [u8; 32],
        remove_count: usize,
    ) -> (Vec<[u8; 32]>, bool) {
        let mut db = VmStorage::<Database>::default();

        let contract_id = ContractId::new([0u8; 32]);

        // prefill db
        for (key, value) in prefilled_slots {
            let key = Bytes32::from(*key);
            let value = Bytes32::new(*value);

            StorageMutate::<ContractsState>::insert(
                db.database_mut(),
                &(&contract_id, &key).into(),
                &value,
            )
            .unwrap();
        }

        // test remove range
        let remove_status = db
            .merkle_contract_state_remove_range(
                &contract_id,
                &Bytes32::new(start_key),
                remove_count,
            )
            .unwrap()
            .is_some();

        // check stored data
        let results: Vec<_> = (0..remove_count)
            .filter_map(|i| {
                let (current_key, overflow) =
                    U256::from_big_endian(&start_key).overflowing_add(i.into());

                if overflow {
                    return None
                }

                let current_key = u256_to_bytes32(current_key);
                let result = db
                    .merkle_contract_state(&contract_id, &current_key)
                    .unwrap()
                    .map(Cow::into_owned)
                    .map(|b| *b);
                result
            })
            .collect();

        (results, remove_status)
    }
}
