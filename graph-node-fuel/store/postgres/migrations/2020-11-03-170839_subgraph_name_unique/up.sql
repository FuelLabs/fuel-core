-- Remove duplicate subgraphs; it should be impossible for
-- subgraphs that have multiple entries to have any versions.
-- The duplication happened because of a race condition in creating
-- subgraphs, but the code checked that there was only one subgraph
-- before deploying a version, and therefore failed
delete from subgraphs.subgraph
 where name in (select name from subgraphs.subgraph
                group by name having count(*) > 1);

alter table subgraphs.subgraph
  add constraint subgraph_name_uq unique(name);
