use graph::{
    prelude::BigInt,
    runtime::{asc_new, gas::GasCounter, AscHeap, HostExportError, ToAscObj},
};
use crate::codec::*;

pub(crate) use super::generated::*;

// impl ToAscObj<AscBlock> for Block {
//     fn to_asc_obj<H: AscHeap + ?Sized>(
//         &self,
//         heap: &mut H,
//         gas: &GasCounter,
//     ) -> Result<AscBlock, HostExportError> {
//         Ok(AscBlock {
//             number: asc_new(heap, &BigInt::from(self.height), gas)?,
//             hash: asc_new(heap, self.hash.as_slice(), gas)?,
//             prev_hash: asc_new(heap, self.prev_hash.as_slice(), gas)?,
//             timestamp: asc_new(heap, &BigInt::from(self.timestamp), gas)?,
//         })
//     }
// }
