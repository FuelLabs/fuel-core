use std::collections::HashMap;
use std::str::FromStr;

use crate::codec::{entity_change, EntityChanges};
use anyhow::{anyhow, Error};
use graph::blockchain::block_stream::{
    BlockStreamEvent, BlockStreamMapper, BlockWithTriggers, FirehoseCursor, SubstreamsError,
};
use graph::data::store::scalar::Bytes;
use graph::data::store::IdType;
use graph::data::value::Word;
use graph::data_source::CausalityRegion;
use graph::prelude::{async_trait, BigInt, BlockHash, BlockNumber, Logger, Value};
use graph::prelude::{BigDecimal, BlockPtr};
use graph::schema::InputSchema;
use graph::substreams::Clock;
use prost::Message;

use crate::{Block, Chain, ParsedChanges, TriggerData};

// WasmBlockMapper will not perform any transformation to the block and cannot make assumptions
// about the block format. This mode just works a passthrough from the block stream to the subgraph
// mapping which will do the decoding and store actions.
pub struct WasmBlockMapper {
    pub handler: String,
}

#[async_trait]
impl BlockStreamMapper<Chain> for WasmBlockMapper {
    fn decode_block(&self, _output: Option<&[u8]>) -> Result<Option<crate::Block>, Error> {
        unreachable!("WasmBlockMapper does not do block decoding")
    }

    async fn block_with_triggers(
        &self,
        _logger: &Logger,
        _block: crate::Block,
    ) -> Result<BlockWithTriggers<Chain>, Error> {
        unreachable!("WasmBlockMapper does not do trigger decoding")
    }

    async fn handle_substreams_block(
        &self,
        _logger: &Logger,
        clock: Clock,
        cursor: FirehoseCursor,
        block: Vec<u8>,
    ) -> Result<BlockStreamEvent<Chain>, Error> {
        let Clock {
            id,
            number,
            timestamp: _,
        } = clock;

        let block_ptr = BlockPtr {
            hash: BlockHash::from(id.into_bytes()),
            number: BlockNumber::from(TryInto::<i32>::try_into(number)?),
        };

        let block_data = block.into_boxed_slice();

        Ok(BlockStreamEvent::ProcessWasmBlock(
            block_ptr,
            block_data,
            self.handler.clone(),
            cursor,
        ))
    }
}

// Mapper will transform the proto content coming from substreams in the graph-out format
// into the internal Block representation. If schema is passed then additional transformation
// into from the substreams block representation is performed into the Entity model used by
// the store. If schema is None then only the original block is passed. This None should only
// be used for block ingestion where entity content is empty and gets discarded.
pub struct Mapper {
    pub schema: Option<InputSchema>,
    // Block ingestors need the block to be returned so they can populate the cache
    // block streams, however, can shave some time by just skipping.
    pub skip_empty_blocks: bool,
}

#[async_trait]
impl BlockStreamMapper<Chain> for Mapper {
    fn decode_block(&self, output: Option<&[u8]>) -> Result<Option<Block>, Error> {
        let changes: EntityChanges = match output {
            Some(msg) => Message::decode(msg).map_err(SubstreamsError::DecodingError)?,
            None => EntityChanges {
                entity_changes: [].to_vec(),
            },
        };

        let parsed_changes = match self.schema.as_ref() {
            Some(schema) => parse_changes(&changes, schema)?,
            None if self.skip_empty_blocks => return Ok(None),
            None => vec![],
        };

        let hash = BlockHash::zero();
        let number = BlockNumber::MIN;
        let block = Block {
            hash,
            number,
            changes,
            parsed_changes,
        };

        Ok(Some(block))
    }

    async fn block_with_triggers(
        &self,
        logger: &Logger,
        block: Block,
    ) -> Result<BlockWithTriggers<Chain>, Error> {
        let mut triggers = vec![];
        if block.changes.entity_changes.len() >= 1 {
            triggers.push(TriggerData {});
        }

        Ok(BlockWithTriggers::new(block, triggers, logger))
    }

    async fn handle_substreams_block(
        &self,
        logger: &Logger,
        clock: Clock,
        cursor: FirehoseCursor,
        block: Vec<u8>,
    ) -> Result<BlockStreamEvent<Chain>, Error> {
        let block_number: BlockNumber = clock.number.try_into()?;
        let block_hash = clock.id.as_bytes().to_vec().try_into()?;

        let block = self
            .decode_block(Some(&block))?
            .ok_or_else(|| anyhow!("expected block to not be empty"))?;

        let block = self.block_with_triggers(logger, block).await.map(|bt| {
            let mut block = bt;

            block.block.number = block_number;
            block.block.hash = block_hash;
            block
        })?;

        Ok(BlockStreamEvent::ProcessBlock(block, cursor))
    }
}

fn parse_changes(
    changes: &EntityChanges,
    schema: &InputSchema,
) -> anyhow::Result<Vec<ParsedChanges>> {
    let mut parsed_changes = vec![];
    for entity_change in changes.entity_changes.iter() {
        let mut parsed_data: HashMap<Word, Value> = HashMap::default();
        let entity_type = schema.entity_type(&entity_change.entity)?;

        // Make sure that the `entity_id` gets set to a value
        // that is safe for roundtrips through the database. In
        // particular, if the type of the id is `Bytes`, we have
        // to make sure that the `entity_id` starts with `0x` as
        // that will be what the key for such an entity have
        // when it is read from the database.
        //
        // Needless to say, this is a very ugly hack, and the
        // real fix is what's described in [this
        // issue](https://github.com/graphprotocol/graph-node/issues/4663)
        let entity_id: String = match entity_type.id_type()? {
            IdType::String | IdType::Int8 => entity_change.id.clone(),
            IdType::Bytes => {
                if entity_change.id.starts_with("0x") {
                    entity_change.id.clone()
                } else {
                    format!("0x{}", entity_change.id)
                }
            }
        };
        // Substreams don't currently support offchain data
        let key = entity_type.parse_key_in(Word::from(entity_id), CausalityRegion::ONCHAIN)?;

        let id = key.id_value();
        parsed_data.insert(Word::from("id"), id);

        let changes = match entity_change.operation() {
            entity_change::Operation::Create | entity_change::Operation::Update => {
                for field in entity_change.fields.iter() {
                    let new_value: &crate::codec::value::Typed = match &field.new_value {
                        Some(crate::codec::Value {
                            typed: Some(new_value),
                        }) => &new_value,
                        _ => continue,
                    };

                    let value: Value = decode_value(new_value)?;
                    *parsed_data
                        .entry(Word::from(field.name.as_str()))
                        .or_insert(Value::Null) = value;
                }
                let entity = schema
                    .make_entity(parsed_data)
                    .map_err(anyhow::Error::from)?;

                ParsedChanges::Upsert { key, entity }
            }
            entity_change::Operation::Delete => ParsedChanges::Delete(key),
            entity_change::Operation::Unset => ParsedChanges::Unset,
        };
        parsed_changes.push(changes);
    }

    Ok(parsed_changes)
}

fn decode_value(value: &crate::codec::value::Typed) -> anyhow::Result<Value> {
    use crate::codec::value::Typed;

    match value {
        Typed::Int32(new_value) => Ok(Value::Int(*new_value)),

        Typed::Bigdecimal(new_value) => BigDecimal::from_str(new_value)
            .map(Value::BigDecimal)
            .map_err(|err| anyhow::Error::from(err)),

        Typed::Bigint(new_value) => BigInt::from_str(new_value)
            .map(Value::BigInt)
            .map_err(|err| anyhow::Error::from(err)),

        Typed::String(new_value) => {
            let mut string = new_value.clone();

            // Strip null characters since they are not accepted by Postgres.
            if string.contains('\u{0000}') {
                string = string.replace('\u{0000}', "");
            }
            Ok(Value::String(string))
        }

        Typed::Bytes(new_value) => base64::decode(new_value)
            .map(|bs| Value::Bytes(Bytes::from(bs)))
            .map_err(|err| anyhow::Error::from(err)),

        Typed::Bool(new_value) => Ok(Value::Bool(*new_value)),

        Typed::Array(arr) => arr
            .value
            .iter()
            .filter_map(|item| item.typed.as_ref().map(decode_value))
            .collect::<anyhow::Result<Vec<Value>>>()
            .map(Value::List),
    }
}

#[cfg(test)]
mod test {
    use std::{ops::Add, str::FromStr};

    use super::decode_value;
    use crate::codec::value::Typed;
    use crate::codec::{Array, Value};
    use graph::{
        data::store::scalar::Bytes,
        prelude::{BigDecimal, BigInt, Value as GraphValue},
    };

    #[test]
    fn validate_substreams_field_types() {
        struct Case {
            name: String,
            value: Value,
            expected_value: GraphValue,
        }

        let cases = vec![
            Case {
                name: "string value".to_string(),
                value: Value {
                    typed: Some(Typed::String(
                        "d4325ee72c39999e778a9908f5fb0803f78e30c441a5f2ce5c65eee0e0eba59d"
                            .to_string(),
                    )),
                },
                expected_value: GraphValue::String(
                    "d4325ee72c39999e778a9908f5fb0803f78e30c441a5f2ce5c65eee0e0eba59d".to_string(),
                ),
            },
            Case {
                name: "bytes value".to_string(),
                value: Value {
                    typed: Some(Typed::Bytes(
                        base64::encode(
                            hex::decode(
                                "445247fe150195bd866516594e087e1728294aa831613f4d48b8ec618908519f",
                            )
                            .unwrap(),
                        )
                        .into_bytes(),
                    )),
                },
                expected_value: GraphValue::Bytes(
                    Bytes::from_str(
                        "0x445247fe150195bd866516594e087e1728294aa831613f4d48b8ec618908519f",
                    )
                    .unwrap(),
                ),
            },
            Case {
                name: "int value for block".to_string(),
                value: Value {
                    typed: Some(Typed::Int32(12369760)),
                },
                expected_value: GraphValue::Int(12369760),
            },
            Case {
                name: "negative int value".to_string(),
                value: Value {
                    typed: Some(Typed::Int32(-12369760)),
                },
                expected_value: GraphValue::Int(-12369760),
            },
            Case {
                name: "big int".to_string(),
                value: Value {
                    typed: Some(Typed::Bigint("123".to_string())),
                },
                expected_value: GraphValue::BigInt(BigInt::from(123u64)),
            },
            Case {
                name: "big int > u64".to_string(),
                value: Value {
                    typed: Some(Typed::Bigint(
                        BigInt::from(u64::MAX).add(BigInt::from(1)).to_string(),
                    )),
                },
                expected_value: GraphValue::BigInt(BigInt::from(u64::MAX).add(BigInt::from(1))),
            },
            Case {
                name: "big decimal value".to_string(),
                value: Value {
                    typed: Some(Typed::Bigdecimal("3133363633312e35".to_string())),
                },
                expected_value: GraphValue::BigDecimal(BigDecimal::new(
                    BigInt::from(3133363633312u64),
                    35,
                )),
            },
            Case {
                name: "bool value".to_string(),
                value: Value {
                    typed: Some(Typed::Bool(true)),
                },
                expected_value: GraphValue::Bool(true),
            },
            Case {
                name: "string array".to_string(),
                value: Value {
                    typed: Some(Typed::Array(Array {
                        value: vec![
                            Value {
                                typed: Some(Typed::String("1".to_string())),
                            },
                            Value {
                                typed: Some(Typed::String("2".to_string())),
                            },
                            Value {
                                typed: Some(Typed::String("3".to_string())),
                            },
                        ],
                    })),
                },
                expected_value: GraphValue::List(vec!["1".into(), "2".into(), "3".into()]),
            },
        ];

        for case in cases.into_iter() {
            let value: GraphValue = decode_value(&case.value.typed.unwrap()).unwrap();
            assert_eq!(case.expected_value, value, "failed case: {}", case.name)
        }
    }
}
