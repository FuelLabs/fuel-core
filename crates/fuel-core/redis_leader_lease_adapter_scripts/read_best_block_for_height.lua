-- Read stream entries and return the highest-epoch block for target height.
-- KEYS[1]: block stream key (e.g., poa:leader:lock:block:stream)
-- ARGV[1]: target height
--
-- Returns: {best_epoch, best_data} when found, false when no match exists.
local entries = redis.call("XRANGE", KEYS[1], "-", "+")
local target_height = tonumber(ARGV[1])
local best_epoch = nil
local best_data = nil

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

    if entry_height == target_height then
        local epoch = entry_epoch or 0
        if (best_epoch == nil) or (epoch > best_epoch) then
            best_epoch = epoch
            best_data = entry_data
        end
    end
end

if best_data ~= nil then
    return {tostring(best_epoch), best_data}
end

return false
