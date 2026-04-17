-- Atomically fence stale leaders, persist a block, trim stream, and renew lease.
-- KEYS[1]: block stream key (e.g., poa:leader:lock:block:stream)
-- KEYS[2]: epoch token key (e.g., poa:leader:lock:epoch:token)
-- KEYS[3]: leader lock key (e.g., poa:leader:lock)
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

-- 4) Strict height-uniqueness check: reject if ANY entry at this height
--    already exists in the stream, regardless of epoch.
--
--    This prevents forks via the pigeonhole principle: any two quorums of
--    size ceil(N/2)+1 overlap on at least one node. If Leader A published
--    a block at height H to quorum, the overlapping node already has an
--    entry at height H. Leader B's write is rejected on that node,
--    preventing it from reaching quorum with a different block.
--
--    Sub-quorum orphans (partial writes from failed leaders) may cause
--    the new leader to fail on the orphan's node, but it can still reach
--    quorum on the remaining nodes. If the orphan blocks quorum entirely,
--    the leader must reconcile via the read path instead.
local posted_height = tonumber(ARGV[3])
local existing = redis.call("XREVRANGE", KEYS[1], "+", "-")
local stop_scan = false
for _, entry in ipairs(existing) do
    local fields = entry[2]
    for i = 1, #fields, 2 do
        if fields[i] == "height" then
            local entry_height = tonumber(fields[i + 1])
            if entry_height == posted_height then
                return redis.error_reply(
                    "HEIGHT_EXISTS: Block at height " .. ARGV[3] .. " already in stream"
                )
            end
            if entry_height ~= nil and entry_height < posted_height then
                stop_scan = true
                break
            end
        end
    end
    if stop_scan then
        break
    end
end

-- 5) Persist block entry.
local stream_id = redis.call("XADD", KEYS[1], "*",
    "height", ARGV[3],
    "data", ARGV[4],
    "epoch", ARGV[1],
    "timestamp", redis.call("TIME")[1]
)

redis.call("XTRIM", KEYS[1], "MAXLEN", "~", ARGV[6])
redis.call("PEXPIRE", KEYS[3], ARGV[5])

return stream_id
