-- add done_at column to data_sources$ table for each subgraph deployment
do $$
declare
  deployments cursor for
     select t.table_schema as sgd
       from information_schema.tables t
      where t.table_schema like 'sgd%'
        and t.table_name = 'data_sources$'
        and not exists (select 1 from information_schema.columns c
                         where c.table_name = t.table_name
                           and c.table_schema = t.table_schema
                           and c.column_name = 'done_at');
begin
  for d in deployments loop
    execute 'alter table ' || d.sgd || '.data_sources$ add done_at int';
  end loop;
end;
$$;