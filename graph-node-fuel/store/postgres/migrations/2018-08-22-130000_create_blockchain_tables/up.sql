/**************************************************************
* CREATE TABLES
**************************************************************/
-- Stores list of Ethereum networks (e.g. mainnet, ropsten) and optional pointer to head block
CREATE TABLE IF NOT EXISTS ethereum_networks (
    name VARCHAR PRIMARY KEY,
    head_block_hash VARCHAR,
    head_block_number BIGINT,
    CHECK ((head_block_hash IS NULL) = (head_block_number IS NULL))
);

-- Stores block data from each Ethereum network
CREATE TABLE IF NOT EXISTS ethereum_blocks (
    hash VARCHAR PRIMARY KEY,
    number BIGINT NOT NULL,
    parent_hash VARCHAR NOT NULL,
    network_name VARCHAR NOT NULL REFERENCES ethereum_networks (name),
    data JSONB NOT NULL
);
