drop function if exists
  build_attribute_index(text, text, text, text, boolean, text, text);

drop function if exists
  revert_block(varchar, varchar);

-- Functions we meant to get rid of earlier, but used the wrong syntax for
-- PostgreSQL 9.6
drop function if exists
  revert_block(varchar, int8, varchar, varchar);

drop function if exists
  revert_block(varchar[], varchar);

drop function if exists
  revert_entity_event(integer, integer);

drop function if exists
  revert_transaction(integer);

drop function if exists
  revert_transaction(integer[]);
