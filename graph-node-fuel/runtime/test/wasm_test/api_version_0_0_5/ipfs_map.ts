export * from './common/global'
import { Value, ValueKind, TypedMapEntry, TypedMap, Entity, JSONValueKind, JSONValue } from './common/types'

/*
 * Declarations copied from graph-ts/input.ts and edited for brevity
 */

declare namespace store {
  function set(entity: string, id: string, data: Entity): void
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

  let id = map.get("id");
  let value = map.get("value");

  assert(id !== null, "'id' should not be null");
  assert(value !== null, "'value' should not be null");

  let stringId = id!.toString();
  let stringValue = value!.toString();

  let entity = new Entity();
  entity.set("id", Value.fromString(stringId));
  entity.set("value", Value.fromString(stringValue));
  entity.set("extra", userData);
  store.set("Thing", stringId, entity);
}

export function ipfsMap(hash: string, userData: string): void {
  ipfs.map(hash, "echoToStore", Value.fromString(userData), ["json"])
}
