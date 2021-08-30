use super::Hash;
use fuel_tx::ContractAddress;

#[derive(Debug, Copy, Clone)]
pub struct Contract {
    contract_id: ContractAddress,
    utxo_id: Hash,
    balance_root: Hash,
    state_root: Hash,
}
