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

-- 4) Persist block entry.
local stream_id = redis.call("XADD", KEYS[1], "*",
    "height", ARGV[3],
    "data", ARGV[4],
    "epoch", ARGV[1],
    "timestamp", redis.call("TIME")[1]
)

redis.call("XTRIM", KEYS[1], "MAXLEN", "~", ARGV[6])
redis.call("PEXPIRE", KEYS[3], ARGV[5])

return stream_id
