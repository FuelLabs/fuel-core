type Address = Uint8Array;

export declare namespace ethereum {
  function call(call: Address): Array<Address> | null
}

export function callContract(address: Address): void {
  ethereum.call(address)
}