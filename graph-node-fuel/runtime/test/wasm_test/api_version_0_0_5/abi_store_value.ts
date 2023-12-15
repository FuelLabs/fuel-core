export * from './common/global'
import { BigInt, BigDecimal, Bytes, Value, ValueKind } from './common/types'

export function value_from_string(str: string): Value {
    let token = new Value();
    token.kind = ValueKind.STRING;
    token.data = changetype<u32>(str);
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
    value.data = changetype<u32>(float);
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
    value.data = changetype<u32>(array);
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
    value.data = changetype<u32>(bytes);
    return value
}

export function value_from_bigint(bigint: BigInt): Value {
    let value = new Value();
    value.kind = ValueKind.BIG_INT;
    value.data = changetype<u32>(bigint);
    return value
}

export function value_from_array(array: Array<string>): Value {
    let value = new Value()
    value.kind = ValueKind.ARRAY
    value.data = changetype<u32>(array)
    return value
}

// Test that this does not cause undefined behaviour in Rust.
export function invalid_discriminant(): Value {
    let token = new Value();
    token.kind = 70;
    token.data = changetype<u32>("blebers");
    return token
}
