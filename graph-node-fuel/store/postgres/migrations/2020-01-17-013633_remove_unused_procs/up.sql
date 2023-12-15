-- This was used to get rid of public.entities
drop function if exists remove_public_deployments();
-- Helper for maintenance of public.entities
drop function if exists remove_unassigned_deployment_indexes();
-- Was used to create entity_history entries for public.entities
drop function if exists log_entity_event();

-- We tried to drop these before, but did not include arguments which made
-- PG 9.6 not do anything; was used a long time ago in block reversion, but
-- not any more. That is all done by subgraph_log_entity_event()
drop function if exists revert_block_group(varchar[], varchar);
drop function if exists revert_transaction_group(integer[]);
drop function if exists rerun_entity(integer, varchar, varchar, varchar);
drop function if exists rerun_entity_history_event(integer, integer);

-- These were triggers on the old subgraph_deployments table; since that
-- table does not exist anymore, these procs are not needed
drop function if exists deployment_delete();
drop function if exists deployment_insert();
drop function if exists deployment_update();

-- Used by old migrations, but not needed anymore, and will not work once
-- we store metadata in relational storage
drop function if exists list_deployment_entities(varchar[]);
drop function if exists remove_deployment(varchar);
drop function if exists remove_deployment_metadata(varchar[]);
