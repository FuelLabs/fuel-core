/**************************************************************
* UPDATE HISTORY TRIGGER FUNCTIONS
*
* History trigger functions have been updated to store the row
* level operation type (insert, update, or delete). Now the op_id
* is stored in the the entity_history table (moved from event_meta_data).
*
* The history triggers and trigger functions have been consolidated
* to a single function for all operations with logic to store the
* database event differently based on which operation fired the trigger (TG_OP).
**************************************************************/

/**************************************************************
* ALTER SCHEMA TO STORE ROW LEVEL OPERATION TYPE
**************************************************************/
ALTER TABLE entity_history
ADD COLUMN IF NOT EXISTS op_id SMALLINT NOT NULL
DEFAULT 3;

ALTER TABLE event_meta_data
DROP COLUMN IF EXISTS op_id;

UPDATE entity_history
SET op_id = 0
WHERE data_before IS NULL;

UPDATE entity_history
SET op_id = 2
WHERE data_after IS NULL;

UPDATE entity_history
SET op_id = 1
WHERE data_before IS NOT NULL AND
    data_after IS NOT NULL;

ALTER TABLE entity_history
ALTER COLUMN op_id
DROP DEFAULT;

CREATE INDEX IF NOT EXISTS entity_history_event_id_btree_idx
ON entity_history
USING BTREE (event_id);

/**************************************************************
* LOG ENTITY CHANGES
*
* Writes row level metadata and before & after state of `data` to entity_history
* Called when after_update_trigger is fired.
* Logs information after all insert, update, delete events
* Revert events are marked with the reversion boolean field
**************************************************************/
CREATE OR REPLACE FUNCTION log_entity_event()
    RETURNS trigger AS
$$
DECLARE
    event_id INTEGER;
    new_event_id INTEGER;
    is_reversion BOOLEAN := FALSE;
    operation_type INTEGER := 10;
    event_source  VARCHAR;
    subgraph VARCHAR;
    entity VARCHAR;
    entity_id VARCHAR;
    data_before JSONB;
    data_after JSONB;
BEGIN
    -- Get operation type and source
    IF (TG_OP = 'INSERT') THEN
        operation_type := 0;
        event_source := NEW.event_source;
        subgraph := NEW.subgraph;
        entity := NEW.entity;
        entity_id := NEW.id;
        data_before := NULL;
        data_after := NEW.data;
    ELSIF (TG_OP = 'UPDATE') THEN
        operation_type := 1;
        event_source := NEW.event_source;
        subgraph := OLD.subgraph;
        entity := OLD.entity;
        entity_id := OLD.id;
        data_before := OLD.data;
        data_after := NEW.data;
    ELSIF (TG_OP = 'DELETE') THEN
        operation_type := 2;
        event_source := current_setting('vars.current_event_source', TRUE);
        subgraph := OLD.subgraph;
        entity := OLD.entity;
        entity_id := OLD.id;
        data_before := OLD.data;
        data_after := NULL;
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
        -- Log information on the postgres transaction for later use in revert operations
        INSERT INTO event_meta_data
            (db_transaction_id, db_transaction_time, source)
        VALUES
            (txid_current(), statement_timestamp(), event_source)
        RETURNING event_meta_data.id INTO new_event_id;
    END IF;

    -- Log row metadata and changes, specify whether event was an original ethereum event or a reversion
    INSERT INTO entity_history
        (event_id, entity_id, subgraph, entity, data_before, data_after, reversion, op_id)
    VALUES
        (COALESCE(new_event_id, event_id), entity_id, subgraph, entity, data_before, data_after, is_reversion, operation_type);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

/**************************************************************
* CREATE ENTITY CHANGE TRIGGER
**************************************************************/
CREATE TRIGGER after_entity_change_trigger
    AFTER INSERT OR UPDATE OR DELETE
    ON entities
    FOR EACH ROW
    EXECUTE PROCEDURE log_entity_event();


/**************************************************************
* DROP PREVIOUS VERSION OF HISTORY TRIGGERS
**************************************************************/

/**************************************************************
* DROP EXISTING HISTORY TRIGGERS
**************************************************************/
DROP TRIGGER after_insert_trigger ON entities;
DROP TRIGGER after_update_trigger ON entities;
DROP TRIGGER after_delete_trigger ON entities;

/**************************************************************
* DROP EXISTING HISTORY FUNCTIONS
**************************************************************/
DROP FUNCTION log_insert();
DROP FUNCTION log_update();
DROP FUNCTION log_delete();
