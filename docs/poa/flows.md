# Flows

## PoA Primary Production Flow
When the node is configured with a POA key, produce blocks and notify network.

```mermaid
sequenceDiagram
    participant POA as PoA Service
    participant BP as Block Producer
    participant TX as Transaction Pool
    participant R as Relayer
    participant E as Executor
    participant D as Database
    participant P2P as P2P
    
    POA->>BP: produce block
    BP->>TX: select_txs
    TX-->>BP: 
    BP->>R: get_da_height
    R-->>BP: 
    BP->>E: execute and store
    E->>D: save diff
    E-->>BP: 
    BP-->>POA: Malleated block
    POA->>R: get_poa_key
    R-->>POA: 
    POA->>POA: sign block
    POA->>D: insert_consensus_data
    POA->>P2P: broadcast_new_block
```

## PoA Synchronization Flow

When a node is behind peers, download the block data and catch up.

```mermaid
sequenceDiagram
    participant S as Synchronizer
    participant P2P as P2P
    participant POA as PoA Service
    participant BI as Block Importer
    participant R as Relayer
    participant E as Executor
    participant D as Database
    participant TX as Transaction Pool
    
    S->>+P2P: get_peer_heights
    P2P-->>-S: 
    alt is not behind peers
    S-->>S: sleep
    end
    S->>+P2P: get_missing_blocks
    P2P-->>-S: 
    S->>+POA: verify block signatures
    POA->>+R: await_synced + get_da_height
    R-->>-POA: 
    POA-->>-S: 
    S->>+BI: commit
    BI->>+R: check_da_height
    R-->>-BI: 
    BI->>+E: validate_and_store 
    E->>+D: save diff
    D-->>-E: 
    E-->>-BI: 
    BI->>+D: insert_consensus_data
    D-->>BI: 
    BI->>+TX: drop_committed_txs
    TX-->>-BI: 
    BI-->>-S: 
    
```

## PoA Gossip-Sync Flow

When a non-producer is synced to the PoA node, the synchronizer can switch to using gossip to capture newly finalized blocks.

```mermaid
sequenceDiagram
    participant S as Synchronizer
    participant POA as PoA Service
    participant P2P as P2P
    participant BI as Block Importer
    participant R as Relayer
    participant E as Executor
    participant D as Database
    participant TX as Transaction Pool
    
    S->>P2P: subscribe to block broadcasts
    P2P-->>S: new block event
    opt new block height != current height + 1
    note right of POA: drop gossiped block
    end
    S->>+POA: verify signed block header
    POA->>+R: await new block da height
    R-->>-POA: 
        note right of POA: verify signature against current authority key
    POA->>-S: 
    S->>+BI: commit sealed block
    BI->>+R: check_da_height for message inclusion
    R-->>-BI: 
    BI->>+E: validate_and_store 
    E->>+D: save diff
    D-->>-E: 
    E-->>-BI: 
    BI->>+D: insert_consensus_data
    D-->>BI: 
    BI->>+TX: drop_committed_txs
    TX-->>-BI: 
    BI-->>-S: 
```
