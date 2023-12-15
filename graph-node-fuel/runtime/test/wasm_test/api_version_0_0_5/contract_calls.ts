export * from './common/global'
import { Address } from './common/types'

export declare namespace ethereum {
  function call(call: Address): Array<Address> | null
}

export function callContract(address: Address): void {
  ethereum.call(address)
}
