-- Installs the pg_trgm extension for trigram indexing
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Build a partial index for each entity-attribute
CREATE OR REPLACE FUNCTION build_attribute_index(subgraph_id Text,index_name Text,index_type Text,index_operator Text,
        jsonb_index Boolean, attribute_name Text,entity_name Text) RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    jsonb_operator TEXT := '->>';
BEGIN
    IF jsonb_index THEN
      jsonb_operator := '->';
    END IF;
    EXECUTE 'CREATE INDEX ' || index_name
      || ' ON entities USING '
      || index_type
      || '((data -> '
      || quote_literal(attribute_name)
      || ' '
      || jsonb_operator
      || '''data'')'
      || index_operator
      || ') where subgraph='
      || quote_literal(subgraph_id)
      || ' and entity='
      || quote_literal(entity_name);
  RETURN ;
EXCEPTION
  WHEN duplicate_table THEN
      -- do nothing if index already exists
END;
$$;
