-- Define the 'up' migration to add the 'paused_at' and 'assigned_at' fields to 'subgraph_deployment_assignment' table
ALTER TABLE subgraphs.subgraph_deployment_assignment
ADD COLUMN paused_at TIMESTAMPTZ NULL,
ADD COLUMN assigned_at TIMESTAMPTZ NULL;