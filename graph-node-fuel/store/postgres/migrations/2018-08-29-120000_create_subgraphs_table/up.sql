/**************************************************************
* CREATE TABLE
**************************************************************/
-- Stores list of immutable subgraphs
CREATE TABLE IF NOT EXISTS subgraphs (
    id VARCHAR PRIMARY KEY,
    network_name VARCHAR NOT NULL,
    latest_block_hash VARCHAR NOT NULL,
    latest_block_number BIGINT NOT NULL
);
