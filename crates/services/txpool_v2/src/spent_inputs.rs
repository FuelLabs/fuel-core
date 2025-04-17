use fuel_core_types::{
    fuel_tx::{
        Input,
        TxId,
        UtxoId,
    },
    fuel_types::Nonce,
};
use lru::LruCache;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
};

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
enum InputKey {
    Tx(TxId),
    Utxo(UtxoId),
    Message(Nonce),
}

/// Keeps track of spent inputs.
pub struct SpentInputs {
    /// Use LRU to support automatic clean up of old entries.
    spent_inputs: LruCache<InputKey, ()>,
    /// When it is unclear whether an input has been spent, we want to store which
    /// transaction spent it. Later, this information can be used to unspent
    /// or fully spend the input.
    spender_of_inputs: HashMap<TxId, Vec<InputKey>>,
}

impl SpentInputs {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            spent_inputs: LruCache::new(capacity),
            spender_of_inputs: HashMap::new(),
        }
    }

    /// Marks inputs as spent, by preserves the information about the spender in the case
    /// if we need to unspend inputs later, see [`unspend_inputs`] for more details.
    ///
    /// This function is called when `TxPool` extracts transactions for the block producer.
    pub fn maybe_spend_inputs(&mut self, tx_id: TxId, inputs: &[Input]) {
        let inputs = inputs
            .iter()
            .filter_map(|input| {
                if input.is_coin() {
                    input.utxo_id().cloned().map(InputKey::Utxo)
                } else if input.is_message() {
                    input.nonce().cloned().map(InputKey::Message)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for input in inputs.iter() {
            self.spent_inputs.put(*input, ());
        }
        self.spent_inputs.put(InputKey::Tx(tx_id), ());
        self.spender_of_inputs.insert(tx_id, inputs);
    }

    pub fn spend_inputs(&mut self, tx_id: TxId, inputs: &[Input]) {
        let inputs = inputs.iter().filter_map(|input| {
            if input.is_coin() {
                input.utxo_id().cloned().map(InputKey::Utxo)
            } else if input.is_message() {
                input.nonce().cloned().map(InputKey::Message)
            } else {
                None
            }
        });

        for input in inputs {
            self.spent_inputs.put(input, ());
        }
        self.spend_inputs_by_tx_id(tx_id);
    }

    pub fn spend_inputs_by_tx_id(&mut self, tx_id: TxId) {
        self.spent_inputs.put(InputKey::Tx(tx_id), ());
        let inputs = self.spender_of_inputs.remove(&tx_id);

        if let Some(inputs) = inputs {
            for input in inputs {
                self.spent_inputs.put(input, ());
            }
        }
    }

    /// If transaction is skipped during the block production, this functions
    /// can be used to unspend inputs, allowing other transactions to spend them.
    pub fn unspend_inputs(&mut self, tx_id: TxId) {
        self.spent_inputs.pop(&InputKey::Tx(tx_id));
        let inputs = self.spender_of_inputs.remove(&tx_id);

        if let Some(inputs) = inputs {
            for input in inputs {
                self.spent_inputs.pop(&input);
            }
        }
    }

    pub fn is_spent_utxo(&self, input: &UtxoId) -> bool {
        self.spent_inputs.contains(&InputKey::Utxo(*input))
    }

    pub fn is_spent_message(&self, input: &Nonce) -> bool {
        self.spent_inputs.contains(&InputKey::Message(*input))
    }

    pub fn is_spent_tx(&self, tx: &TxId) -> bool {
        self.spent_inputs.contains(&InputKey::Tx(*tx))
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use fuel_core_types::fuel_tx::Input;
    use std::num::NonZeroUsize;

    #[test]
    fn maybe_spend_inputs_works__inputs_marked_as_spent() {
        let mut spent_inputs = SpentInputs::new(NonZeroUsize::new(10).unwrap());

        let tx_id = TxId::default();
        let input_1 = Input::coin_signed(
            UtxoId::new([123; 32].into(), 1),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let input_2 = Input::message_coin_signed(
            Default::default(),
            Default::default(),
            Default::default(),
            [123; 32].into(),
            Default::default(),
        );

        // Given
        assert!(!spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(!spent_inputs.is_spent_message(input_2.nonce().unwrap()));

        // When
        spent_inputs.maybe_spend_inputs(tx_id, &[input_1.clone(), input_2.clone()]);

        // Then
        assert!(spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(spent_inputs.is_spent_message(input_2.nonce().unwrap()));
    }

    #[test]
    fn unspend_inputs_works__after_maybe_spend_inputs() {
        let mut spent_inputs = SpentInputs::new(NonZeroUsize::new(10).unwrap());

        let tx_id = TxId::default();
        let input_1 = Input::coin_signed(
            UtxoId::new([123; 32].into(), 1),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let input_2 = Input::message_coin_signed(
            Default::default(),
            Default::default(),
            Default::default(),
            [123; 32].into(),
            Default::default(),
        );

        // Given
        spent_inputs.maybe_spend_inputs(tx_id, &[input_1.clone(), input_2.clone()]);
        assert!(spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(spent_inputs.is_spent_message(input_2.nonce().unwrap()));

        // When
        spent_inputs.unspend_inputs(tx_id);

        // Then
        assert!(!spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(!spent_inputs.is_spent_message(input_2.nonce().unwrap()));
    }

    #[test]
    fn unspend_inputs_do_nothing__after_spend_inputs_by_tx_id() {
        let mut spent_inputs = SpentInputs::new(NonZeroUsize::new(10).unwrap());

        let tx_id = TxId::default();
        let input_1 = Input::coin_signed(
            UtxoId::new([123; 32].into(), 1),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let input_2 = Input::message_coin_signed(
            Default::default(),
            Default::default(),
            Default::default(),
            [123; 32].into(),
            Default::default(),
        );

        // Given
        spent_inputs.maybe_spend_inputs(tx_id, &[input_1.clone(), input_2.clone()]);
        assert!(spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(spent_inputs.is_spent_message(input_2.nonce().unwrap()));
        spent_inputs.spend_inputs_by_tx_id(tx_id);

        // When
        spent_inputs.unspend_inputs(tx_id);

        // Then
        assert!(spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(spent_inputs.is_spent_message(input_2.nonce().unwrap()));
    }

    #[test]
    fn unspend_inputs_do_nothing__after_spend_inputs__with_valid_inputs() {
        let mut spent_inputs = SpentInputs::new(NonZeroUsize::new(10).unwrap());

        let tx_id = TxId::default();
        let input_1 = Input::coin_signed(
            UtxoId::new([123; 32].into(), 1),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let input_2 = Input::message_coin_signed(
            Default::default(),
            Default::default(),
            Default::default(),
            [123; 32].into(),
            Default::default(),
        );

        // Given
        spent_inputs.maybe_spend_inputs(tx_id, &[input_1.clone(), input_2.clone()]);
        assert!(spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(spent_inputs.is_spent_message(input_2.nonce().unwrap()));
        spent_inputs.spend_inputs(tx_id, &[input_1.clone(), input_2.clone()]);

        // When
        spent_inputs.unspend_inputs(tx_id);

        // Then
        assert!(spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(spent_inputs.is_spent_message(input_2.nonce().unwrap()));
    }

    #[test]
    fn unspend_inputs_do_nothing__after_spend_inputs__without_valid_inputs() {
        let mut spent_inputs = SpentInputs::new(NonZeroUsize::new(10).unwrap());

        let tx_id = TxId::default();
        let input_1 = Input::coin_signed(
            UtxoId::new([123; 32].into(), 1),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let input_2 = Input::message_coin_signed(
            Default::default(),
            Default::default(),
            Default::default(),
            [123; 32].into(),
            Default::default(),
        );

        // Given
        spent_inputs.maybe_spend_inputs(tx_id, &[input_1.clone(), input_2.clone()]);
        assert!(spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(spent_inputs.is_spent_message(input_2.nonce().unwrap()));
        spent_inputs.spend_inputs(tx_id, &[]);

        // When
        spent_inputs.unspend_inputs(tx_id);

        // Then
        assert!(spent_inputs.is_spent_utxo(input_1.utxo_id().unwrap()));
        assert!(spent_inputs.is_spent_message(input_2.nonce().unwrap()));
    }
}
