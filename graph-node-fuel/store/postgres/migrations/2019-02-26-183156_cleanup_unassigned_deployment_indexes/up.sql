-- Remove subgraph attribute indexes for subgraphs without a deployment assignment
SELECT remove_unassigned_deployment_indexes();
