-- Read all stream entries and return compact tuples for reconciliation.
-- KEYS[1]: block stream key (e.g., poa:leader:lock:block:stream)
--
-- Returns: array of {height, epoch, data}
local entries = redis.call("XRANGE", KEYS[1], "-", "+")
local result = {}

for _, entry in ipairs(entries) do
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

    if entry_height ~= nil and entry_data ~= nil then
        table.insert(result, {entry_height, entry_epoch or 0, entry_data})
    end
end

return result
