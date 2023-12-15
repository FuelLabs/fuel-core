-- Same as ../2018-09-07-220000_create_ancestor_lookup_procedure/up.sql

CREATE OR REPLACE FUNCTION lookup_ancestor_block(start_block_hash VARCHAR, ancestor_count BIGINT)
    RETURNS JSONB AS
$$
DECLARE
    target_block_hash VARCHAR;
    json_blob JSONB;
BEGIN
    -- Follow parent hashes back the necessary number of steps to find the
    -- target block's hash.
    WITH RECURSIVE
        ancestors(block_hash, block_offset)
        AS (
            VALUES
                (start_block_hash, 0)
            UNION ALL
                SELECT ethereum_blocks.parent_hash, ancestors.block_offset+1
                FROM ancestors, ethereum_blocks
                WHERE
                    ancestors.block_hash = ethereum_blocks.hash
                    AND block_offset < ancestor_count
        )
        SELECT block_hash
        INTO target_block_hash
        FROM ancestors
        WHERE block_offset = ancestor_count;

    -- Load the JSON blob from the target block
    SELECT data INTO json_blob
    FROM ethereum_blocks
    WHERE hash = target_block_hash;

    -- Return
    RETURN json_blob;
END;
$$ LANGUAGE plpgsql;
