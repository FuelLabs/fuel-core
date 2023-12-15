/**************************************************************
* CREATE REVERT FUNCTIONS
**************************************************************/

/**************************************************************
* REVERT ROW EVENT
*
* Revert a specific row level event
* Parameters: entity_history.id (primary key)
*             operation_id
**************************************************************/
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

/**************************************************************
* REVERT TRANSACTION
*
* Get all row level events associated with a SQL transaction
* For each row level event call revert_entity_event()
* Parameters: event_id
**************************************************************/
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
            entity_history.id as id,
            event_meta_data.op_id as op_id
        FROM entity_history
        JOIN event_meta_data ON
            event_meta_data.id=entity_history.event_id
        WHERE event_meta_data.id = event_id_to_revert
        ORDER BY entity_history.id DESC
    -- Iterate over entity changes and revert each
    LOOP
        PERFORM revert_entity_event(entity_history_row.id, entity_history_row.op_id);
    END LOOP;
END;
$$ LANGUAGE plpgsql;

/**************************************************************
* REVERT TRANSACTION GROUP
*
* Get all row level events associated with a set of SQL transactions
* For each row level event call revert_entity_event()
* Parameters: array of event_id's
**************************************************************/
CREATE OR REPLACE FUNCTION revert_transaction_group(event_ids_to_revert INTEGER[])
    RETURNS VOID AS
$$
DECLARE
    entity_history_row RECORD;
BEGIN
    FOR entity_history_row IN
        SELECT
            entity_history.id as id,
            event_meta_data.op_id as op_id
        FROM entity_history
        JOIN event_meta_data ON
            event_meta_data.id=entity_history.event_id
        WHERE event_meta_data.id = ANY(event_id_to_revert)
        ORDER BY entity_history.id DESC
    LOOP
        PERFORM revert_entity_event(row.id, row.op_id);
    END LOOP;
END;
$$ LANGUAGE plpgsql;

/**************************************************************
* RERUN ROW EVENT
*
* Rerun a specific row level event
* Parameters: entity_history pkey (id) and operation type
**************************************************************/
CREATE OR REPLACE FUNCTION rerun_entity_history_event(entity_history_id INTEGER, operation_id INTEGER)
    RETURNS VOID AS
$$
DECLARE
    target_entity_id VARCHAR;
    target_subgraph VARCHAR;
    target_entity VARCHAR;
    target_data_after JSONB;
BEGIN
    SELECT
        entity_id,
        subgraph,
        entity,
        data_before,
        data_after
    INTO
        target_entity_id,
        target_subgraph,
        target_entity,
        target_data_after
    FROM entity_history
    WHERE entity_history.id = entity_history_id;

    CASE
        -- INSERT or UPDATE case
        WHEN operation_id IN (0,1) THEN
            -- Re insert row
            -- If row exists perform update
            BEGIN
                EXECUTE
                    'INSERT INTO entities (subgraph, entity, id, data, event_source)
                        VALUES ($1, $2, $3, $4, "REVERSION")
                        ON CONFLICT (subgraph, entity, id) DO UPDATE
                        SET data = $4, event_source = NULL'
                USING
                    target_subgraph,
                    target_entity,
                    target_entity_id,
                    target_data_after;
            END;

        -- DELETE case
        WHEN operation_id = 2 THEN
            -- Delete entity
            BEGIN
                -- Set event source as "REVERSION"
                PERFORM set_config('vars.current_event_source', 'REVERSION', FALSE);
                EXECUTE
                    'DELETE FROM entities WHERE (
                        subgraph = $1 AND
                        entity = $2 AND
                        id = $3)'
                USING
                    target_subgraph,
                    target_entity,
                    target_entity_id;
            END;
    END CASE;
END;
$$ LANGUAGE plpgsql;

/**************************************************************
* RERUN ENTITY
*
* Rerun all events for a specific entity
* avoiding any revert or uncled events
* Parameters: entity pkey -> (entity_id, subgraph, entity)
              event_id of revert event
**************************************************************/
CREATE OR REPLACE FUNCTION rerun_entity(
    event_id_to_rerun INTEGER, subgraph_to_rerun VARCHAR, entity_to_rerun VARCHAR, entity_id_to_rerun VARCHAR)
    RETURNS VOID AS
$$
DECLARE
    entity_history_event RECORD;
BEGIN
     FOR entity_history_event IN
        -- Get all events that effect given entity and come after given event
        SELECT
            entity_history.id as id,
            event_meta_data.op_id as op_id
        FROM entity_history
        JOIN event_meta_data ON
            event_meta_data.id = entity_history.event_id
        WHERE (
            entity_history.entity = entity_to_rerun AND
            entity_history.entity_id = entity_id_to_rerun AND
            entity_history.subgraph = subgraph_to_rerun
            AND
            entity_history.event_id > event_i_to_rerund
            AND
            entity_history.reversion = FALSE )
        ORDER BY entity_history.id ASC
    LOOP
        -- For each event rerun the operation
        PERFORM rerun_entity_history_event(entity_history_event.id, entity_history_event.op_id);
    END LOOP;
END;
$$ LANGUAGE plpgsql;

/**************************************************************
* REVERT BLOCK
*
* Revert the row store events related to a particular block
* Rerun all of an entities changes that come after the row store events related to that block
**************************************************************/
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

/**************************************************************
* REVERT BLOCK GROUP
*
* Revert the row store events related to a set of blocks
* for each block in the set run the revert block function
* Parameters: array of block_hash's, subgraph
**************************************************************/
CREATE OR REPLACE FUNCTION revert_block_group(block_hash_group VARCHAR[], subgraph_id VARCHAR)
    RETURNS VOID AS
$$
DECLARE
    block_row RECORD;
    event_row RECORD;
    entity_row RECORD;
BEGIN
    FOR block_row IN
        SELECT
            source
        FROM event_meta_data
        WHERE source = ANY(block_hash_group)
        GROUP BY source
        ORDER BY id DESC
    LOOP
        FOR event_row IN
            SELECT
                entity_history.event_id as event_id
            FROM entity_history
            JOIN event_meta_data ON
                entity_history.event_id = event_meta_data.id
            WHERE event_meta_data.block_hash = block_row.block_hash AND
                entity_history.subgraph = subgraph_id
            GROUP BY
                entity_history.event_id
            ORDER BY entity_history.event_id DESC
        LOOP
            PERFORM revert_transaction(event_row.event_id::integer);
        END LOOP;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
