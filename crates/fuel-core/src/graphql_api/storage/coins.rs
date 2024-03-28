use fuel_core_storage::{
    blueprint::plain::Plain,
    codec::{
        postcard::Postcard,
        primitive::utxo_id_to_bytes,
        raw::Raw,
    },
    structured_storage::TableWithBlueprint,
    Mappable,
};
use fuel_core_txpool::types::TxId;
use fuel_core_types::fuel_tx::{
    Address,
    UtxoId,
};

// TODO: Reuse `fuel_vm::storage::double_key` macro.
pub fn owner_coin_id_key(owner: &Address, coin_id: &UtxoId) -> OwnedCoinKey {
    let mut default = [0u8; Address::LEN + TxId::LEN + 2];
    default[0..Address::LEN].copy_from_slice(owner.as_ref());
    let utxo_id_bytes: [u8; TxId::LEN + 2] = utxo_id_to_bytes(coin_id);
    default[Address::LEN..].copy_from_slice(utxo_id_bytes.as_ref());
    default
}

/// The storage table of owned coin ids. Maps addresses to owned coins.
pub struct OwnedCoins;
/// The storage key for owned coins: `Address ++ UtxoId`
pub type OwnedCoinKey = [u8; Address::LEN + TxId::LEN + 2];

impl Mappable for OwnedCoins {
    type Key = Self::OwnedKey;
    type OwnedKey = OwnedCoinKey;
    type Value = Self::OwnedValue;
    type OwnedValue = ();
}

impl TableWithBlueprint for OwnedCoins {
    type Blueprint = Plain<Raw, Postcard>;
    type Column = super::Column;

    fn column() -> Self::Column {
        Self::Column::OwnedCoins
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn generate_key(rng: &mut impl rand::Rng) -> <OwnedCoins as Mappable>::Key {
        let mut bytes = [0u8; 66];
        rng.fill(bytes.as_mut());
        bytes
    }

    fuel_core_storage::basic_storage_tests!(
        OwnedCoins,
        [0u8; 66],
        <OwnedCoins as Mappable>::Value::default(),
        <OwnedCoins as Mappable>::Value::default(),
        generate_key
    );
}
