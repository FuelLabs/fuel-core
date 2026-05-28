-- Atomically fence stale leaders, persist a block, trim stream, and renew lease.
-- KEYS[1]: block stream key (e.g., poa:leader:lock:block:stream)
-- KEYS[2]: epoch token key (e.g., poa:leader:lock:epoch:token)
-- KEYS[3]: leader lock key (e.g., poa:leader:lock)
-- KEYS[4]: heights index key (e.g., poa:leader:lock:heights:index)
-- ARGV[1]: my_epoch (max token observed during promotion quorum)
-- ARGV[2]: lease owner token (UUID)
-- ARGV[3]: block_height
-- ARGV[4]: block_data
-- ARGV[5]: lease_ttl_ms
-- ARGV[6]: stream_max_len
local current_token = tonumber(redis.call("GET", KEYS[2]) or "0")
local current_leader = redis.call("GET", KEYS[3])

-- 1) Identity check: only current owner may write.
if current_leader ~= ARGV[2] then
    return redis.error_reply("FENCING_ERROR: Lock lost or held by another node")
end

-- 2) Fencing check: reject stale epoch token.
if tonumber(ARGV[1]) < current_token then
    return redis.error_reply("FENCING_ERROR: Token is stale")
end

-- 3) Self-heal node epoch if our token is newer.
if tonumber(ARGV[1]) > current_token then
    redis.call("SET", KEYS[2], ARGV[1])
end

-- 4) Strict height-uniqueness check via the heights-index ZSET:
--    reject if a block at this height was ever written through this
--    script. ZSCORE is O(log N) and does not materialize stream
--    payloads into Lua memory, unlike the prior XREVRANGE scan.
--
--    This prevents forks via the pigeonhole principle: any two quorums
--    of size ceil(N/2)+1 overlap on at least one node. If Leader A
--    published a block at height H to quorum, the overlapping node has
--    height H indexed. Leader B's ZSCORE on that node returns non-nil,
--    so its write is rejected there, preventing it from reaching
--    quorum with a different block.
--
--    The index is trimmed below to retain strictly more heights than
--    the stream's `XTRIM ~` keeps, so a height that is still in the
--    stream is always still in the index — no false negatives.
--
--    Sub-quorum orphans (partial writes from failed leaders) may cause
--    the new leader to fail on the orphan's node, but it can still
--    reach quorum on the remaining nodes. If the orphan blocks quorum
--    entirely, the leader must reconcile via the read path instead.
local posted_height = tonumber(ARGV[3])
if redis.call("ZSCORE", KEYS[4], posted_height) then
    return redis.error_reply(
        "HEIGHT_EXISTS: Block at height " .. ARGV[3] .. " already in stream"
    )
end

-- 5) Persist block entry.
local stream_id = redis.call("XADD", KEYS[1], "*",
    "height", ARGV[3],
    "data", ARGV[4],
    "epoch", ARGV[1],
    "timestamp", redis.call("TIME")[1]
)

-- 6) Record the height in the dedup index (member == score == height).
redis.call("ZADD", KEYS[4], posted_height, posted_height)

redis.call("XTRIM", KEYS[1], "MAXLEN", "~", ARGV[6])

-- 7) Trim the index to a window strictly larger than the stream's
--    `XTRIM ~` retention so the index always covers what the stream
--    still holds. `XTRIM MAXLEN ~ N` keeps up to N + macronode_size
--    (~64 in current Redis); 2N + 256 is comfortably above that for
--    both production and small-N test paths.
local stream_max_len = tonumber(ARGV[6])
redis.call("ZREMRANGEBYRANK", KEYS[4], 0, -(2 * stream_max_len + 256) - 1)

redis.call("PEXPIRE", KEYS[3], ARGV[5])

return stream_id
