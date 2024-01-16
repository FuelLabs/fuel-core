use crate::protobuf::*;
use graph::runtime::{
    asc_new,
    gas::GasCounter,
    AscHeap,
    AscIndexId,
    AscPtr,
    AscType,
    DeterministicHostError,
    HostExportError,
    IndexForAscTypeId,
    ToAscObj,
};
pub use graph::semver::Version;

pub struct AscBytesArray(pub Array<AscPtr<Uint8Array>>);
pub struct Ascu64Array(pub Array<AscPtr<u64>>);

impl AscIndexId for Block {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::FuelBlock;
}

impl AscIndexId for Transaction {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::FuelTransaction;
}

impl AscType for AscBytesArray {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        self.0.to_asc_bytes()
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        Ok(Self(Array::from_asc_bytes(asc_obj, api_version)?))
    }
}

impl AscIndexId for AscBytesArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::FuelBytesArray;
}

impl ToAscObj<AscBytesArray> for Vec<Vec<u8>> {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<AscBytesArray, HostExportError> {
        let content: Result<Vec<_>, _> = self
            .iter()
            .map(|x| asc_new(heap, &graph_runtime_wasm::asc_abi::class::Bytes(x), gas))
            .collect();

        Ok(AscBytesArray(Array::new(&content?, heap, gas)?))
    }
}

impl ToAscObj<Ascu64Array> for Vec<u64> {
    fn to_asc_obj<H: AscHeap + ?Sized>(
        &self,
        heap: &mut H,
        gas: &GasCounter,
    ) -> Result<Ascu64Array, HostExportError> {
        // let content: Result<Vec<_>, _> = self
        //     .iter()
        //     .map(|x| asc_new(heap, x, gas))
        //     .collect();
        //
        // Ok(Ascu64Array(Array::new(&content?, heap, gas)?))
        todo!()
    }
}

impl AscType for Ascu64Array {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        self.0.to_asc_bytes()
    }

    fn from_asc_bytes(
        asc_obj: &[u8],
        api_version: &Version,
    ) -> Result<Self, DeterministicHostError> {
        // Ok(Self(Ascu64Array(Array::from_asc_bytes(asc_obj, api_version)?)))
        todo!()
    }
}

impl AscIndexId for Ascu64Array {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::FuelU64Array;
}

#[cfg(test)]
mod test {
    use crate::protobuf::*;
    use graph::runtime::{
        AscPtr,
        AscType,
    };

    use graph::semver::Version;

    /// A macro that takes an ASC struct value definition and calls AscBytes methods to check that
    /// memory layout is padded properly.
    macro_rules! assert_asc_bytes {
        ($struct_name:ident {
            $($field:ident : $field_value:expr),+
            $(,)? // trailing
        }) => {
            let value = $struct_name {
                $($field: $field_value),+
            };

            // just call the function. it will panic on misalignments
            let asc_bytes = value.to_asc_bytes().unwrap();

            let value_004 = $struct_name::from_asc_bytes(&asc_bytes, &Version::new(0, 0, 4)).unwrap();
            let value_005 = $struct_name::from_asc_bytes(&asc_bytes, &Version::new(0, 0, 5)).unwrap();

            // turn the values into bytes again to verify that they are the same as the original
            // because these types usually don't implement PartialEq
            assert_eq!(
                asc_bytes,
                value_004.to_asc_bytes().unwrap(),
                "Expected {} v0.0.4 asc bytes to be the same",
                stringify!($struct_name)
            );
            assert_eq!(
                asc_bytes,
                value_005.to_asc_bytes().unwrap(),
                "Expected {} v0.0.5 asc bytes to be the same",
                stringify!($struct_name)
            );
        };
    }

    #[test]
    fn test_asc_type_alignment() {
        // TODO: automatically generate these tests for each struct in derive(AscType) macro

        assert_asc_bytes!(AscBlock {
            id: new_asc_ptr(),
            height: 1,
            da_height: 1,
            msg_receipt_count: 0,
            tx_root: new_asc_ptr(),
            msg_receipt_root: new_asc_ptr(),
            prev_id: new_asc_ptr(),
            prev_root: new_asc_ptr(),
            timestamp: 0,
            application_hash: new_asc_ptr(),
            transactions: new_asc_ptr(),
        });

        assert_asc_bytes!(AscTransaction {
            script: new_asc_ptr(),
            create: new_asc_ptr(),
            mint: new_asc_ptr(),
        });

        assert_asc_bytes!(AscScript {
            script_gas_limit: 0,
            script: new_asc_ptr(),
            script_data: new_asc_ptr(),
            policies: new_asc_ptr(),
            inputs: new_asc_ptr(),
            outputs: new_asc_ptr(),
            witnesses: new_asc_ptr(),
            receipts_root: new_asc_ptr(),
        });

        assert_asc_bytes!(AscCreate {
            bytecode_length: 0,
            bytecode_witness_index: 0,
            policies: new_asc_ptr(),
            storage_slots: new_asc_ptr(),
            inputs: new_asc_ptr(),
            outputs: new_asc_ptr(),
            witnesses: new_asc_ptr(),
            salt: new_asc_ptr(),
        });

        assert_asc_bytes!(AscMint {
            tx_pointer: new_asc_ptr(),
            input_contract: new_asc_ptr(),
            output_contract: new_asc_ptr(),
            mint_amount: 0,
            mint_asset_id: new_asc_ptr(),
        });

        assert_asc_bytes!(AscInput {
            coin_signed: new_asc_ptr(),
            coin_predicate: new_asc_ptr(),
            contract: new_asc_ptr(),
            message_coin_signed: new_asc_ptr(),
            message_coin_predicate: new_asc_ptr(),
            message_data_signed: new_asc_ptr(),
            message_data_predicate: new_asc_ptr(),
        });

        assert_asc_bytes!(AscCoin {
            utxo_id: new_asc_ptr(),
            owner: new_asc_ptr(),
            amount: 0,
            asset_id: new_asc_ptr(),
            tx_pointer: new_asc_ptr(),
            witness_index: 0,
            maturity: 0,
            predicate_gas_used: 0,
            predicate: new_asc_ptr(),
            predicate_data: new_asc_ptr(),
        });

        assert_asc_bytes!(AscMessage {
            sender: new_asc_ptr(),
            recipient: new_asc_ptr(),
            amount: 0,
            nonce: new_asc_ptr(),
            witness_index: 0,
            predicate_gas_used: 0,
            data: new_asc_ptr(),
            predicate: new_asc_ptr(),
            predicate_data: new_asc_ptr(),
        });

        assert_asc_bytes!(AscOutput {
            coin: new_asc_ptr(),
            contract: new_asc_ptr(),
            change: new_asc_ptr(),
            variable: new_asc_ptr(),
            contract_created: new_asc_ptr(),
        });

        assert_asc_bytes!(AscOutputCoin {
            to: new_asc_ptr(),
            amount: 0,
            asset_id: new_asc_ptr(),
        });

        assert_asc_bytes!(AscOutputContractCreated {
            contract_id: new_asc_ptr(),
            state_root: new_asc_ptr(),
        });

        assert_asc_bytes!(AscInputContract {
            utxo_id: new_asc_ptr(),
            balance_root: new_asc_ptr(),
            state_root: new_asc_ptr(),
            tx_pointer: new_asc_ptr(),
            contract_id: new_asc_ptr(),
        });

        assert_asc_bytes!(AscOutputContract {
            input_index: 0,
            balance_root: new_asc_ptr(),
            state_root: new_asc_ptr(),
        });

        assert_asc_bytes!(AscStorageSlot {
            key: new_asc_ptr(),
            value: new_asc_ptr(),
        });

        assert_asc_bytes!(AscUtxoId {
            tx_id: new_asc_ptr(),
            output_index: 0
        });

        assert_asc_bytes!(AscTxPointer {
            block_height: 0,
            tx_index: 0
        });

        assert_asc_bytes!(AscPolicies {
            values: new_asc_ptr()
        });
    }

    // non-null AscPtr
    fn new_asc_ptr<T>() -> AscPtr<T> {
        AscPtr::new(12)
    }
}
