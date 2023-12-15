-- Remove indexes on entities for data->'id'->>'data'
create or replace function remove_entities_id_indexes()
    returns void as
$$
declare
    index_to_drop record;
    counter int := 0;
begin
    for index_to_drop in
        select
          indexname as name
        from pg_indexes
       where tablename = 'entities'
         and indexname like 'qm%'
         and indexdef like '%ON public.entities USING btree ((((data -> ''id''::text) ->> ''data''::text))) WHERE%'
    loop
        execute 'drop index ' || index_to_drop.name;
        counter := counter + 1;
    end loop;
    raise notice 'Successfully dropped % indexes', counter;
end;
$$ language plpgsql;

select remove_entities_id_indexes();

drop function remove_entities_id_indexes();
