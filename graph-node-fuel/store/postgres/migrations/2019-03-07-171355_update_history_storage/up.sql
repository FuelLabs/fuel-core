-- Change log_entity_event such that we do not record history entries for
-- the 'subgraphs' subgraph and for operations that don't actually change
-- the data of an entity
drop trigger if exists after_entity_change_trigger on entities;

create trigger entity_change_insert_trigger
  after insert on entities
  for each row
  when (new.subgraph != 'subgraphs')
  execute procedure log_entity_event();

create trigger entity_change_update_trigger
  after update on entities
  for each row
  when (old.subgraph != 'subgraphs' and old.data != new.data)
  execute procedure log_entity_event();

create trigger entity_change_delete_trigger
  after delete on entities
  for each row
  when (old.subgraph != 'subgraphs')
  execute procedure log_entity_event();

-- Create a view to get entity_history by source ordered by event_id desc
-- We use this both in revert_block and in the Rust code in
-- store_events.get_revert_event
create or replace view entity_history_with_source as
select
  h.subgraph,
  h.entity,
  h.entity_id,
  h.data_before,
  h.op_id,
  m.source
from entity_history h, event_meta_data m
where h.event_id = m.id
order by h.event_id desc;

-- Rewrite revert_block to be selfcontained and to get all the information
-- needed for reversion in one query
create or replace function
  revert_block(block_to_revert_hash VARCHAR,
               subgraph_id VARCHAR)
    returns void as
$$
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
$$ language plpgsql;

-- remove unused stored procs
drop function if exists revert_transaction();
drop function if exists revert_entity_event();

-- remove some old, never used procs while we are at it
drop function if exists revert_block_group();
drop function if exists revert_transaction_group();
drop function if exists rerun_entity();
drop function if exists rerun_entity_history_event();
