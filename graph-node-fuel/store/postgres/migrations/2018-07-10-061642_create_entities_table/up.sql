-- Track database version.
-- This value is used to identify when the current database is in a format that
-- is incompatible with and cannot be upgraded to a new version of graph-node.
--
-- See migration 2019-01-07-120000 for an example of how to check the version
-- number.
CREATE TABLE db_version (
    db_version BIGINT PRIMARY KEY
);

-- Insert current version number
INSERT INTO db_version VALUES (2);


/**************************************************************
* CREATE TABLE
**************************************************************/
CREATE TABLE IF NOT EXISTS entities (
     id VARCHAR NOT NULL,
     subgraph VARCHAR NOT NULL,
     entity VARCHAR NOT NULL,
     data jsonb NOT NULL,
     event_source VARCHAR DEFAULT NULL,
     PRIMARY KEY (id, subgraph, entity)
 );
