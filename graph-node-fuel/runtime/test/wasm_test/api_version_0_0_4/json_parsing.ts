import "allocator/arena";
export { memory };

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

/** Type hint for JSON values. */
export enum JSONValueKind {
  NULL = 0,
  BOOL = 1,
  NUMBER = 2,
  STRING = 3,
  ARRAY = 4,
  OBJECT = 5
}

/**
 * Pointer type for JSONValue data.
 *
 * Big enough to fit any pointer or native `this.data`.
 */
export type JSONValuePayload = u64;

export class JSONValue {
  kind: JSONValueKind;
  data: JSONValuePayload;

  toString(): string {
    assert(this.kind == JSONValueKind.STRING, "JSON value is not a string.");
    return changetype<string>(this.data as u32);
  }
}

export class Bytes extends Uint8Array {}

declare namespace json {
  function try_fromBytes(data: Bytes): Result<JSONValue, boolean>;
}

export function handleJsonError(data: Bytes): string {
  let result = json.try_fromBytes(data);
  if (result.isOk) {
    return "OK: " + result.value.toString() + ", ERROR: " + (result.isError ? "true" : "false");
  } else {
    return "ERROR: " + (result.error ? "true" : "false");
  }
}
