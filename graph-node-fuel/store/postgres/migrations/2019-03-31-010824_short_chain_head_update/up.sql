-- Fix attempt_chain_head_update for short chains (chains shorter than
-- ancestor_count many blocks)
CREATE OR REPLACE FUNCTION attempt_chain_head_update(net_name VARCHAR, ancestor_count BIGINT)
    RETURNS VARCHAR[] AS
$$
DECLARE
    current_head_number BIGINT;
    new_head_hash VARCHAR;
    new_head_number BIGINT;
    genesis_hash VARCHAR;
    missing_parents VARCHAR[];
BEGIN
    -- Find candidate new chain head block
    SELECT
       hash,
       number,
       genesis_block_hash
    INTO
       new_head_hash,
       new_head_number,
       genesis_hash
    FROM ethereum_blocks b, ethereum_networks n
    WHERE b.network_name = net_name
      and n.name = net_name
      --- Handle the case where ethereum_networks has a NULL head_block_number
      and b.number > coalesce(n.head_block_number, -1)
    ORDER BY
       number DESC,
       hash ASC
    LIMIT 1;

    -- Stop now if it's no better than the current chain head block
    IF new_head_hash IS NULL THEN
        RETURN ARRAY[]::VARCHAR[];
    END IF;

    -- Aggregate a list of missing parent hashes into missing_parents,
    -- selecting only parents of blocks within ancestor_count of new head.
    --
    -- In the common case during block ingestion, this will find only one or
    -- zero missing parents, which causes the block ingestor to walk backwards
    -- from the latest block, loading blocks one at a time.
    -- A possible performance improvement in the block ingestor would be to
    -- load blocks speculatively by number instead of by hash.
    --
    -- We build a list of ancestor_count (hash, parent_hash) tuples and
    -- check that for each mention of a parent_hash in a tuple (_, h1)
    -- there is also a tuple that has that in the hash position, i.e. that
    -- we have a tuple (h1, _) That can of course not work for the last
    -- tuple in the chain. We therefore mark it with 'last = t' (all the
    -- others will have 'last = f') and do not try to find its parent, or
    -- report its parent as missing. We can tell this last tuple because it
    -- is either the one with number 'new_head_number - ancestor_count',
    -- or, if the chain has fewer than ancestor_count blocks, the genesis
    -- block. The latter can happen for chains from ganache which are
    -- generally very short (say 8 blocks or so)
    WITH head AS
        (SELECT hash,
                parent_hash,
                (number = (new_head_number - ancestor_count)
                 or hash = genesis_hash) as last
           FROM ethereum_blocks
          WHERE network_name = net_name
            AND number >= GREATEST(new_head_number - ancestor_count, 0))
    SELECT array_agg(h1.parent_hash)
      INTO STRICT missing_parents
      FROM head h1
     WHERE not h1.last
       AND NOT EXISTS (SELECT 1 FROM head h2 WHERE h1.parent_hash = h2.hash);

    -- Stop now if there are any recent blocks with missing parents
    IF array_length(missing_parents, 1) > 0 THEN
        RETURN missing_parents;
    END IF;

    -- No recent missing parent blocks, therefore candidate new chain head block has
    -- the necessary minimum number of ancestors present in DB.

    -- Set chain head block pointer to candidate chain head block
    UPDATE ethereum_networks
    SET
        head_block_hash = new_head_hash,
        head_block_number = new_head_number
    WHERE name = net_name;

    -- Fire chain head block update event
    PERFORM pg_notify('chain_head_updates', json_build_object(
        'network_name', net_name,
        'head_block_hash', new_head_hash,
        'head_block_number', new_head_number
    )::text);

    -- Done
    RETURN ARRAY[]::VARCHAR[];
END;
$$ LANGUAGE plpgsql;
