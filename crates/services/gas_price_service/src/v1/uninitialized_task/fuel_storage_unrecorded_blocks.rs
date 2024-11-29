use fuel_gas_price_algorithm::v1::UnrecordedBlocks;

#[derive(Debug, Clone)]
pub struct FuelStorageUnrecordedBlocks;

impl UnrecordedBlocks for FuelStorageUnrecordedBlocks {
    fn insert(
        &mut self,
        height: fuel_gas_price_algorithm::v1::Height,
        bytes: fuel_gas_price_algorithm::v1::Bytes,
    ) {
        todo!()
    }

    fn remove(
        &mut self,
        height: &fuel_gas_price_algorithm::v1::Height,
    ) -> Option<fuel_gas_price_algorithm::v1::Bytes> {
        todo!()
    }
}
