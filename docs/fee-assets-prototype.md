# Fee assets prototype

This local network lets a wallet with only FUEL fund and sign a Script transaction. The signed `FeeAsset` policy selects a Change output. Its asset pays the gas charge and tip. Successful and reverted scripts return the unspent balance to that output. A validator gets each asset's inclusion price from the block's trailing Mint transactions.

## Run it

Cargo fetches the pinned [Fuel VM prototype](https://github.com/FuelLabs/fuel-vm/pull/1029). Install the `wasm32-unknown-unknown` Rust target for the node build. Run these commands from `fuel-core`.

```sh
cargo test -p fuel-core --lib mixed_fee_assets_replay_and_refund
cargo test -p fuel-core-tests --test integration_tests assemble_and_submit_with_only_fee_asset_coins
```

The first test produces one block containing an ETH payment, a successful FUEL payment and a reverted FUEL payment. It checks the change outputs and Mint amounts, then replays the block against an independent database initialized with the same coins. The second starts a local node with a funded test wallet, asks GraphQL for a quote, assembles and signs a payment, submits it and compares the finalized fee with the wallet's balance change.

Set `FUEL_ALWAYS_USE_WASM=1` when running the second command to exercise the embedded WebAssembly executor. The integration test `da_compression_restores_both_mint_pointers_and_block_id` checks the same block-ID property after a two-Mint DA round trip.

For your own local network, set the shell variable `FUEL_ASSET_ID` to the canonical FUEL ID for your network, run `fuel-core run --fee-asset-rate "${FUEL_ASSET_ID}=5"` and fund the signing address with that asset in genesis or with a transfer. The flag is repeatable. Here, 5 means five atomic units of the fee asset per atomic unit of the base asset. The test fixture uses a synthetic asset ID. The prototype activates at local STF version 35.

To inspect the next-block quote, send this query to the producer's GraphQL endpoint. After inclusion, `feeAssetId` and `feeInAsset` on the transaction status identify what the wallet spent.

```graphql
{
  estimateGasPrice(blockHorizon: "1") { gasPrice }
  feeAssetQuotes(blockHorizon: "1") { assetId rate gasPrice }
}
```

## Decisions to revisit

| Draft rules | Candidate change and tradeoff |
| --- | --- |
| V2, V5 | Keep the existing word encoding. Narrowing the index to `u8` saves zero wire bytes and restricts which output a wallet can select. The output-count check already bounds the index to `u16` in a valid transaction. Keeping the width check also rejects an invalid value when validating policies in isolation. |
| F5, GM 0x09 | A script can derive the fee asset from the policy bit, the output index and its asset ID, with a base-asset fallback. Dropping the convenience getter saves one VM selector but makes that sequence necessary for every caller. The prototype implements the getter. |
| P3–P7, G2 | A Mint records an inclusion price. A zero base price makes its exchange rate unrecoverable. A block with no FUEL payments also gives a follower no signal that the producer stopped accepting FUEL. Consider an explicit announcement with an exact rate and expiry or withdrawal, and keep historical settlement prices in Mints. Alternatively, publishing independent per-asset gas prices removes the on-chain ratio inference but requires a rule for comparing tips and reporting base-equivalent fees. |
| T3, T4, G3 | For fractional rates, specify how to round charges, affordable gas prices, tip comparisons and reported fees. Today the prototype settles the exact fee in the chosen asset, floors its base-equivalent value and attributes zero base value when the asset gas price is zero. It clamps an unrepresentable base-equivalent value to `u64::MAX`. Converting aggregate Mint revenue can differ by a few atomic units from summing individually rounded transaction fees. |
| B1, B9 | Each additional Mint consumes a transaction index and encoded bytes, including an announcement for an unused asset. The producer conservatively reserves an index per configured asset. Reserving only on first use could reclaim capacity in full blocks. Specify whether supplemental Mint bytes count toward the existing chargeable-transaction byte limit. |

## Boundaries of this milestone

The rate source is static and supports positive integer multipliers. The executor can produce and validate an explicit zero-amount announcement. Automatic announcements on first use of a rate or on rate changes, persistent announcement lookup for follower RPCs, the O2 read-only feed, decimal and margin handling, staleness rules and an upgrade rehearsal remain open. The static quote endpoint returns the locally configured supplemental assets and horizon prices. Exact asset and amount are available on finalized GraphQL statuses. Networks with existing clients also need the compatibility work for typed queries, protobuf exporters and downstream consumers described by the draft.
