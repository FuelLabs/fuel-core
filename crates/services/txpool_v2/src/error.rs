use fuel_core_types::fuel_tx::UtxoId;

#[derive(Debug, derive_more::Display)]
pub enum Error {
    #[display(fmt = "Database error: {_0}")]
    Database(String),
    #[display(fmt = "Transaction not found: {_0}")]
    TransactionNotFound(String),
    #[display(fmt = "Wrong number of outputs: {_0}")]
    WrongOutputNumber(String),
    #[display(
        fmt = "Transaction is not inserted. Input coin does not match the values from database"
    )]
    NotInsertedIoCoinMismatch,
    // TODO: Make more specific errors
    #[display(fmt = "Transaction collided: {_0}")]
    Collided(String),
    #[display(fmt = "Utxo not found: {_0}")]
    UtxoNotFound(UtxoId),
}
