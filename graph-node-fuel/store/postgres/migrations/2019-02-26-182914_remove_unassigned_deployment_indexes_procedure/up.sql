-- Find all subgraphs without deployment assignments and remove their indexes
CREATE OR REPLACE FUNCTION remove_unassigned_deployment_indexes()
    RETURNS VOID AS
$$
DECLARE
    index_to_drop RECORD;
    counter INT := 0;
BEGIN
    -- Loop through each index
    FOR index_to_drop IN
        -- Get all indexes on subgraphs without deployment assignments
        SELECT
          indexname as name
        from pg_indexes
        where
          -- Extract the subgraph hash from the index name (subgraph hashes are 46 characters long)
          left(indexname,46) IN
          (
            -- Format the subgraph id for comparison with the index names
            select left(lower(deployments.id),46)
            from entities as deployments
            left join
              (
                select
                  id, entity
                from entities
                where
                  subgraph='subgraphs' and
                  entity='SubgraphDeploymentAssignment'
              ) assigments
            on deployments.id=assigments.id
            where
              deployments.subgraph='subgraphs' and
              deployments.entity='SubgraphDeployment' and
              assigments.id is null)
    -- Iterate over indexes and drop each
    LOOP
        EXECUTE 'DROP INDEX ' || index_to_drop.name;
        counter := counter + 1;
    END LOOP;
    RAISE NOTICE 'Successfully dropped % indexes', counter;
END;
$$ LANGUAGE plpgsql;
