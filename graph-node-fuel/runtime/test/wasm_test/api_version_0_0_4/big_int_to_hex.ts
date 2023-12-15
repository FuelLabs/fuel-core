import "allocator/arena";

export { memory };

declare namespace typeConversion {
    function bigIntToHex(n: Uint8Array): String
}

export function big_int_to_hex(n: Uint8Array): String {
    return typeConversion.bigIntToHex(n)
}
