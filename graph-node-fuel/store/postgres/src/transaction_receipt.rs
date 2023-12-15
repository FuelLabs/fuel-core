use diesel::sql_types::{Binary, Nullable};
use diesel_derives::QueryableByName;
use graph::prelude::transaction_receipt::LightTransactionReceipt;
use itertools::Itertools;
use std::convert::TryFrom;

/// Type that comes straight out of a SQL query
#[derive(QueryableByName)]
pub(crate) struct RawTransactionReceipt {
    #[sql_type = "Binary"]
    transaction_hash: Vec<u8>,
    #[sql_type = "Binary"]
    transaction_index: Vec<u8>,
    #[sql_type = "Nullable<Binary>"]
    block_hash: Option<Vec<u8>>,
    #[sql_type = "Nullable<Binary>"]
    block_number: Option<Vec<u8>>,
    #[sql_type = "Nullable<Binary>"]
    gas_used: Option<Vec<u8>>,
    #[sql_type = "Nullable<Binary>"]
    status: Option<Vec<u8>>,
}

impl TryFrom<RawTransactionReceipt> for LightTransactionReceipt {
    type Error = anyhow::Error;

    fn try_from(value: RawTransactionReceipt) -> Result<Self, Self::Error> {
        let RawTransactionReceipt {
            transaction_hash,
            transaction_index,
            block_hash,
            block_number,
            gas_used,
            status,
        } = value;

        let transaction_hash = drain_vector(transaction_hash)?;
        let transaction_index = drain_vector(transaction_index)?;
        let block_hash = block_hash.map(drain_vector).transpose()?;
        let block_number = block_number.map(drain_vector).transpose()?;
        let gas_used = gas_used.map(drain_vector).transpose()?;
        let status = status.map(drain_vector).transpose()?;

        Ok(LightTransactionReceipt {
            transaction_hash: transaction_hash.into(),
            transaction_index: transaction_index.into(),
            block_hash: block_hash.map(Into::into),
            block_number: block_number.map(Into::into),
            gas_used: gas_used.map(Into::into),
            status: status.map(Into::into),
        })
    }
}

/// Converts Vec<u8> to [u8; N], where N is the vector's expected lenght.
/// Fails if input size is larger than output size.
pub(crate) fn drain_vector<const N: usize>(input: Vec<u8>) -> Result<[u8; N], anyhow::Error> {
    anyhow::ensure!(input.len() <= N, "source is larger than output");
    let mut output = [0u8; N];
    let start = output.len() - input.len();
    output[start..].iter_mut().set_from(input);
    Ok(output)
}

#[test]
fn test_drain_vector() {
    let input = vec![191, 153, 17];
    let expected_output = [0, 0, 0, 0, 0, 191, 153, 17];
    let result = drain_vector(input).expect("failed to drain vector into array");
    assert_eq!(result, expected_output);
}
