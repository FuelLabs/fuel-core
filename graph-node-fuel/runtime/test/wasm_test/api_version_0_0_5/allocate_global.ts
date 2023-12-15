export * from './common/global'
import { BigInt } from './common/types'

let globalOne = bigInt.fromString("1")

declare namespace bigInt {
    function fromString(s: string): BigInt
}

export function assert_global_works(): void {
  let localOne = bigInt.fromString("1")
  assert(globalOne != localOne)
}
