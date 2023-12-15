-- It would seem logical to add a foreign key constraint on graft_base to
-- make sure the base subgraph sticks around. But once we have copied the base
-- subgraph, the graft does not need the base subgraph anymore, and it is
-- therefore perfectly fine if the base gets deleted; the graft_base is
-- purely informational
alter table subgraphs.subgraph_deployment
  add column graft_base text,
  add column graft_block_hash bytea,
  add column graft_block_number numeric;
