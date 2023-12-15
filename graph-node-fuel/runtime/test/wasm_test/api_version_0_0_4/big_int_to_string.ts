import "allocator/arena";

export { memory };

declare namespace typeConversion {
    function bigIntToString(n: Uint8Array): String
}

export function big_int_to_string(n: Uint8Array): String {
    return typeConversion.bigIntToString(n)
}
