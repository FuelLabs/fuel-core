ALTER TABLE subgraph_deployments ADD COLUMN cost DECIMAL(12, 4) NOT NULL CHECK (cost >= 0) DEFAULT 1;
