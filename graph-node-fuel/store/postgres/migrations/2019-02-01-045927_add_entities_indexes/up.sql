-- Change the order of columns in the pk index for entities
ALTER TABLE entities DROP CONSTRAINT entities_pkey;
ALTER TABLE entities ADD CONSTRAINT
  entities_pkey primary key (subgraph, entity, id);
