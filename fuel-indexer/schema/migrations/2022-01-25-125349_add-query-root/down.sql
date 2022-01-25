-- This file should undo anything in `up.sql`
DROP TABLE graph_registry.graph_root;

ALTER TABLE graph_registry.columns
DROP COLUMN graphql_type;
