-- Version from 2019-06-05-214320_uncled_chain_head_update
CREATE OR REPLACE FUNCTION attempt_chain_head_update(net_name VARCHAR, ancestor_count BIGINT)
    RETURNS VARCHAR[] AS
$$
DECLARE
    current_head_number BIGINT;
    new_head_hash VARCHAR;
    new_head_number BIGINT;
    genesis_hash VARCHAR;
    missing_parents VARCHAR[];
    first_block_number BIGINT;
BEGIN
    --
    -- Find candidate new chain head block
    --
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

    --
    -- Verify that we have a complete (hash, parent_hash) chain at least
    -- ancestor_count blocks long in the database, starting with the
    -- candidate head we just found
    --

    -- The first (lowest) number of a block we have to check for a
    -- continuous chain. If new_head_number is less than ancestor_count,
    -- check all blocks
    first_block_number = greatest(new_head_number - ancestor_count, 0);

    -- Find the first block that is missing from the database needed to
    -- complete the chain of ancestor_count blocks. We return it as an
    -- array because the Rust code expects that, but the array will only
    -- ever have one element.
    -- We recursively build a temp table 'chain' containing the hash and
    -- parent_hash of blocks to check. The 'last' value is used to stop
    -- the recursion and is true if one of these conditions is true:
    --   * we are missing a parent block
    --   * we checked the required number of blocks
    --   * we checked the genesis block
    with recursive chain(hash, parent_hash, last) as (
       -- base case: look at the head candidate block
       select b.hash, b.parent_hash, false
         from ethereum_blocks b
        where b.network_name = net_name
          and b.hash = new_head_hash
       union all
       -- recursion step: add a block whose hash is the latest parent_hash
       -- on chain
       select chain.parent_hash,
              b.parent_hash,
              coalesce(b.parent_hash is null
                    or b.number <= first_block_number
                    or b.hash = genesis_hash, true)
         from chain left outer join ethereum_blocks b
                     on chain.parent_hash = b.hash
                    and b.network_name = net_name
        where not chain.last)
    select array_agg(hash)
      into missing_parents
      from chain
     where chain.parent_hash is null;

    -- Stop now if we found a missing parent
    IF missing_parents is not null THEN
        RETURN missing_parents;
    END IF;

    -- No missing parent blocks, therefore candidate new chain head block
    -- has the necessary minimum number of ancestors present in DB.

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
