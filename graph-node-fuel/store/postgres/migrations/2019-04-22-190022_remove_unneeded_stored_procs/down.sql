CREATE OR REPLACE FUNCTION build_attribute_index(subgraph_id text, index_name text, index_type text, index_operator text, jsonb_index boolean, attribute_name text, entity_name text)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
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
$function$
;

CREATE OR REPLACE FUNCTION public.revert_block(block_to_revert_hash character varying, subgraph_id character varying)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
declare
    history record;
begin
    -- Revert all relevant events
    for history in
        -- Get all history entries associated with the given block. Note
        -- that the view imposes the correct order
    select
        h.entity,
        h.entity_id,
        h.data_before,
        h.op_id
    from entity_history_with_source h
    where h.source = block_to_revert_hash
      and h.subgraph = subgraph_id
    loop
        case
            -- insert case
            when history.op_id = 0 then
                -- Delete inserted row
            begin
                perform set_config('vars.current_event_source',
                                   'REVERSION', false);
                delete from entities
                 where subgraph = subgraph_id
                   and entity = history.entity
                   and id = history.entity_id;
                -- Row was already updated
            exception when no_data_found then
              -- do nothing, the entity was gone already
            end;
            -- update or delete case
            when history.op_id IN (1,2) then
                -- Insert deleted row if not exists
                -- If row exists perform update
                begin
                     insert into entities
                                 (id, subgraph, entity, data, event_source)
                     values (history.entity_id,
                             subgraph_id,
                             history.entity,
                             history.data_before,
                             'REVERSION')
                     on conflict (id, subgraph, entity)
                     do update
                           set data = history.data_before,
                               event_source = 'REVERSION';
            end;
        end case;
    end loop;
end;
$function$
;
