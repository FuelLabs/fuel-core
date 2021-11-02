use super::Hash;
use fuel_tx::ContractId;

#[derive(Debug, Copy, Clone)]
pub struct Contract {
    contract_id: ContractId,
    utxo_id: Hash,
    balance_root: Hash,
    state_root: Hash,
}
