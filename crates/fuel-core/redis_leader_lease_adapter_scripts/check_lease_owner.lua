-- Check whether this node still sees us as lease owner.
-- KEYS[1]: lease key (e.g., poa:leader:lock)
-- ARGV[1]: lease owner token (UUID)
if redis.call("GET", KEYS[1]) == ARGV[1] then
    return 1
else
    return 0
end
