-- Atomically promote follower to leader and return a new fencing token.
-- KEYS[1]: leader lock key (e.g., poa:leader:lock)
-- KEYS[2]: epoch token key (e.g., poa:leader:lock:epoch:token)
-- ARGV[1]: lease owner token (UUID)
-- ARGV[2]: lease TTL in milliseconds
--
-- Returns: new epoch token on success, error if lock is held.
local acquired = redis.call("SET", KEYS[1], ARGV[1], "PX", ARGV[2], "NX")
if not acquired then
    return redis.error_reply("LOCK_HELD: Another leader holds the lock")
end

local new_token = redis.call("INCR", KEYS[2])
return new_token
