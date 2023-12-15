-- This file should undo anything in `up.sql`

drop view if exists info.all_sizes;

create view info.all_sizes as
select * from info.subgraph_sizes
union all
select * from info.table_sizes;

drop materialized view if exists info.chain_sizes;
