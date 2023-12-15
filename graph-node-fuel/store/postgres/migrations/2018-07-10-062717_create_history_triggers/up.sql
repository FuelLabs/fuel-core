/**************************************************************
* CREATE TRIGGER FUNCTIONS
**************************************************************/

/**************************************************************
* LOG UPDATE
*
* Writes row level metadata and before & after state of `data` to entity_history
* Called when after_update_trigger is fired.
* Logs information after all insert, update, delete events
* Revert events are marked with the reversion boolean field
**************************************************************/
CREATE OR REPLACE FUNCTION log_update()
    RETURNS trigger AS
$$
DECLARE
    event_id INTEGER;
    new_event_id INTEGER;
    is_reversion BOOLEAN;
BEGIN
    -- Sets the is_reversion variable for differentiating between Ethereum events and block reorg events
    IF NEW.event_source = 'REVERSION' THEN
        is_reversion := TRUE;
    ELSE
        is_reversion := FALSE;
    END IF;

    SELECT id INTO event_id
    FROM event_meta_data
    WHERE db_transaction_id = txid_current();

    new_event_id := null;

    IF event_id IS NULL THEN
        -- Log information on the postgres transaction for later use in revert operations
        INSERT INTO event_meta_data
            (db_transaction_id, db_transaction_time, op_id, source)
        VALUES
            (txid_current(), statement_timestamp(), 1, NEW.event_source)
        RETURNING event_meta_data.id INTO new_event_id;
    END IF;

    -- Log row metadata and changes, specify whether event was an original ethereum event or a reversion
    INSERT INTO entity_history
        (event_id, entity_id, subgraph, entity, data_before, data_after, reversion)
    VALUES
        (COALESCE(new_event_id, event_id), OLD.id, OLD.subgraph, OLD.entity, OLD.data, NEW.data, is_reversion);

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

/**************************************************************
* LOG INSERT
*
* Writes out newly inserted entity to entity_history
* Called when after_insert_trigger is fired.
**************************************************************/
CREATE OR REPLACE FUNCTION log_insert()
    RETURNS trigger AS
$$
DECLARE
    temp_event_id INTEGER;
    event_id INTEGER;
    new_event_id INTEGER;
    is_reversion BOOLEAN;
BEGIN
    -- Sets the is_reversion variable for differentiating between Ethereum events and block reorg events
    IF NEW.event_source = 'REVERSION' THEN
        is_reversion := TRUE;
    ELSE
        is_reversion := FALSE;
    END IF;

    SELECT id INTO event_id
    FROM event_meta_data
    WHERE db_transaction_id = txid_current();

    new_event_id := null;

    IF event_id IS NULL THEN
        -- Log information on the postgres transaction for later use in revert operations
        INSERT INTO event_meta_data
            (db_transaction_id, db_transaction_time, op_id, source)
        VALUES
            (txid_current(), statement_timestamp(), 0, NEW.event_source)
        RETURNING event_meta_data.id INTO new_event_id;
    END IF;

    -- Log inserted row
    INSERT INTO entity_history
        (event_id, entity_id, subgraph, entity, data_before, data_after, reversion)
    VALUES
        (COALESCE(new_event_id, event_id), NEW.id, NEW.subgraph, NEW.entity, NULL, NEW.data, is_reversion);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

/**************************************************************
* LOG DELETE
*
* Writes deleted entity to entity_history
* Called when after_delete_trigger is fired.
**************************************************************/
CREATE OR REPLACE FUNCTION log_delete()
    RETURNS trigger AS
$$
DECLARE
    event_id INTEGER;
    current_event_source  VARCHAR;
    new_event_id INTEGER;
    is_reversion BOOLEAN;
BEGIN
    -- Use session level setting to get the event_source for the current transaction
    current_event_source := current_setting('vars.current_event_source', TRUE);

    -- Sets the is_reversion variable for differentiating between Ethereum events and block reorg events
    IF (
      current_event_source = 'REVERSION'
    )
    THEN
        is_reversion := TRUE;
    ELSE
        is_reversion := FALSE;
    END IF;

    SELECT id INTO event_id
    FROM event_meta_data
    WHERE db_transaction_id = txid_current();

    new_event_id := null;

    IF event_id IS NULL THEN
        -- Log information on the postgres transaction for later use in revert operations
        INSERT INTO event_meta_data
            (db_transaction_id, db_transaction_time, op_id, source)
        VALUES
            (txid_current(), statement_timestamp(), 2, current_event_source)
        RETURNING event_meta_data.id INTO new_event_id;
    END IF;

    -- Log content of deleted entity
    INSERT INTO entity_history
        (event_id, entity_id, subgraph, entity, data_before, data_after, reversion)
    VALUES
        (COALESCE(new_event_id, event_id), OLD.id, OLD.subgraph, OLD.entity, OlD.data, NULL, is_reversion);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

/**************************************************************
* CREATE TRIGGERS
**************************************************************/
CREATE TRIGGER after_insert_trigger
    AFTER INSERT
    ON entities
    FOR EACH ROW
    EXECUTE PROCEDURE log_insert();

CREATE TRIGGER after_update_trigger
    AFTER UPDATE
    ON entities
    FOR EACH ROW
    EXECUTE PROCEDURE log_update();

CREATE TRIGGER after_delete_trigger
    AFTER DELETE
    ON entities
    FOR EACH ROW
    EXECUTE PROCEDURE log_delete();
