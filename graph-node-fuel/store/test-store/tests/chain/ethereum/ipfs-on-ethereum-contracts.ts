export enum ValueKind {
  STRING = 0,
  INT = 1,
  BIGDECIMAL = 2,
  BOOL = 3,
  ARRAY = 4,
  NULL = 5,
  BYTES = 6,
  BIGINT = 7,
}

export type ValuePayload = u64;

export class Value {
  constructor(public kind: ValueKind, public data: ValuePayload) {}
}

declare namespace ipfs {
  function cat(hash: String): void
  function map(hash: String, callback: String, userData: Value, flags: String[]): void
}

export function foo(): void {
    let value = new Value(0, 0)
    ipfs.cat("")
    ipfs.map("", "", value, [])
}
