## Fee calculations

### Storage fees

Fuel doesn't currently support separate fees for execution costs vs storage costs.
This is a planned future feature, but in the mean time we need a system for
compensating the network for storing contract data. The temporary solution is
to include an additional cost to op codes that write new data to storage or to
transactions that add new contracts to the chain.

There are a number of ways we might calculate this value; we have decided to go 
with a simple calculatoin based on our target storage growth and working
backward from there.

#### Pessimistic Estimate

If we arbitrarily set the target growth to 500 GB/year, we can ask, "what would 
be the gas price if users maxed out each block gas limit with storage?" 
This gives us this graph:

| bytes per year  | block gas limit | blocks per year | bytes per block | gas per bytes |
| --------------- | --------------- | --------------- | --------------- | ------------- |
| 500,000,000,000 |      10,000,000 |        31536000 |      **15,855** |    **630.72** |

This is a harsh estimate that isn't taking into account the additional base cost of tx
execution and the cost of any additional op codes. It is also assuming that 
all blocks would be maxing out the storage.

#### Generous Estimate

Additionally, this will only apply to our early networks, which won't be long-lived.
This allows us to take a bigger risk on the storage price and increase it over 
time to compensate for users adding a lot of data.

All this included, if we re-estimate the yearly storage limit as 5 TB we get:

| bytes per year    | block gas limit | blocks per year | bytes per block | gas per bytes |
| ----------------- | --------------- | --------------- | --------------- | ------------- |
| 5,000,000,000,000 |      10,000,000 |        31536000 |     **158,549** |     **63.07** |

Giving us a cost per byte of **63 gas** as our initial gas price for storage.

#### Additional justifications

If we compare our numbers to what Ethereum charges for gas, we get similar numbers.

If we had a pessimistic estimate for Ethereum and every block on Ethereum was full of new contract creations:

max_contract_size = 24,000 bytes
gas_per_max_contract = 32,000 + max_contract_size * 200 = 4,832,000

gas_per_block = 30,000,000
contracts_per_block = gas_per_block / gas_per_max_contract = ~6

blocks_per_year = 365 * 24 * 60 * ~5 = ~2628000

yearly_new_contract_bytes = blocks_per_year * contracts_per_block * max_contract_size = **378,432,000,000**

Which roughly lines up with our pessimistic estimate.

#### Conclusion

We'd recommend starting at something like **63 gas/byte**. If this is underpriced,
this will be tested in a testnet before being launched, hopefully giving us 
ample time to gather more data and update the price
