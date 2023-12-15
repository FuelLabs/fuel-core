--  DROP ATTRIBUTE INDEXING FUNCTION
DROP FUNCTION build_attribute_index(
  subgraph_id Text,
  index_name Text,
  index_type Text,
  index_operator Text,
  attribute_name Text,
  entity_name Text
);