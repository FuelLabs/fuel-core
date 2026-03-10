-- Atomically fence stale leaders, persist a block, trim stream, and renew lease.
-- KEYS[1]: block stream key (e.g., poa:leader:lock:block:stream)
-- KEYS[2]: epoch token key (e.g., poa:leader:lock:epoch:token)
-- KEYS[3]: leader lock key (e.g., poa:leader:lock)
-- KEYS[4]: height index hash key (e.g., poa:leader:lock:block:heights)
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

-- 4) Height-uniqueness check with block identity verification.
--
--    Uses an O(1) hash index (KEYS[4]) mapping height -> sha1(data).
--    When the height exists, compares fingerprints to distinguish
--    "same block already stored" from "different block at same height".
--    Falls back to a stream scan for entries not yet in the index
--    (e.g., written before the index was introduced).
--
--    HEIGHT_EXISTS_MATCH means the exact same block is already stored.
--    HEIGHT_EXISTS means a different block occupies this height.
local data_fp = redis.sha1hex(ARGV[4])
local existing_fp = redis.call("HGET", KEYS[4], ARGV[3])
if existing_fp ~= false then
    if existing_fp == data_fp then
        return redis.error_reply(
            "HEIGHT_EXISTS_MATCH: Block at height " .. ARGV[3] .. " already in stream (same data)"
        )
    else
        return redis.error_reply(
            "HEIGHT_EXISTS: Block at height " .. ARGV[3] .. " already in stream"
        )
    end
end

-- Fallback: scan stream for entries not yet in the hash index.
local existing = redis.call("XREVRANGE", KEYS[1], "+", "-")
for _, entry in ipairs(existing) do
    local fields = entry[2]
    for i = 1, #fields, 2 do
        if fields[i] == "height" and fields[i + 1] == ARGV[3] then
            local entry_data = nil
            for j = 1, #fields, 2 do
                if fields[j] == "data" then
                    entry_data = fields[j + 1]
                    break
                end
            end
            local entry_fp = entry_data and redis.sha1hex(entry_data) or ""
            redis.call("HSET", KEYS[4], ARGV[3], entry_fp)
            if entry_fp == data_fp then
                return redis.error_reply(
                    "HEIGHT_EXISTS_MATCH: Block at height " .. ARGV[3] .. " already in stream (same data)"
                )
            else
                return redis.error_reply(
                    "HEIGHT_EXISTS: Block at height " .. ARGV[3] .. " already in stream"
                )
            end
        end
    end
end

-- 5) Persist block entry.
local stream_id = redis.call("XADD", KEYS[1], "*",
    "height", ARGV[3],
    "data", ARGV[4],
    "epoch", ARGV[1],
    "timestamp", redis.call("TIME")[1]
)

-- Update height index and trim stale entries.
redis.call("HSET", KEYS[4], ARGV[3], data_fp)
local old_height = tonumber(ARGV[3]) - tonumber(ARGV[6]) * 2
if old_height >= 0 then
    redis.call("HDEL", KEYS[4], tostring(old_height))
end

redis.call("XTRIM", KEYS[1], "MAXLEN", "~", ARGV[6])
redis.call("PEXPIRE", KEYS[3], ARGV[5])

return stream_id
