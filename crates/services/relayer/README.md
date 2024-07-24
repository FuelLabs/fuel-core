
# Relayer

The Relayer connects Fuel to the DA (data availability) layer contract.
The primary functionality is to track the finality of blocks in the DA layer and download all log events for blocks that are considered final.

## Validity, finality and synchronization

Ethereum blocks are considered final after two epochs. Each epoch contains 32 slots. A slot takes 12s so epoch is around 6.4min and two epochs will take 12.8min. After 13min we are sure that our deposits will not be reverted by some big reorganization. So we are okay to say that everything older then ~100blocks is finalized.

Second finality that we have is related to fuel block attestation time limit, how long are we going to wait until challenge comes. It should be at least longer than ethereum finality. Not relevant for first version.

* Problem: Validator deposit to ethereum gets reverted by block reorg. (Eth clients usually have priority for reverted txs but this does not mean it can't happen). It can potentially rearrange order of transactions
* Solution: Introduce sliding window, only deposits that are at least eth finality long can be finalized and included in validators leader selection.

* Problem: How to choose when bridge message event gets enabled for use in fuel, at what exact fuel block does this happen? (Note that we have sliding window)
* Solution: introduce `da_height` variable inside fuel block header that will tell at what block we are including token deposits.

## Implementation
The `RelayerHandle` type is used to start / stop a background task that runs the actual `Relayer`.

The relayer background task is a simple loop that polls the fuel database and DA node to build a picture of the current state.
If the state determines that the relayer is out of sync with the DA layer then log messages from the DA contract are downloaded and written to the database.
The range of blocks is `(last_downloaded_height + 1)..=current_finalized_height`.

Logs are paginated into sets of blocks to avoid overloading a single rpc call.
