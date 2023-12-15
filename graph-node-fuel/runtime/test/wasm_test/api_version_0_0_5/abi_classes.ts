export * from './common/global'
import { Address, Uint8, FixedBytes, Bytes, Payload, Value } from './common/types'

// Clone the address to a new buffer, add 1 to the first and last bytes of the
// address and return the new address.
export function test_address(address: Address): Address {
  let new_address = address.subarray();

  // Add 1 to the first and last byte.
  new_address[0] += 1;
  new_address[address.length - 1] += 1;

  return changetype<Address>(new_address)
}

// Clone the Uint8 to a new buffer, add 1 to the first and last `u8`s and return
// the new Uint8
export function test_uint(address: Uint8): Uint8 {
  let new_address = address.subarray();

  // Add 1 to the first byte.
  new_address[0] += 1;

  return new_address
}

// Return the string repeated twice.
export function repeat_twice(original: string): string {
  return original.repeat(2)
}

// Concatenate two byte sequences into a new one.
export function concat(bytes1: Bytes, bytes2: FixedBytes): Bytes {
  let concated_buff = new ArrayBuffer(bytes1.byteLength + bytes2.byteLength);
  let concated_buff_ptr = changetype<usize>(concated_buff);

  let bytes1_ptr = changetype<usize>(bytes1);
  let bytes1_buff_ptr = load<usize>(bytes1_ptr);

  let bytes2_ptr = changetype<usize>(bytes2);
  let bytes2_buff_ptr = load<usize>(bytes2_ptr);

  // Move bytes1.
  memory.copy(concated_buff_ptr, bytes1_buff_ptr, bytes1.byteLength);
  concated_buff_ptr += bytes1.byteLength

  // Move bytes2.
  memory.copy(concated_buff_ptr, bytes2_buff_ptr, bytes2.byteLength);

  let new_typed_array = Uint8Array.wrap(concated_buff);

  return changetype<Bytes>(new_typed_array);
}

export function test_array(strings: Array<string>): Array<string> {
  strings.push("5")
  return strings
}

export function byte_array_third_quarter(bytes: Uint8Array): Uint8Array {
  return bytes.subarray(bytes.length * 2/4, bytes.length * 3/4)
}
