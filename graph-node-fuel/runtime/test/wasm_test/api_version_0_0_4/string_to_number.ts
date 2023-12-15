import "allocator/arena";

export { memory };

type BigInt = Uint8Array;

/** Host JSON interface */
declare namespace json {
    function toI64(decimal: string): i64
    function toU64(decimal: string): u64
    function toF64(decimal: string): f64
    function toBigInt(decimal: string): BigInt
}

export function testToI64(decimal: string): i64 {
    return json.toI64(decimal);
}

export function testToU64(decimal: string): u64 {
    return json.toU64(decimal);
}

export function testToF64(decimal: string): f64 {
    return json.toF64(decimal)
}

export function testToBigInt(decimal: string): BigInt {
    return json.toBigInt(decimal)
}
