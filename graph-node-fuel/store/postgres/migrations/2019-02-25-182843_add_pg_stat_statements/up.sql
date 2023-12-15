-- The extension requires that the pg_stat_statements DSO is loaded
-- via shared_preload_libraries. Luckily, this is only needed when
-- trying to read from the pg_stat_statements view. Extension
-- creation will succeed either way
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;
