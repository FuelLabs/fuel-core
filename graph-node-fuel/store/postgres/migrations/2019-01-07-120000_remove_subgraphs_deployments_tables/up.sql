-- Before proceeding and throwing away data, check that database version is
-- high enough to guarantee that these tables were not in use.
--
-- Copy this code to a new migration and increment the db_version in the
-- 2018-07-10-061642 migration if you need to make a non-backwards compatible
-- change to the database schema.
CREATE FUNCTION check_db_version() RETURNS VOID AS $$
    DECLARE
        version BIGINT;
    BEGIN
        SELECT db_version INTO STRICT version FROM db_version;
        IF version < 2 THEN
            RAISE EXCEPTION 'Database schemas are out of date and incompatible';
        END IF;
    END
$$ LANGUAGE plpgsql;
SELECT check_db_version();
DROP FUNCTION check_db_version();

-- Remove unused tables (this data is now stored in entities)
DROP TABLE subgraphs;
DROP TABLE subgraph_deployments;
