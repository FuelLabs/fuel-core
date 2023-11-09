use std::sync::Arc;

use parquet::{basic::Repetition, schema::types::Type};

use crate::{
    config::{contract_balance::ContractBalance, contract_state::ContractState},
    CoinConfig, ContractConfig, MessageConfig,
};

pub trait ParquetSchema {
    fn schema() -> Type;
    fn num_of_columns() -> usize {
        Self::schema().get_fields().len()
    }
}

impl ParquetSchema for ContractConfig {
    fn schema() -> Type {
        use parquet::basic::Type as PhysicalType;
        let contract_id = Type::primitive_type_builder(
            "contract_id",
            PhysicalType::FIXED_LEN_BYTE_ARRAY,
        )
        .with_length(32)
        .with_repetition(Repetition::REQUIRED)
        .build()
        .unwrap();
        let code = Type::primitive_type_builder("code", PhysicalType::BYTE_ARRAY)
            .with_repetition(Repetition::REQUIRED)
            .build()
            .unwrap();

        let salt =
            Type::primitive_type_builder("salt", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .unwrap();

        let tx_id =
            Type::primitive_type_builder("tx_id", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::OPTIONAL)
                .build()
                .unwrap();

        let output_index =
            Type::primitive_type_builder("output_index", PhysicalType::INT32)
                .with_converted_type(parquet::basic::ConvertedType::UINT_8)
                .with_repetition(Repetition::OPTIONAL)
                .build()
                .unwrap();

        let tx_pointer_block_height =
            Type::primitive_type_builder("tx_pointer_block_height", PhysicalType::INT32)
                .with_converted_type(parquet::basic::ConvertedType::UINT_32)
                .with_repetition(Repetition::OPTIONAL)
                .build()
                .unwrap();

        let tx_pointer_tx_idx =
            Type::primitive_type_builder("tx_pointer_tx_idx", PhysicalType::INT32)
                .with_converted_type(parquet::basic::ConvertedType::UINT_16)
                .with_repetition(Repetition::OPTIONAL)
                .build()
                .unwrap();

        parquet::schema::types::Type::group_type_builder("ContractConfig")
            .with_fields(
                [
                    contract_id,
                    code,
                    salt,
                    tx_id,
                    output_index,
                    tx_pointer_block_height,
                    tx_pointer_tx_idx,
                ]
                .map(Arc::new)
                .to_vec(),
            )
            .build()
            .unwrap()
    }
}

impl ParquetSchema for MessageConfig {
    fn schema() -> Type {
        use parquet::basic::Type as PhysicalType;
        let sender =
            Type::primitive_type_builder("sender", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .unwrap();
        let recipient =
            Type::primitive_type_builder("recipient", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .unwrap();
        let nonce =
            Type::primitive_type_builder("nonce", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .unwrap();
        let amount = Type::primitive_type_builder("amount", PhysicalType::INT64)
            .with_converted_type(parquet::basic::ConvertedType::UINT_64)
            .with_repetition(Repetition::REQUIRED)
            .build()
            .unwrap();
        let data = Type::primitive_type_builder("data", PhysicalType::BYTE_ARRAY)
            .with_repetition(Repetition::REQUIRED)
            .build()
            .unwrap();
        let da_height = Type::primitive_type_builder("da_height", PhysicalType::INT64)
            .with_converted_type(parquet::basic::ConvertedType::UINT_64)
            .with_repetition(Repetition::REQUIRED)
            .build()
            .unwrap();

        parquet::schema::types::Type::group_type_builder("CoinConfig")
            .with_fields(
                [sender, recipient, nonce, amount, data, da_height]
                    .map(Arc::new)
                    .to_vec(),
            )
            .build()
            .unwrap()
    }
}

impl ParquetSchema for CoinConfig {
    fn schema() -> Type {
        use parquet::basic::Type as PhysicalType;
        let tx_id =
            Type::primitive_type_builder("tx_id", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::OPTIONAL)
                .build()
                .unwrap();
        let output_index =
            Type::primitive_type_builder("output_index", PhysicalType::INT32)
                .with_converted_type(parquet::basic::ConvertedType::UINT_8)
                .with_repetition(Repetition::OPTIONAL)
                .build()
                .unwrap();
        let tx_pointer_block_height =
            Type::primitive_type_builder("tx_pointer_block_height", PhysicalType::INT32)
                .with_converted_type(parquet::basic::ConvertedType::UINT_32)
                .with_repetition(Repetition::OPTIONAL)
                .build()
                .unwrap();
        let tx_pointer_tx_idx =
            Type::primitive_type_builder("tx_pointer_tx_idx", PhysicalType::INT32)
                .with_converted_type(parquet::basic::ConvertedType::UINT_16)
                .with_repetition(Repetition::OPTIONAL)
                .build()
                .unwrap();
        let maturity = Type::primitive_type_builder("maturity", PhysicalType::INT32)
            .with_converted_type(parquet::basic::ConvertedType::UINT_32)
            .with_repetition(Repetition::OPTIONAL)
            .build()
            .unwrap();
        let owner =
            Type::primitive_type_builder("owner", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .unwrap();
        let amount = Type::primitive_type_builder("amount", PhysicalType::INT64)
            .with_converted_type(parquet::basic::ConvertedType::UINT_64)
            .with_repetition(Repetition::REQUIRED)
            .build()
            .unwrap();
        let asset_id =
            Type::primitive_type_builder("asset_id", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .unwrap();

        parquet::schema::types::Type::group_type_builder("CoinConfig")
            .with_fields(
                [
                    tx_id,
                    output_index,
                    tx_pointer_block_height,
                    tx_pointer_tx_idx,
                    maturity,
                    owner,
                    amount,
                    asset_id,
                ]
                .map(Arc::new)
                .to_vec(),
            )
            .build()
            .unwrap()
    }
}

impl ParquetSchema for ContractState {
    fn schema() -> Type {
        use parquet::basic::Type as PhysicalType;
        let key = Type::primitive_type_builder("key", PhysicalType::FIXED_LEN_BYTE_ARRAY)
            .with_length(32)
            .with_repetition(Repetition::REQUIRED)
            .build()
            .unwrap();
        let value =
            Type::primitive_type_builder("value", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .unwrap();

        parquet::schema::types::Type::group_type_builder("ContractState")
            .with_fields([key, value].map(Arc::new).to_vec())
            .build()
            .unwrap()
    }
}

impl ParquetSchema for ContractBalance {
    fn schema() -> Type {
        use parquet::basic::Type as PhysicalType;
        let asset_id =
            Type::primitive_type_builder("asset_id", PhysicalType::FIXED_LEN_BYTE_ARRAY)
                .with_length(32)
                .with_repetition(Repetition::REQUIRED)
                .build()
                .unwrap();
        let amount = Type::primitive_type_builder("amount", PhysicalType::INT64)
            .with_converted_type(parquet::basic::ConvertedType::UINT_64)
            .with_repetition(Repetition::REQUIRED)
            .build()
            .unwrap();

        parquet::schema::types::Type::group_type_builder("ContractBalance")
            .with_fields([asset_id, amount].map(Arc::new).to_vec())
            .build()
            .unwrap()
    }
}
