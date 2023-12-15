import { BigInt, Bytes } from '@graphprotocol/graph-ts'
import { Trigger, Contract } from '../generated/Contract/Contract'
import { Call } from '../generated/schema'

export function handleTrigger(event: Trigger): void {
  let contract = Contract.bind(event.address)

  // The contract can only handle numbers < 10, so this call
  // should revert
  let call1 = new Call('100')
  let result1 = contract.try_inc(BigInt.fromI32(100))
  if (result1.reverted) {
    call1.reverted = true
  } else {
    call1.reverted = false
    call1.returnValue = result1.value
  }
  call1.save()

  // This call should no revert, as the value is < 10
  let call2 = new Call('9')
  let result2 = contract.try_inc(BigInt.fromI32(9))
  if (result2.reverted) {
    call2.reverted = true
  } else {
    call2.reverted = false
    call2.returnValue = result2.value
  }
  call2.save()
}
