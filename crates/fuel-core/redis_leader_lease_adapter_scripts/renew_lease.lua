-- Renew lease TTL if we are still the owner.
-- KEYS[1]: lease key
-- ARGV[1]: lease owner token (UUID)
-- ARGV[2]: lease TTL in milliseconds
if redis.call("GET", KEYS[1]) == ARGV[1] then
    return redis.call("PEXPIRE", KEYS[1], ARGV[2])
else
    return 0
end
