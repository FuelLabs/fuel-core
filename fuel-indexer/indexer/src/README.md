# Fuel Indexer

## Runtime Components:
1. database.rs - Database connections, schema management for metadata
  1. Database: interfaces ffi functions with database queries for indexer WASM
  1. SchemaManager: sets up, validates new graphql schemas
1. executor.rs - wasm runtime environment
  1. IndexEnv: holds references to objects that need to be available to ffi functions
  1. IndexExecutor: load WASM, execute event triggers
1. ffi.rs - functions callable from WASM, loading data structures to/from WASM
1. manifest.rs - the yaml format for a graphql instance
  1. namespace: The unique namespace this graphql schema lives in. This will correspond to the SQL database schema as well.
  1. graphql_schema: file path for the graphql schema.
  1. wasm_module: file path for the indexer WASM.
  1. handlers: list of mappings from event -> event_handler_name.
  1. list of test events to run through the indexer
1. schema.rs - SQL table schema builder

## Indexer Components
1. fuel-indexer/lib - Crate with traits/types used in a WASM indexer
1. fuel-indexer/derive - Derive macros to generate rust types from schema, and handler functions
1. fuel-indexer/schema - Crate for common types between runtime and indexer, sql types, serialization, etc.
