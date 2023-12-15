TRUNCATE TABLE subgraphs.subgraph_features;
ALTER TABLE subgraphs.subgraph_features
ADD COLUMN network text not null;