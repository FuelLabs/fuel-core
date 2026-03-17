-- Read stream entries at/after a minimum block height and return compact tuples for reconciliation.
-- KEYS[1]: block stream key (e.g., poa:leader:lock:block:stream)
-- ARGV[1]: minimum block height (inclusive)
-- ARGV[2]: max entries to read
--
-- Returns: array of {height, epoch, data, stream_id}
local min_height = tonumber(ARGV[1])
if min_height == nil then
    return {}
end

local count = tonumber(ARGV[2])
if count == nil or count <= 0 then
    return {}
end

local entries = redis.call("XRANGE", KEYS[1], "-", "+")
local result = {}

for _, entry in ipairs(entries) do
    local entry_id = entry[1]
    local fields = entry[2]
    local entry_height = nil
    local entry_data = nil
    local entry_epoch = nil

    for i = 1, #fields, 2 do
        if fields[i] == "height" then
            entry_height = tonumber(fields[i + 1])
        elseif fields[i] == "data" then
            entry_data = fields[i + 1]
        elseif fields[i] == "epoch" then
            entry_epoch = tonumber(fields[i + 1])
        end
    end

    if entry_height ~= nil and entry_data ~= nil and entry_height >= min_height then
        table.insert(result, {entry_height, entry_epoch or 0, entry_data, entry_id})
        if #result >= count then
            break
        end
    end
end

return result
