#[derive(derive_more::Display)]
pub(crate) enum Error {
    #[display(fmt = "Transaction has been skipped during block insertion: {_0}")]
    SkippedTransaction(String),
}
