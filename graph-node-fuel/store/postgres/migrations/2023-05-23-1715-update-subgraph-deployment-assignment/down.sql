-- Define the 'down' migration to remove the 'paused_at' and 'assigned_at' fields from 'subgraph_deployment_assignment' table
ALTER TABLE subgraphs.subgraph_deployment_assignment
DROP COLUMN paused_at,
DROP COLUMN assigned_at;