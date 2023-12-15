import "allocator/arena";

export { memory };

/*
 * Declarations copied from graph-ts/input.ts and edited for brevity
 */

declare namespace store {
  function set(entity: string, id: string, data: Entity): void
}

class TypedMapEntry<K, V> {
  key: K
  value: V

  constructor(key: K, value: V) {
    this.key = key
    this.value = value
  }
}

class TypedMap<K, V> {
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
}

enum ValueKind {
  STRING = 0,
  INT = 1,
  FLOAT = 2,
  BOOL = 3,
  ARRAY = 4,
  NULL = 5,
  BYTES = 6,
  BIGINT = 7,
}

type ValuePayload = u64

class Value {
  kind: ValueKind
  data: ValuePayload

  toString(): string {
    assert(this.kind == ValueKind.STRING, 'Value is not a string.')
    return changetype<string>(this.data as u32)
  }

  static fromString(s: string): Value {
    let value = new Value()
    value.kind = ValueKind.STRING
    value.data = s as u64
    return value
  }
}

class Entity extends TypedMap<string, Value> { }

enum JSONValueKind {
  NULL = 0,
  BOOL = 1,
  NUMBER = 2,
  STRING = 3,
  ARRAY = 4,
  OBJECT = 5,
}

type JSONValuePayload = u64
class JSONValue {
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

/*
 * Actual setup for the test
 */
declare namespace ipfs {
  function map(hash: String, callback: String, userData: Value, flags: String[]): void
}

export function echoToStore(data: JSONValue, userData: Value): void {
  // expect a map of the form { "id": "anId", "value": "aValue" }
  let map = data.toObject();
  let id = map.get("id").toString();
  let value = map.get("value").toString();

  let entity = new Entity();
  entity.set("id", Value.fromString(id));
  entity.set("value", Value.fromString(value));
  entity.set("extra", userData);
  store.set("Thing", id, entity);
}

export function ipfsMap(hash: string, userData: string): void {
  ipfs.map(hash, "echoToStore", Value.fromString(userData), ["json"])
}
