export * from './common/global'
import { Entity, BigDecimal, Value, BigInt } from './common/types'

/** Definitions copied from graph-ts/index.ts */
declare namespace store {
  function get(entity: string, id: string): Entity | null
  function set(entity: string, id: string, data: Entity): void
  function remove(entity: string, id: string): void
}

/** Host interface for BigInt arithmetic */
declare namespace bigInt {
  function plus(x: BigInt, y: BigInt): BigInt
  function minus(x: BigInt, y: BigInt): BigInt
  function times(x: BigInt, y: BigInt): BigInt
  function dividedBy(x: BigInt, y: BigInt): BigInt
  function dividedByDecimal(x: BigInt, y: BigDecimal): BigDecimal
  function mod(x: BigInt, y: BigInt): BigInt
}

/**
 * Test functions
 */
export function bigIntWithLength(bytes: u32): void {
  let user = new Entity();
  user.set("id", Value.fromString("jhon"));

  let array = new Uint8Array(bytes);
  array.fill(127);
  let big_int = changetype<BigInt>(array);
  user.set("count", Value.fromBigInt(big_int));
  store.set("User", "jhon", user);
}
