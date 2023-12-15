/**************************************************************
* CREATE TABLES
**************************************************************/
-- Stores the metadata from each SQL transaction
CREATE TABLE IF NOT EXISTS event_meta_data (
    id SERIAL PRIMARY KEY,
    db_transaction_id BIGINT NOT NULL UNIQUE,
    db_transaction_time TIMESTAMP NOT NULL,
    op_id SMALLINT NOT NULL,
    source VARCHAR DEFAULT NULL
);

-- Stores the row level data and changes (1 or more per SQL transaction)
CREATE TABLE IF NOT EXISTS entity_history (
     id SERIAL PRIMARY KEY,
     event_id BIGINT NOT NULL,
     entity_id VARCHAR NOT NULL,
     subgraph VARCHAR NOT NULL,
     entity VARCHAR NOT NULL,
     data_before JSONB,
     data_after JSONB,
     reversion BOOLEAN NOT NULL DEFAULT FALSE
 );

 /**************************************************************
 * ADD FOREIGN KEYS
 **************************************************************/
 -- Define relationship between event_meta_data and entity_history
ALTER TABLE entity_history
    ADD CONSTRAINT entity_history_event_id_fkey
    FOREIGN KEY (event_id)
    REFERENCES event_meta_data(id)
    MATCH FULL
    ON DELETE CASCADE
    ON UPDATE CASCADE;
