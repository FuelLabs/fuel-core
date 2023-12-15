-- metadata to track the DB schemas we create for subgraph deployments

-- The name of the DB schema corresponding to a deployment
create table deployment_schemas(
  id         serial primary key,
  subgraph   varchar unique not null,
  name       varchar not null default 'sgd' || currval('deployment_schemas_id_seq')
);

insert into deployment_schemas(id, subgraph, name)
  values(0, 'subgraphs', 'subgraphs');

-- Schema for the subgraph of subgraphs
-- Note that this schema does not have a entity_history table
-- because we do not collect history for the subgraph of subgraphs
create schema subgraphs;

create table subgraphs.entities (
  entity       varchar not null,
  id           varchar not null,
  data         jsonb,
  event_source varchar not null,
  primary key(entity, id)
);

insert into subgraphs.entities
  select entity, id, data, event_source
  from public.entities
  where subgraph='subgraphs';

delete from public.entities
  where subgraph='subgraphs';

--
-- Log entity history into sgdN.entities (without a subgraph field)
--
CREATE OR REPLACE FUNCTION public.subgraph_log_entity_event()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE
    event_id INTEGER;
    new_event_id INTEGER;
    is_reversion BOOLEAN := FALSE;
    operation_type INTEGER := 10;
    event_source  VARCHAR;
    entity VARCHAR;
    entity_id VARCHAR;
    data_before JSONB;
BEGIN
    -- Get operation type and source
    IF (TG_OP = 'INSERT') THEN
        operation_type := 0;
        event_source := NEW.event_source;
        entity := NEW.entity;
        entity_id := NEW.id;
        data_before := NULL;
    ELSIF (TG_OP = 'UPDATE') THEN
        operation_type := 1;
        event_source := NEW.event_source;
        entity := OLD.entity;
        entity_id := OLD.id;
        data_before := OLD.data;
    ELSIF (TG_OP = 'DELETE') THEN
        operation_type := 2;
        event_source := current_setting('vars.current_event_source', TRUE);
        entity := OLD.entity;
        entity_id := OLD.id;
        data_before := OLD.data;
    ELSE
        RAISE EXCEPTION 'unexpected entity row operation type, %', TG_OP;
    END IF;

    IF event_source = 'REVERSION' THEN
        is_reversion := TRUE;
    END IF;

    SELECT id INTO event_id
    FROM event_meta_data
    WHERE db_transaction_id = txid_current();

    new_event_id := null;

    IF event_id IS NULL THEN
        -- Log information on the postgres transaction for later use in
        -- revert operations
        INSERT INTO event_meta_data
            (db_transaction_id, db_transaction_time, source)
        VALUES
            (txid_current(), statement_timestamp(), event_source)
        RETURNING event_meta_data.id INTO new_event_id;
    END IF;

    -- Log row metadata and changes, specify whether event was an original
    -- ethereum event or a reversion
    EXECUTE format('INSERT INTO %I.entity_history
        (event_id, entity_id, entity,
         data_before, reversion, op_id)
      VALUES
        ($1, $2, $3, $4, $5, $6)', TG_TABLE_SCHEMA)
    USING COALESCE(new_event_id, event_id), entity_id, entity,
          data_before, is_reversion, operation_type;
    RETURN NULL;
END;
$function$
;
