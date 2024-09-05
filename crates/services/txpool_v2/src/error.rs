#[derive(Debug, derive_more::Display)]
pub enum Error {
    #[display(fmt = "Transaction not found: {_0}")]
    TransactionNotFound(String),
    #[display(fmt = "Wrong number of outputs: {_0}")]
    WrongOutputNumber(String),
    #[display(
        fmt = "Transaction is not inserted. Maximum depth of dependent transaction chain reached"
    )]
    NotInsertedMaxDepth,
}
