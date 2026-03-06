-- Release lease if we are still the owner (graceful shutdown / stepdown).
-- KEYS[1]: lease key
-- ARGV[1]: lease owner token (UUID)
if redis.call("GET", KEYS[1]) == ARGV[1] then
    return redis.call("DEL", KEYS[1])
else
    return 0
end
