CREATE OR REPLACE FUNCTION revert_entity_event(entity_history_id INTEGER, operation_id INTEGER)
    RETURNS VOID AS
$$
DECLARE
    target_entity_id VARCHAR;
    target_subgraph VARCHAR;
    target_entity VARCHAR;
    target_data_before JSONB;
    reversion_identifier VARCHAR;
BEGIN
    -- Get entity history event information and save into the declared variables
    SELECT
        entity_id,
        subgraph,
        entity,
        data_before
    INTO
        target_entity_id,
        target_subgraph,
        target_entity,
        target_data_before
    FROM entity_history
    WHERE entity_history.id = entity_history_id;

    reversion_identifier := 'REVERSION';

    CASE
        -- INSERT case
        WHEN operation_id = 0 THEN
            -- Delete inserted row
            BEGIN
                PERFORM set_config('vars.current_event_source', 'REVERSION', FALSE);
                EXECUTE
                    'DELETE FROM entities WHERE (
                        subgraph = $1 AND
                        entity = $2 AND
                        id = $3)'
                USING target_subgraph, target_entity, target_entity_id;

                -- Row was already updated
                EXCEPTION
                    WHEN no_data_found THEN
                        NULL;
            END;

        -- UPDATE or DELETE case
        WHEN operation_id IN (1,2) THEN
            -- Insert deleted row if not exists
            -- If row exists perform update
            BEGIN
                EXECUTE
                    'INSERT INTO entities (id, subgraph, entity, data, event_source)
                        VALUES ($1, $2, $3, $4, $5)
                        ON CONFLICT (id, subgraph, entity) DO UPDATE
                        SET data = $4, event_source = $5'
                USING
                    target_entity_id,
                    target_subgraph,
                    target_entity,
                    target_data_before,
                    reversion_identifier;
            END;
    END CASE;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION revert_transaction(event_id_to_revert INTEGER)
    RETURNS VOID AS
$$
DECLARE
    entity_history_row RECORD;
BEGIN
    -- Loop through each record change event
    FOR entity_history_row IN
        -- Get all entity changes driven by given event
        SELECT
            id,
            op_id
        FROM entity_history
        WHERE (
            subgraph <> 'subgraphs' AND
            event_id = event_id_to_revert)
        ORDER BY id DESC
    -- Iterate over entity changes and revert each
    LOOP
        PERFORM revert_entity_event(entity_history_row.id, entity_history_row.op_id);
    END LOOP;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION revert_block(block_to_revert_hash VARCHAR, block_to_revert_number BIGINT, target_block_hash VARCHAR, subgraph_id VARCHAR)
    RETURNS VOID AS
$$
DECLARE
    event_row RECORD;
    entity_row RECORD;
BEGIN
    -- Revert all relevant events
    FOR event_row IN
        -- Get all events associated with the given block
        SELECT
            entity_history.event_id AS event_id
        FROM entity_history
        JOIN event_meta_data ON
            entity_history.event_id = event_meta_data.id
        WHERE event_meta_data.source = block_to_revert_hash AND
            entity_history.subgraph = subgraph_id
        GROUP BY
            entity_history.event_id
        ORDER BY entity_history.event_id DESC
    LOOP
        PERFORM revert_transaction(event_row.event_id::integer);
    END LOOP;
END;
$$ LANGUAGE plpgsql;

DROP VIEW entity_history_with_source;

DROP TRIGGER IF EXISTS entity_change_insert_trigger ON entities;
DROP TRIGGER IF EXISTS entity_change_update_trigger ON entities;
DROP TRIGGER IF EXISTS entity_change_delete_trigger ON entities;
CREATE TRIGGER after_entity_change_trigger
  AFTER INSERT OR UPDATE OR DELETE
  ON entities
  FOR EACH ROW
  EXECUTE PROCEDURE log_entity_event();
