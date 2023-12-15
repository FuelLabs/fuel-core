/** A dynamically-sized byte array. */
export class Bytes extends ByteArray { }

/** An Ethereum address (20 bytes). */
export class Address extends Bytes {
  static fromString(s: string): Address {
    return typeConversion.stringToH160(s) as Address
  }
}

// Sequence of 20 `u8`s.
// export type Address = Uint8Array;

// Sequence of 32 `u8`s.
export type Uint8 = Uint8Array;

// Sequences of `u8`s.
export type FixedBytes = Uint8Array;
// export type Bytes = Uint8Array;

/**
 * Enum for supported value types.
 */
export enum ValueKind {
  STRING = 0,
  INT = 1,
  BIG_DECIMAL = 2,
  BOOL = 3,
  ARRAY = 4,
  NULL = 5,
  BYTES = 6,
  BIG_INT = 7,
  INT8 = 8
}
// Big enough to fit any pointer or native `this.data`.
export type Payload = u64
/**
 * A dynamically typed value.
 */
export class Value {
  kind: ValueKind
  data: Payload

  toAddress(): Address {
    assert(this.kind == ValueKind.BYTES, 'Value is not an address.')
    return changetype<Address>(this.data as u32)
  }

  toBoolean(): boolean {
    if (this.kind == ValueKind.NULL) {
      return false;
    }
    assert(this.kind == ValueKind.BOOL, 'Value is not a boolean.')
    return this.data != 0
  }

  toBytes(): Bytes {
    assert(this.kind == ValueKind.BYTES, 'Value is not a byte array.')
    return changetype<Bytes>(this.data as u32)
  }

  toI32(): i32 {
    if (this.kind == ValueKind.NULL) {
      return 0;
    }
    assert(this.kind == ValueKind.INT, 'Value is not an i32.')
    return this.data as i32
  }

  toString(): string {
    assert(this.kind == ValueKind.STRING, 'Value is not a string.')
    return changetype<string>(this.data as u32)
  }

  toBigInt(): BigInt {
    assert(this.kind == ValueKind.BIG_INT, 'Value is not a BigInt.')
    return changetype<BigInt>(this.data as u32)
  }

  toBigDecimal(): BigDecimal {
    assert(this.kind == ValueKind.BIG_DECIMAL, 'Value is not a BigDecimal.')
    return changetype<BigDecimal>(this.data as u32)
  }

  toArray(): Array<Value> {
    assert(this.kind == ValueKind.ARRAY, 'Value is not an array.')
    return changetype<Array<Value>>(this.data as u32)
  }

  toBooleanArray(): Array<boolean> {
    let values = this.toArray()
    let output = new Array<boolean>(values.length)
    for (let i: i32; i < values.length; i++) {
      output[i] = values[i].toBoolean()
    }
    return output
  }

  toBytesArray(): Array<Bytes> {
    let values = this.toArray()
    let output = new Array<Bytes>(values.length)
    for (let i: i32 = 0; i < values.length; i++) {
      output[i] = values[i].toBytes()
    }
    return output
  }

  toStringArray(): Array<string> {
    let values = this.toArray()
    let output = new Array<string>(values.length)
    for (let i: i32 = 0; i < values.length; i++) {
      output[i] = values[i].toString()
    }
    return output
  }

  toI32Array(): Array<i32> {
    let values = this.toArray()
    let output = new Array<i32>(values.length)
    for (let i: i32 = 0; i < values.length; i++) {
      output[i] = values[i].toI32()
    }
    return output
  }

  toBigIntArray(): Array<BigInt> {
    let values = this.toArray()
    let output = new Array<BigInt>(values.length)
    for (let i: i32 = 0; i < values.length; i++) {
      output[i] = values[i].toBigInt()
    }
    return output
  }

  toBigDecimalArray(): Array<BigDecimal> {
    let values = this.toArray()
    let output = new Array<BigDecimal>(values.length)
    for (let i: i32 = 0; i < values.length; i++) {
      output[i] = values[i].toBigDecimal()
    }
    return output
  }

  static fromBooleanArray(input: Array<boolean>): Value {
    let output = new Array<Value>(input.length)
    for (let i: i32 = 0; i < input.length; i++) {
      output[i] = Value.fromBoolean(input[i])
    }
    return Value.fromArray(output)
  }

  static fromBytesArray(input: Array<Bytes>): Value {
    let output = new Array<Value>(input.length)
    for (let i: i32 = 0; i < input.length; i++) {
      output[i] = Value.fromBytes(input[i])
    }
    return Value.fromArray(output)
  }

  static fromI32Array(input: Array<i32>): Value {
    let output = new Array<Value>(input.length)
    for (let i: i32 = 0; i < input.length; i++) {
      output[i] = Value.fromI32(input[i])
    }
    return Value.fromArray(output)
  }

  static fromBigIntArray(input: Array<BigInt>): Value {
    let output = new Array<Value>(input.length)
    for (let i: i32 = 0; i < input.length; i++) {
      output[i] = Value.fromBigInt(input[i])
    }
    return Value.fromArray(output)
  }

  static fromBigDecimalArray(input: Array<BigDecimal>): Value {
    let output = new Array<Value>(input.length)
    for (let i: i32 = 0; i < input.length; i++) {
      output[i] = Value.fromBigDecimal(input[i])
    }
    return Value.fromArray(output)
  }

  static fromStringArray(input: Array<string>): Value {
    let output = new Array<Value>(input.length)
    for (let i: i32 = 0; i < input.length; i++) {
      output[i] = Value.fromString(input[i])
    }
    return Value.fromArray(output)
  }

  static fromArray(input: Array<Value>): Value {
    let value = new Value()
    value.kind = ValueKind.ARRAY
    value.data = changetype<u32>(input) as u64
    return value
  }

  static fromBigInt(n: BigInt): Value {
    let value = new Value()
    value.kind = ValueKind.BIG_INT
    value.data = changetype<u32>(n) as u64
    return value
  }

  static fromBoolean(b: boolean): Value {
    let value = new Value()
    value.kind = ValueKind.BOOL
    value.data = b ? 1 : 0
    return value
  }

  static fromBytes(bytes: Bytes): Value {
    let value = new Value()
    value.kind = ValueKind.BYTES
    value.data = changetype<u32>(bytes) as u64
    return value
  }

  static fromNull(): Value {
    let value = new Value()
    value.kind = ValueKind.NULL
    return value
  }

  static fromI32(n: i32): Value {
    let value = new Value()
    value.kind = ValueKind.INT
    value.data = n as u64
    return value
  }

  static fromString(s: string): Value {
    let value = new Value()
    value.kind = ValueKind.STRING
    value.data = changetype<u32>(s)
    return value
  }

  static fromBigDecimal(n: BigDecimal): Value {
    let value = new Value()
    value.kind = ValueKind.BIGDECIMAL
    value.data = n as u64
    return value
  }
}

/** An arbitrary size integer represented as an array of bytes. */
export class BigInt extends Uint8Array {
  toHex(): string {
    return typeConversion.bigIntToHex(this)
  }

  toHexString(): string {
    return typeConversion.bigIntToHex(this)
  }

  toString(): string {
    return typeConversion.bigIntToString(this)
  }

  static fromI32(x: i32): BigInt {
    return typeConversion.i32ToBigInt(x) as BigInt
  }

  toI32(): i32 {
    return typeConversion.bigIntToI32(this)
  }

  @operator('+')
  plus(other: BigInt): BigInt {
    return bigInt.plus(this, other)
  }

  @operator('-')
  minus(other: BigInt): BigInt {
    return bigInt.minus(this, other)
  }

  @operator('*')
  times(other: BigInt): BigInt {
    return bigInt.times(this, other)
  }

  @operator('/')
  div(other: BigInt): BigInt {
    return bigInt.dividedBy(this, other)
  }

  divDecimal(other: BigDecimal): BigDecimal {
    return bigInt.dividedByDecimal(this, other)
  }

  @operator('%')
  mod(other: BigInt): BigInt {
    return bigInt.mod(this, other)
  }

  @operator('==')
  equals(other: BigInt): boolean {
    if (this.length !== other.length) {
      return false;
    }
    for (let i = 0; i < this.length; i++) {
      if (this[i] !== other[i]) {
        return false;
      }
    }
    return true;
  }

  toBigDecimal(): BigDecimal {
    return new BigDecimal(this)
  }
}

export class BigDecimal {
  exp!: BigInt
  digits!: BigInt

  constructor(bigInt: BigInt) {
    this.digits = bigInt
    this.exp = BigInt.fromI32(0)
  }

  static fromString(s: string): BigDecimal {
    return bigDecimal.fromString(s)
  }

  toString(): string {
    return bigDecimal.toString(this)
  }

  truncate(decimals: i32): BigDecimal {
    let digitsRightOfZero = this.digits.toString().length + this.exp.toI32()
    let newDigitLength = decimals + digitsRightOfZero
    let truncateLength = this.digits.toString().length - newDigitLength
    if (truncateLength < 0) {
      return this
    } else {
      for (let i = 0; i < truncateLength; i++) {
        this.digits = this.digits.div(BigInt.fromI32(10))
      }
      this.exp = BigInt.fromI32(decimals * -1)
      return this
    }
  }

  @operator('+')
  plus(other: BigDecimal): BigDecimal {
    return bigDecimal.plus(this, other)
  }

  @operator('-')
  minus(other: BigDecimal): BigDecimal {
    return bigDecimal.minus(this, other)
  }

  @operator('*')
  times(other: BigDecimal): BigDecimal {
    return bigDecimal.times(this, other)
  }

  @operator('/')
  div(other: BigDecimal): BigDecimal {
    return bigDecimal.dividedBy(this, other)
  }

  @operator('==')
  equals(other: BigDecimal): boolean {
    return bigDecimal.equals(this, other)
  }
}

export enum TokenKind {
  ADDRESS = 0,
  FIXED_BYTES = 1,
  BYTES = 2,
  INT = 3,
  UINT = 4,
  BOOL = 5,
  STRING = 6,
  FIXED_ARRAY = 7,
  ARRAY = 8
}
export class Token {
  kind: TokenKind
  data: Payload
}

// Sequence of 4 `u64`s.
export type Int64 = Uint64Array;
export type Uint64 = Uint64Array;

/**
 * TypedMap entry.
 */
export class TypedMapEntry<K, V> {
  key: K
  value: V

  constructor(key: K, value: V) {
    this.key = key
    this.value = value
  }
}

/** Typed map */
export class TypedMap<K, V> {
  entries: Array<TypedMapEntry<K, V>>

  constructor() {
    this.entries = new Array<TypedMapEntry<K, V>>(0)
  }

  set(key: K, value: V): void {
    let entry = this.getEntry(key)
    if (entry !== null) {
      entry.value = value
    } else {
      let entry = new TypedMapEntry<K, V>(key, value)
      this.entries.push(entry)
    }
  }

  getEntry(key: K): TypedMapEntry<K, V> | null {
    for (let i: i32 = 0; i < this.entries.length; i++) {
      if (this.entries[i].key == key) {
        return this.entries[i]
      }
    }
    return null
  }

  get(key: K): V | null {
    for (let i: i32 = 0; i < this.entries.length; i++) {
      if (this.entries[i].key == key) {
        return this.entries[i].value
      }
    }
    return null
  }

  isSet(key: K): bool {
    for (let i: i32 = 0; i < this.entries.length; i++) {
      if (this.entries[i].key == key) {
        return true
      }
    }
    return false
  }
}

/**
 * Common representation for entity data, storing entity attributes
 * as `string` keys and the attribute values as dynamically-typed
 * `Value` objects.
 */
export class Entity extends TypedMap<string, Value> {
  unset(key: string): void {
    this.set(key, Value.fromNull())
  }

  /** Assigns properties from sources to this Entity in right-to-left order */
  merge(sources: Array<Entity>): Entity {
    var target = this
    for (let i = 0; i < sources.length; i++) {
      let entries = sources[i].entries
      for (let j = 0; j < entries.length; j++) {
        target.set(entries[j].key, entries[j].value)
      }
    }
    return target
  }
}

/** Type hint for JSON values. */
export enum JSONValueKind {
  NULL = 0,
  BOOL = 1,
  NUMBER = 2,
  STRING = 3,
  ARRAY = 4,
  OBJECT = 5,
}

/**
 * Pointer type for JSONValue data.
 *
 * Big enough to fit any pointer or native `this.data`.
 */
export type JSONValuePayload = u64
export class JSONValue {
  kind: JSONValueKind
  data: JSONValuePayload

  toString(): string {
    assert(this.kind == JSONValueKind.STRING, 'JSON value is not a string.')
    return changetype<string>(this.data as u32)
  }

  toObject(): TypedMap<string, JSONValue> {
    assert(this.kind == JSONValueKind.OBJECT, 'JSON value is not an object.')
    return changetype<TypedMap<string, JSONValue>>(this.data as u32)
  }
}

export class Wrapped<T> {
  inner: T;

  constructor(inner: T) {
    this.inner = inner;
  }
}

export class Result<V, E> {
  _value: Wrapped<V> | null;
  _error: Wrapped<E> | null;

  get isOk(): boolean {
    return this._value !== null;
  }

  get isError(): boolean {
    return this._error !== null;
  }

  get value(): V {
    assert(this._value != null, "Trying to get a value from an error result");
    return (this._value as Wrapped<V>).inner;
  }

  get error(): E {
    assert(
      this._error != null,
      "Trying to get an error from a successful result"
    );
    return (this._error as Wrapped<E>).inner;
  }
}

/**
 * Byte array
 */
class ByteArray extends Uint8Array {
  toHex(): string {
    return typeConversion.bytesToHex(this)
  }

  toHexString(): string {
    return typeConversion.bytesToHex(this)
  }

  toString(): string {
    return typeConversion.bytesToString(this)
  }

  toBase58(): string {
    return typeConversion.bytesToBase58(this)
  }
}
