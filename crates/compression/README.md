# Compression and decompression of transactions for the DA layer

## Compressed block header

Each compressed block begins with a version field, so that it's possible to change the format later.

## Temporal registry

This crate provides offchain registries for different types such as `AssetId`, `ContractId`, scripts, and predicates. Each registry is a key-value store with three-byte key. The registries are essentially compression caches. The three byte key allows cache size of 16 million values before reregistering the older values.

The registries allow replacing repeated objects with their respective keys, so if an object
is used multiple times in a short interval (couple of months, maybe), then the full value
exists on only a single uncompressed block.

### Fraud proofs

Compressed block will contain a merkle root over all compression smts, followed by newly registered values along with their keys. Using an SMT provides flexibility around the algorithm we use to define keys without knowing how exactly values were chosen to be registered.

Each registry also uses an SMT. Since the keys are three bytes long, the depth of the SMT is capped at 24 levels. 

## Compression of `UtxoIds`

Since each `UtxoId` only appears once, there's no point in registering them. Instead, they are replaced with `TxPointer` and output index, which are still unique.

### Fraud proofs

During fraud proofs we need to use the `prev_root` to prove that the referenced block height is part of the chain.
