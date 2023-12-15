import "allocator/arena";

export { memory };

declare namespace crypto {
    function keccak256(input: Uint8Array): Uint8Array
}

export function hash(input: Uint8Array): Uint8Array {
    return crypto.keccak256(input)
}
