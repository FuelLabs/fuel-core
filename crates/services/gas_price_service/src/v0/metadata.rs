use fuel_gas_price_algorithm::v0::AlgorithmUpdaterV0;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct V0Metadata {
    /// The gas price to cover the execution of the next block
    pub new_exec_price: u64,
    /// The height for which the `new_exec_price` is calculated, which should be the _next_ block
    pub l2_block_height: u32,
}

pub struct V0MetadataInitializer {
    pub starting_gas_price: u64,
    pub min_gas_price: u64,
    pub gas_price_change_percent: u64,
    pub gas_price_threshold_percent: u64,
}

impl V0MetadataInitializer {
    pub fn initialize(&self, l2_block_height: u32) -> V0Metadata {
        V0Metadata {
            new_exec_price: self.starting_gas_price.max(self.min_gas_price),
            l2_block_height,
        }
    }
}

// impl From<V0Metadata> for AlgorithmUpdaterV0 {
//     fn from(metadata: V0Metadata) -> Self {
//         Self {
//             new_exec_price: metadata.new_exec_price,
//             l2_block_height: metadata.l2_block_height,
//         }
//     }
// }

impl From<AlgorithmUpdaterV0> for V0Metadata {
    fn from(updater: AlgorithmUpdaterV0) -> Self {
        Self {
            new_exec_price: updater.new_exec_price,
            l2_block_height: updater.l2_block_height,
        }
    }
}
