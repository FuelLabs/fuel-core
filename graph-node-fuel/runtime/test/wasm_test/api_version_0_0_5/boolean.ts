export * from "./common/global"

export function testReceiveTrue(a: bool): void {
  assert(a)
}

export function testReceiveFalse(a: bool): void {
  assert(!a)
}

export function testReturnTrue(): bool {
  return true
}

export function testReturnFalse(): bool {
  return false
}
