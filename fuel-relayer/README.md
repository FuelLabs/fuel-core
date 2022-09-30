
# Relayer

Functionalities expected from Relayer:

* Bridge:
  * Receive message

## Validity, finality and synchronization

With Ethereum's The Merge coming in few months we are gaining finality of blocks in ethereum after two epochs, epoch contains 32 slots, slot takes 12s so epoch is around 6,4min and two epochs are 12.8min, so after 13min we are sure that our deposits will not be reverted by some big reorganization. So we are okay to say that everything older then ~100blocks are finalized.

Second finality that we have is related to fuel block attestation time limit, how long are we going to wait until challenge comes. It should be at least longer than ethereum finality. Not relevant for first version.

* Problem: Validator deposit to ethereum gets reverted by block reorg. (Eth clients usually have priority for reverted txs but this does not mean it cant happen). It can potentially rearrange order of transactions
* Solution: Introduce sliding window, only deposits that are at least eth finality long can be finalized and included in validators leader selection. We will need to have pending events before they are merged and handle reorgs.

Example of sliding window:
![Sliding Window](../docs/diagrams/fuel_v2_relayer_sliding_window.jpg)

* Problem: How to choose when bridge message event gets enabled for use in fuel, at what exact fuel block does this happen? (Note that we have sliding window)
* Solution: introduce `da_height` variable inside fuel block header that will tell at what block we are including validator delegation/withdrawal and token deposits.

There are few rules that `da_height` (da as data availability) need to follow and can be enforced with v2 contract:

* Last block `da_height` needs to be smaller number then current one.
* `da_height` shouldn't be greater then `best_da_block`-`finalization_slider`

In most cases for fuel block `da_height` can be calculated as `current_da_height-1`

### Bridge

Bridging is currently WIP and it is moving as generalizes messages between smart contract gateway.

## Contract Flake
The `flake.nix` in this repo has a tool to fetch, compile and generate the abi for the fuel contracts.
While it is not mandatory to use nix to do this the advantage is the exact versions of the contracts that were used to generate the abi files is pinned in the `flake.lock` file.

### Usage
To generate the api files run the following from the `fuel-relayer` directory.
```bash
nix run .#generate-abi-json abi
```
To update the version of the contracts that is used run:
```bash
nix flake update
nix run .#generate-abi-json abi
```
You can see the versions that are pinned by running:
```bash
nix flake info
```