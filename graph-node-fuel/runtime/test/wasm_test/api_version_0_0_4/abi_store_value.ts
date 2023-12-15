import "allocator/arena";

export { memory };

enum ValueKind {
    STRING = 0,
    INT = 1,
    BIG_DECIMAL = 2,
    BOOL = 3,
    ARRAY = 4,
    NULL = 5,
    BYTES = 6,
    BIG_INT = 7,
    INT8 = 8,
}

// Big enough to fit any pointer or native `this.data`.
type Payload = u64

type Bytes = Uint8Array;
type BigInt = Uint8Array;

export class BigDecimal {
    exp: BigInt
    digits: BigInt
}

export class Value {
    kind: ValueKind
    data: Payload
}

export function value_from_string(str: string): Value {
    let token = new Value();
    token.kind = ValueKind.STRING;
    token.data = str as u64;
    return token
}

export function value_from_int(int: i32): Value {
    let value = new Value();
    value.kind = ValueKind.INT;
    value.data = int as u64
    return value
}

export function value_from_int8(int: i64): Value {
    let value = new Value();
    value.kind = ValueKind.INT8;
    value.data = int as i64
    return value
}

export function value_from_big_decimal(float: BigInt): Value {
    let value = new Value();
    value.kind = ValueKind.BIG_DECIMAL;
    value.data = float as u64;
    return value
}

export function value_from_bool(bool: boolean): Value {
    let value = new Value();
    value.kind = ValueKind.BOOL;
    value.data = bool ? 1 : 0;
    return value
}

export function array_from_values(str: string, i: i32): Value {
    let array = new Array<Value>();
    array.push(value_from_string(str));
    array.push(value_from_int(i));

    let value = new Value();
    value.kind = ValueKind.ARRAY;
    value.data = array as u64;
    return value
}

export function value_null(): Value {
    let value = new Value();
    value.kind = ValueKind.NULL;
    return value
}

export function value_from_bytes(bytes: Bytes): Value {
    let value = new Value();
    value.kind = ValueKind.BYTES;
    value.data = bytes as u64;
    return value
}

export function value_from_bigint(bigint: BigInt): Value {
    let value = new Value();
    value.kind = ValueKind.BIG_INT;
    value.data = bigint as u64;
    return value
}

export function value_from_array(array: Array<string>): Value {
    let value = new Value()
    value.kind = ValueKind.ARRAY
    value.data = array as u64
    return value
}

// Test that this does not cause undefined behaviour in Rust.
export function invalid_discriminant(): Value {
    let token = new Value();
    token.kind = 70;
    token.data = "blebers" as u64;
    return token
}
