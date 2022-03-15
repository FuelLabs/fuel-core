-- This file should undo anything in `up.sql`
DROP TABLE graph_registry.graph_root;

ALTER TABLE graph_registry.columns
DROP COLUMN graphql_type;

ALTER TYPE ColumnTypeName DROP VALUE 'Int4';
ALTER TYPE ColumnTypeName DROP VALUE 'Int8';
ALTER TYPE ColumnTypeName DROP VALUE 'UInt4';
ALTER TYPE ColumnTypeName DROP VALUE 'UInt8';
ALTER TYPE ColumnTypeName DROP VALUE 'Timestamp';
