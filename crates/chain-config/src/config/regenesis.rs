use fuel_core_types::fuel_tx::{ConsensusParameters, GasCosts};
use fuel_core_types::fuel_types::BlockHeight;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserializer;
use std::str::FromStr;
use std::sync::mpsc::SyncSender;

use crate::{ConsensusConfig, CoinConfig, ContractConfig, MessageConfig};

trait ProcessEntry {
    fn process(&self, event: Event);
    fn clone_me(&self) -> Box<dyn ProcessEntry>;
}

pub struct ChainConfigVisitor {
    pub callback: EventHandler,
}

struct StateConfigVisitor {
    callback: EventHandler,
}

impl<'de> Visitor<'de> for StateConfigVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "StateConfigVisitor")
    }
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while let Some(key) = map.next_key::<String>().unwrap() {
            match key.as_str() {
                "coins" => {
                    map.next_value_seed(SeqDeserializator {
                        visitor: CoinConfigVisitor {
                            callback: self.callback.clone(),
                        },
                    })?;
                }
                "contracts" => {
                    map.next_value_seed(SeqDeserializator {
                        visitor: ContractConfigVisitor {
                            callback: self.callback.clone(),
                        },
                    })?;
                }
                "messages" => {
                    map.next_value_seed(SeqDeserializator {
                        visitor: MessageConfigVisitor {
                            callback: self.callback.clone(),
                        },
                    })?;
                }
                "height" => {
                    let height_str = map.next_value::<String>()?;
                    let height = BlockHeight::from_str(&height_str).unwrap();
                    self.callback.process(Event::BlockHeight(height))
                }
                _ => {
                    todo!("See about unexpected keys")
                }
            }
        }
        Ok(())
    }
}

struct SeqDeserializator<T> {
    visitor: T,
}

impl<'de, T: Visitor<'de>> DeserializeSeed<'de> for SeqDeserializator<T> {
    type Value = T::Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(self.visitor)
    }
}

macro_rules! make_visitors {
    ($( ($visitor_name: ident, $event_name: ident) ),*) => {
        $(
        struct $visitor_name {
            callback: EventHandler,
        }
        impl<'de> Visitor<'de> for $visitor_name {
            type Value = ();

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "{}", stringify!($visitor_name))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                while let Some(event) = seq.next_element::<$event_name>()? {
                    self.callback.process(Event::$event_name(event))
                }

                Ok(())
            }
        }
        )*
    };
}
make_visitors!(
    (ContractConfigVisitor, ContractConfig),
    (CoinConfigVisitor, CoinConfig),
    (MessageConfigVisitor, MessageConfig)
);

struct InitialStateDeser {
    callback: EventHandler,
}

impl<'de> DeserializeSeed<'de> for InitialStateDeser {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(StateConfigVisitor {
            callback: self.callback,
        })
    }
}

impl<'de> Visitor<'de> for ChainConfigVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "ChainConfigVisitor")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while let Some(key) = MapAccess::next_key::<String>(&mut map)? {
            match key.as_str() {
                "chain_name" => {
                    let chain_name = map.next_value().unwrap();
                    self.callback.process(Event::ChainName(chain_name))
                }
                "block_gas_limit" => {
                    let block_gas_limit = map.next_value().unwrap();
                    self.callback.process(Event::BlockGasLimit(block_gas_limit))
                }
                "initial_state" => {
                    map.next_value_seed(InitialStateDeser {
                        callback: self.callback.clone(),
                    })?;
                }
                "transaction_parameters" => {
                    let transaction_parameters = map.next_value().unwrap();
                    self.callback
                        .process(Event::ConsensusParameters(transaction_parameters))
                }
                "gas_costs" => {
                    let gas_costs = map.next_value().unwrap();
                    self.callback.process(Event::GasCosts(gas_costs))
                }
                "consensus" => {
                    let consensus = map.next_value().unwrap();
                    self.callback.process(Event::ConsensusConfig(consensus))
                }
                unexpected => {
                    eprintln!("Didn't expect key: {unexpected}");
                    todo!("We should either error here or read the type using the Any deserializator that will ignore the read value");
                }
            };
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct EventHandler {
    pub sender: SyncSender<Event>,
}

#[derive(Debug)]
pub enum Event {
    ChainName(String),
    BlockGasLimit(u64),
    ConsensusParameters(ConsensusParameters),
    GasCosts(GasCosts),
    ConsensusConfig(ConsensusConfig),
    CoinConfig(CoinConfig),
    ContractConfig(ContractConfig),
    MessageConfig(MessageConfig),
    BlockHeight(BlockHeight),
}

impl EventHandler {
    fn process(&self, event: Event) {
        self.sender.send(event).unwrap()
    }
}
