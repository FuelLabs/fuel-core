use crate::protobuf::*;
use graph::runtime::{asc_new, AscHeap, AscIndexId, AscPtr, AscType, DeterministicHostError, HostExportError, IndexForAscTypeId, ToAscObj};
use graph::runtime::gas::GasCounter;
pub use graph::semver::Version;
use graph_runtime_wasm::asc_abi::class::TypedArray;

type Uint64Array = TypedArray<u64>;


pub struct AscBytesArray(pub Array<AscPtr<Uint8Array>>);
pub struct Ascu64Array(pub Array<AscPtr<Uint64Array>>);
pub struct AscInputArray(pub Array<AscPtr<Input>>);
pub struct AscOutputArray(pub Array<AscPtr<Output>>);
pub struct AscStorageSlotArray(pub Array<AscPtr<StorageSlot>>);
pub struct AscTransactionArray(pub Array<AscPtr<Transaction>>);

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
        //     .map(|x| asc_new(heap, &x.to_asc_bytes(), gas))
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
        Ok(Self(Array::from_asc_bytes(asc_obj, api_version)?))
    }
}

impl AscIndexId for Ascu64Array {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::FuelU64Array;
}

impl AscType for AscTransactionArray {
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

impl AscIndexId for AscTransactionArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::FuelTransactionArray;
}

impl AscType for AscInputArray {
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

impl AscIndexId for AscInputArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::FuelInputArray;
}

impl AscType for AscOutputArray {
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

impl AscIndexId for AscOutputArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::FuelOutputArray;
}


impl AscType for AscStorageSlotArray {
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

impl AscIndexId for AscStorageSlotArray {
    const INDEX_ASC_TYPE_ID: IndexForAscTypeId = IndexForAscTypeId::FuelStorageSlotArray;
}


// Todo Emir
impl ToAscObj<AscTransactionArray> for Vec<Transaction>{
    fn to_asc_obj<H: AscHeap + ?Sized>(&self, heap: &mut H, gas: &GasCounter) -> Result<AscTransactionArray, HostExportError> {
        todo!()
    }
}

impl ToAscObj<AscInputArray> for Vec<Input>{
    fn to_asc_obj<H: AscHeap + ?Sized>(&self, heap: &mut H, gas: &GasCounter) -> Result<AscInputArray, HostExportError> {
        todo!()
    }
}

impl ToAscObj<AscOutputArray> for Vec<Output>{
    fn to_asc_obj<H: AscHeap + ?Sized>(&self, heap: &mut H, gas: &GasCounter) -> Result<AscOutputArray, HostExportError> {
        todo!()
    }
}

impl ToAscObj<AscStorageSlotArray> for Vec<StorageSlot>{
    fn to_asc_obj<H: AscHeap + ?Sized>(&self, heap: &mut H, gas: &GasCounter) -> Result<AscStorageSlotArray, HostExportError> {
        todo!()
    }
}

