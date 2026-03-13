-- Read the latest stream entry height and id.
-- KEYS[1]: block stream key (e.g., poa:leader:lock:block:stream)
--
-- Returns: {height, stream_id} or {} when no entries exist.
local entries = redis.call("XREVRANGE", KEYS[1], "+", "-", "COUNT", 1)
if #entries == 0 then
    return {}
end

local entry = entries[1]
local entry_id = entry[1]
local fields = entry[2]
local entry_height = nil

for i = 1, #fields, 2 do
    if fields[i] == "height" then
        entry_height = tonumber(fields[i + 1])
        break
    end
end

if entry_height == nil then
    return {}
end

return {tostring(entry_height), entry_id}
