export * from './common/global'
import { TypedMap, Entity, BigDecimal, Value } from './common/types'

/** Definitions copied from graph-ts/index.ts */
declare namespace store {
  function get(entity: string, id: string): Entity | null
  function set(entity: string, id: string, data: Entity): void
  function remove(entity: string, id: string): void
}

/** Host Ethereum interface */
declare namespace ethereum {
  function call(call: SmartContractCall): Array<EthereumValue>
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

/** Host interface for BigDecimal */
declare namespace bigDecimal {
  function plus(x: BigDecimal, y: BigDecimal): BigDecimal
  function minus(x: BigDecimal, y: BigDecimal): BigDecimal
  function times(x: BigDecimal, y: BigDecimal): BigDecimal
  function dividedBy(x: BigDecimal, y: BigDecimal): BigDecimal
  function equals(x: BigDecimal, y: BigDecimal): boolean
  function toString(bigDecimal: BigDecimal): string
  function fromString(s: string): BigDecimal
}

/**
 * Test functions
 */
export function getUser(id: string): Entity | null {
  return store.get("User", id);
}

export function loadAndSetUserName(id: string, name: string) : void {
  let user = store.get("User", id);
  if (user == null) {
    user = new Entity();
    user.set("id", Value.fromString(id));
  }
  user.set("name", Value.fromString(name));
  // save it
  store.set("User", id, (user as Entity));
}
