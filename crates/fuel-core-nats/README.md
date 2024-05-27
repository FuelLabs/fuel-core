#### receipts.{height}.{contract_id}.{topic}...

The data in a `LogData` receipt can be tagged for publishing to specified topics.

In Sway, this is done by wrapping the `log` message in a `Topic<T>` struct:

```rust
struct Topic<T> {
    header: u64,
    topics: [b256; 3],
    payload: T
}

impl<T> Topic<T> {
    fn new(topics: Vec<str>, payload: T) -> Topic<T> {
        assert(topics.len() <= 3);
        let mut i = 0;
        let mut result = [b256::min(); 3];
        while i < topics.len() {
            let topic = topics.get(i).unwrap();
            topic.as_ptr().copy_to::<u8>(__addr_of(result[i]), topic.len());
            i += 1;
        }
        Topic {
            header: 0x12345678,
            topics: result,
            payload
        }
    }
}
```

For example:

```rust
impl Counter for Contract {
   #[storage(read, write)]
    fn incr(amount: u64) -> u64 {
        let incremented = storage.counter.read() + amount;
        storage.counter.write(incremented);
        let mut topics = Vec::new();
        topics.push("counter");
        topics.push("incr");
        log(Topic::new(topics, incremented));
        incremented
    }
}
```

NATS Publisher recognizes the predefined prefix, unwraps the data, and publishes it. The data published from the `incr` call above is the 8 bytes of the `u64` incremented value.

```
nats sub "receipts.*.*.counter.>" --last --headers-only
```

```
12:38:41 Subscribing to JetStream Stream holding messages with subject receipts.*.*.counter.> starting with the last message received
[#1] Received JetStream message: stream: fuel seq 68 / subject: receipts.6.0000000000000000000000000000000000000000000000000000000000000000.counter.incr / time: 2024-05-20T12:06:59+02:00
Nats-Msg-Size: 8
```

#### receipts.{height}.{contract_id}.{kind}

```json
{
  "ScriptResult": {
    "result": "Success",
    "gas_used": 636
  }
}
```

#### blocks.{height}

```json
{
  "V1": {
    "header": {
      "V1": {
        "application": {
          "da_height": 5927820,
          "consensus_parameters_version": 1,
          "state_transition_bytecode_version": 0,
          "generated": {
            "transactions_count": 1,
            "message_receipt_count": 0,
            "transactions_root": "d201d5b2ace9dd0c319411f49f0447a38fca0991250c5e0657b53c6831a9faa5",
            "message_outbox_root": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "event_inbox_root": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
          }
        },
        "consensus": {
          "prev_root": "62678e891bb5181f33cf62f81feb147dec407e87d5e9b3c826ea9dd3c722d3a7",
          "height": 1249127,
          "time": [
            64,
            0,
            0,
            0,
            102,
            72,
            158,
            33
          ],
          "generated": {
            "application_hash": "122bcbc1a4a28586722e9b48db213c2d208b22f1a06f3ecd38eb0e2d8e8cf0da"
          }
        }
      }
    },
    "transactions": [
      {
        "Mint": {
          "tx_pointer": {
            "block_height": 1249127,
            "tx_index": 0
          },
          "input_contract": {
            "utxo_id": {
              "tx_id": "5206082b3d7f71595c5c02b830469cad0145d1149ba5a764343eaf000251a80e",
              "output_index": 0
            },
            "balance_root": "8b8d7c5dfa3e7caf6efd8e697616f78e04a5ccdfc7607bf017e7a3cff2be1bae",
            "state_root": "0000000000000000000000000000000000000000000000000000000000000000",
            "tx_pointer": {
              "block_height": 1249126,
              "tx_index": 0
            },
            "contract_id": "7777777777777777777777777777777777777777777777777777777777777777"
          },
          "output_contract": {
            "input_index": 0,
            "balance_root": "8b8d7c5dfa3e7caf6efd8e697616f78e04a5ccdfc7607bf017e7a3cff2be1bae",
            "state_root": "0000000000000000000000000000000000000000000000000000000000000000"
          },
          "mint_amount": 0,
          "mint_asset_id": "f8f8b6283d7fa5b672b530cbb84fcccb4ff8dc40f8176ef4544ddb1f1952ad07",
          "gas_price": 1
        }
      }
    ]
  }
}
```

#### transactions.{height}.{index}.{kind}

```json
{
  "Mint": {
    "tx_pointer": {
      "block_height": 1249127,
      "tx_index": 0
    },
    "input_contract": {
      "utxo_id": {
        "tx_id": "5206082b3d7f71595c5c02b830469cad0145d1149ba5a764343eaf000251a80e",
        "output_index": 0
      },
      "balance_root": "8b8d7c5dfa3e7caf6efd8e697616f78e04a5ccdfc7607bf017e7a3cff2be1bae",
      "state_root": "0000000000000000000000000000000000000000000000000000000000000000",
      "tx_pointer": {
        "block_height": 1249126,
        "tx_index": 0
      },
      "contract_id": "7777777777777777777777777777777777777777777777777777777777777777"
    },
    "output_contract": {
      "input_index": 0,
      "balance_root": "8b8d7c5dfa3e7caf6efd8e697616f78e04a5ccdfc7607bf017e7a3cff2be1bae",
      "state_root": "0000000000000000000000000000000000000000000000000000000000000000"
    },
    "mint_amount": 0,
    "mint_asset_id": "f8f8b6283d7fa5b672b530cbb84fcccb4ff8dc40f8176ef4544ddb1f1952ad07",
    "gas_price": 1
  }
}
```