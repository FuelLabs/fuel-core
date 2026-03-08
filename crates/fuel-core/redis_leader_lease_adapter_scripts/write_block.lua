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

-- 4) Height-uniqueness check: prevent two blocks at the same height from
--    coexisting in the stream (which causes forks via quorum overlap).
--
--    If an entry at this height already exists:
--      - Same or higher epoch: reject (the existing block was written by an
--        equal or newer leader — it may be quorum-committed).
--      - Lower epoch: the existing entry is an orphaned sub-quorum write from
--        an older leader. Replace it (delete old, allow XADD below) so the
--        stream doesn't permanently deadlock with conflicting orphans.
local my_epoch = tonumber(ARGV[1])
local existing = redis.call("XREVRANGE", KEYS[1], "+", "-")
for _, entry in ipairs(existing) do
    local entry_id = entry[1]
    local fields = entry[2]
    local entry_height = nil
    local entry_epoch = 0
    for i = 1, #fields, 2 do
        if fields[i] == "height" then
            entry_height = fields[i + 1]
        elseif fields[i] == "epoch" then
            entry_epoch = tonumber(fields[i + 1]) or 0
        end
    end
    if entry_height == ARGV[3] then
        if entry_epoch >= my_epoch then
            return redis.error_reply(
                "HEIGHT_EXISTS: Block at height " .. ARGV[3] ..
                " already in stream at epoch " .. entry_epoch
            )
        else
            -- Orphaned entry from older epoch — remove it to make room
            redis.call("XDEL", KEYS[1], entry_id)
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

redis.call("XTRIM", KEYS[1], "MAXLEN", "~", ARGV[6])
redis.call("PEXPIRE", KEYS[3], ARGV[5])

return stream_id
